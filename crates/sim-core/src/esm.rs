//! ESM pass of the detection system: passive intercept of radiating radars and jammers, bearing
//! lines, multi-platform fixes and emitter identification. The model is in `sim_sensors::esm`
//! (ADR-0005); model doc `docs/models/esm.md`.
//!
//! Order (fixed for determinism, ADR-0003): observers in `UnitId` order, their ESM sensors in
//! mount order, emitters in (unit id, mount order) with a unit's jammer after its radars. Each
//! (ESM, emitter) pair inside range and horizon takes one `Sensors` draw; an intercept takes two
//! more for the bearing error (only the first normal is used).

use crate::detection::{
    sensor_height_m, target_height_m, ContactTable, Measurement, Observed, SensorKind,
};
use crate::events::{EventLog, SimEventKind};
use crate::ids::{SideId, UnitId};
use crate::rng::{RngStreams, StreamId};
use crate::time::Tick;
use bevy_ecs::prelude::Resource;
use serde::{Deserialize, Serialize};
use sim_geo::{chord_m, inverse, normalize_azimuth_deg, radar_horizon_m};
use sim_sensors::{
    gaussian_pair, identify, intercept_range_m, measurement_error, pd_per_scan, triangulate,
    BearingLine, Carrier, Classification, Identification,
};
use std::collections::BTreeMap;

/// Bearing lines older than this are no longer shown.
pub const BEARING_MAX_AGE_TICKS: u64 = 60;
/// Emitter name used for noise jammers (not in the emitter library).
pub const JAMMER_EMITTER: &str = "JAMMER";
/// A lower bound on metres per degree of latitude (as in the radar broad phase).
const MIN_METRES_PER_DEG_LAT: f64 = 110_000.0;

/// A side's latest bearing on a target from one observer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BearingRecord {
    pub line: BearingLine,
    pub tick: Tick,
    /// The intercepted emitter type (database sensor id, or `JAMMER`).
    pub emitter: String,
}

/// Recent bearing lines: side → target → observer.
#[derive(Debug, Clone, Default, Resource, Serialize, Deserialize)]
pub struct BearingTable {
    pub by_side: BTreeMap<SideId, BTreeMap<UnitId, BTreeMap<UnitId, BearingRecord>>>,
}

impl BearingTable {
    fn expire(&mut self, now: Tick) {
        for targets in self.by_side.values_mut() {
            for observers in targets.values_mut() {
                observers.retain(|_, r| now.0.saturating_sub(r.tick.0) <= BEARING_MAX_AGE_TICKS);
            }
            targets.retain(|_, o| !o.is_empty());
        }
        self.by_side.retain(|_, t| !t.is_empty());
    }

    /// Lines held by `side` (or every side), without the target's identity.
    pub(crate) fn views(&self, side: Option<SideId>, now: Tick) -> Vec<BearingView> {
        self.by_side
            .iter()
            .filter(|(s, _)| side.is_none_or(|v| v == **s))
            .flat_map(|(s, targets)| {
                targets.values().flat_map(move |observers| {
                    observers.iter().map(move |(o, r)| BearingView {
                        side: *s,
                        observer: *o,
                        line: r.line,
                        #[allow(clippy::cast_precision_loss)]
                        age_s: now.0.saturating_sub(r.tick.0) as f64,
                        emitter: r.emitter.clone(),
                    })
                })
            })
            .collect()
    }
}

/// A bearing line as shown to its side.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct BearingView {
    pub side: SideId,
    /// Own unit that took the bearing.
    pub observer: UnitId,
    pub line: BearingLine,
    pub age_s: f64,
    /// Intercepted emitter type.
    pub emitter: String,
}

/// Emitter type → the platform classes known to carry it (from the platform database).
#[derive(Debug, Clone, Default, Resource, Serialize, Deserialize)]
pub struct EmitterLibrary(pub BTreeMap<String, Vec<Carrier>>);

struct Emitter {
    unit: usize,
    name: String,
    ref_range_m: Option<f64>,
    height_m: f64,
}

struct Intercepts {
    target_index: usize,
    /// One line per observer (the first emitter it intercepted this scan).
    lines: Vec<(UnitId, BearingLine)>,
    best: Option<Identification>,
}

// The detection system's state, passed through unchanged.
#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
pub(crate) fn esm_pass(
    all: &[Observed<'_>],
    ecef: &[[f64; 3]],
    tick: Tick,
    rng: &mut RngStreams,
    contacts: &mut ContactTable,
    log: &mut EventLog,
    bearings: &mut BearingTable,
    library: &EmitterLibrary,
) {
    // Radiating emitters: radars and jammers of units whose EMCON is active.
    let mut emitters: Vec<Emitter> = Vec::new();
    for (i, o) in all.iter().enumerate() {
        if !o.emits {
            continue;
        }
        for s in o.sensors.0.iter().filter(|s| s.kind == SensorKind::Radar) {
            if let (Some(r), Some(h)) = (
                s.ref_range_m,
                sensor_height_m(s.mount_height_m, o.domain, o.position.altitude_m),
            ) {
                emitters.push(Emitter {
                    unit: i,
                    name: s.sensor_id.clone(),
                    ref_range_m: Some(r),
                    height_m: h,
                });
            }
        }
        if o.jammer.is_some() {
            if let Some(h) = target_height_m(o.domain, o.position.altitude_m) {
                emitters.push(Emitter {
                    unit: i,
                    name: JAMMER_EMITTER.to_owned(),
                    ref_range_m: None,
                    height_m: h,
                });
            }
        }
    }
    let mut fresh: BTreeMap<(SideId, UnitId), Intercepts> = BTreeMap::new();
    if !emitters.is_empty() {
        let mut by_lat: Vec<(f64, usize)> = emitters
            .iter()
            .enumerate()
            .map(|(e, em)| (all[em.unit].position.point.lat_deg, e))
            .collect();
        by_lat.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let tallest = emitters.iter().map(|e| e.height_m).fold(0.0_f64, f64::max);
        let sigma = measurement_error(SensorKind::Esm).bearing_sigma_deg;
        let mut candidates: Vec<usize> = Vec::new();
        for (obs_index, obs) in all.iter().enumerate() {
            for esm in obs.sensors.0.iter().filter(|s| s.kind == SensorKind::Esm) {
                let Some(h) =
                    sensor_height_m(esm.mount_height_m, obs.domain, obs.position.altitude_m)
                else {
                    continue;
                };
                // Broad phase, as for radar: exact, so results equal a check of every pair.
                let reach = esm.max_range_m.min(radar_horizon_m(h, tallest));
                let lat0 = obs.position.point.lat_deg;
                let band = reach / MIN_METRES_PER_DEG_LAT;
                let lo = by_lat.partition_point(|&(lat, _)| lat < lat0 - band);
                let hi = by_lat.partition_point(|&(lat, _)| lat <= lat0 + band);
                candidates.clear();
                candidates.extend(
                    by_lat[lo..hi]
                        .iter()
                        .map(|&(_, e)| e)
                        .filter(|&e| all[emitters[e].unit].side != obs.side),
                );
                // Emitter index order = (unit id, mount order): the draw order is fixed.
                candidates.sort_unstable();
                // A unit's emitters are consecutive: one geodesic per (observer, emitter unit).
                let mut cached: Option<(usize, sim_geo::Inverse)> = None;
                for &e in &candidates {
                    let em = &emitters[e];
                    let tgt = &all[em.unit];
                    let horizon = radar_horizon_m(h, em.height_m);
                    let bound = esm.max_range_m.min(horizon);
                    if (tgt.position.point.lat_deg - lat0).abs() * MIN_METRES_PER_DEG_LAT > bound {
                        continue;
                    }
                    // Exact: the chord is never longer than the geodesic (1 m rounding margin).
                    if chord_m(ecef[obs_index], ecef[em.unit]) > bound + 1.0 {
                        continue;
                    }
                    let inv = match cached {
                        Some((u, inv)) if u == em.unit => inv,
                        _ => {
                            let inv = inverse(obs.position.point, tgt.position.point);
                            cached = Some((em.unit, inv));
                            inv
                        }
                    };
                    let range = inv.distance_m;
                    if range < esm.min_range_m || range > esm.max_range_m || range > horizon {
                        continue;
                    }
                    let r_int = intercept_range_m(em.ref_range_m, esm.max_range_m);
                    let pd = pd_per_scan(range, r_int);
                    let draw = rng.uniform(StreamId::Sensors);
                    let detected = draw < pd;
                    let bearing_error = detected.then(|| {
                        let (n, _) = gaussian_pair(
                            rng.uniform(StreamId::Sensors),
                            rng.uniform(StreamId::Sensors),
                        );
                        n * sigma
                    });
                    log.push(
                        tick,
                        SimEventKind::EsmIntercept {
                            observer: obs.id,
                            sensor: esm.sensor_id.clone(),
                            emitter_unit: tgt.id,
                            emitter: em.name.clone(),
                            range_m: range,
                            horizon_m: horizon,
                            intercept_range_m: r_int,
                            pd,
                            draw,
                            detected,
                            bearing_error_deg: bearing_error,
                        },
                    );
                    let Some(err) = bearing_error else {
                        continue;
                    };
                    let line = BearingLine {
                        from: obs.position.point,
                        bearing_deg: normalize_azimuth_deg(inv.azimuth1_deg + err),
                        sigma_deg: sigma,
                    };
                    bearings
                        .by_side
                        .entry(obs.side)
                        .or_default()
                        .entry(tgt.id)
                        .or_default()
                        .insert(
                            obs.id,
                            BearingRecord {
                                line,
                                tick,
                                emitter: em.name.clone(),
                            },
                        );
                    let entry = fresh
                        .entry((obs.side, tgt.id))
                        .or_insert_with(|| Intercepts {
                            target_index: em.unit,
                            lines: Vec::new(),
                            best: None,
                        });
                    if !entry.lines.iter().any(|(o, _)| *o == obs.id) {
                        entry.lines.push((obs.id, line));
                    }
                    let ident = identify(library.0.get(&em.name).map_or(&[], Vec::as_slice));
                    if entry
                        .best
                        .as_ref()
                        .is_none_or(|b| ident.classification > b.classification)
                    {
                        entry.best = Some(ident);
                    }
                }
            }
        }
    }
    for ((side, target), i) in fresh {
        let domain = all[i.target_index].domain;
        let ident = i.best.unwrap_or(Identification {
            classification: Classification::Unknown,
            platform: None,
            category: None,
        });
        // A fix needs bearings from at least two observers taken this scan.
        let lines: Vec<BearingLine> = i.lines.iter().map(|(_, l)| *l).collect();
        if let Some((point, cov)) = triangulate(&lines) {
            #[allow(clippy::cast_possible_truncation)]
            log.push(
                tick,
                SimEventKind::EsmFix {
                    side,
                    target,
                    bearings: lines.len() as u32,
                },
            );
            let m = Measurement {
                point,
                cov,
                classification: ident.classification,
                category: ident.category.clone(),
                platform: ident.platform.clone(),
                domain,
                observer: i.lines[0].0,
                sensor_id: "ESM",
            };
            if let Some(number) = contacts.record(side, target, tick, &m) {
                log.push(
                    tick,
                    SimEventKind::ContactGained {
                        side,
                        number,
                        target,
                    },
                );
            }
        } else {
            // A bearing alone does not place the target, but it identifies an existing track.
            contacts.classify(side, target, &ident, domain);
        }
    }
    bearings.expire(tick);
}
