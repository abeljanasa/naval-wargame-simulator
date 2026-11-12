//! Visual, EO/IR and MAD pass of the detection system. The model is in `sim_sensors::optical`
//! (ADR-0005); model doc `docs/models/optical-mad.md`.
//!
//! Order (fixed for determinism, ADR-0003): observers in `UnitId` order, their sensors in mount
//! order, targets of other sides in `UnitId` order. One `Sensors` draw per pair in range; a
//! detection takes two more for its measurement error.
//!
//! * **Visual:** range and bearing (a position measurement), with identification by range.
//! * **EO / IR:** a precise bearing (pooled with ESM and sonar bearings), with identification.
//! * **MAD:** a submerged submarine within about a kilometre of slant range; a coarse position.

use crate::components::Domain;
use crate::detection::{
    sensor_height_m, target_height_m, ContactTable, Measurement, Observed, SensorKind,
};
use crate::esm::{note_bearing, BearingTable, FreshBearings};
use crate::events::{EventLog, SimEventKind};
use crate::rng::{RngStreams, StreamId};
use crate::time::Tick;
use sim_geo::{chord_m, direct, inverse, normalize_azimuth_deg, radar_horizon_m};
use sim_sensors::{
    gaussian_pair, light, mad_r50_m, measured_point, measurement_error, optical_identification,
    optical_r50_m, pd_per_scan, size_factor, slant_range_m, sun_elevation_deg, BearingLine,
    Classification, Cov2, Environment, Identification, Light, MAD_POSITION_SIGMA_M,
};

const MIN_METRES_PER_DEG_LAT: f64 = 110_000.0;

/// Length assumed for a unit without a platform figure, metres.
#[must_use]
pub fn default_length_m(domain: Domain) -> f64 {
    match domain {
        Domain::Surface => 100.0,
        Domain::Subsurface => 70.0,
        Domain::Air => 15.0,
        Domain::Land => 10.0,
    }
}

fn is_optical(kind: SensorKind) -> bool {
    matches!(kind, SensorKind::Visual | SensorKind::Eo | SensorKind::Ir)
}

fn light_code(l: Light) -> u8 {
    match l {
        Light::Day => 0,
        Light::Twilight => 1,
        Light::Night => 2,
    }
}

fn identification(level: Classification, tgt: &Observed<'_>) -> Identification {
    Identification {
        classification: level,
        platform: (level >= Classification::Class)
            .then(|| tgt.platform.map(str::to_owned))
            .flatten(),
        category: (level >= Classification::Category)
            .then(|| tgt.category.map(str::to_owned))
            .flatten(),
    }
}

/// Candidate target indices of other sides within `reach` of the observer's latitude, in
/// `UnitId` order.
fn candidates(
    by_lat: &[(f64, usize)],
    all: &[Observed<'_>],
    obs: &Observed<'_>,
    reach: f64,
    out: &mut Vec<usize>,
) {
    let lat0 = obs.position.point.lat_deg;
    let band = reach / MIN_METRES_PER_DEG_LAT;
    let lo = by_lat.partition_point(|&(lat, _)| lat < lat0 - band);
    let hi = by_lat.partition_point(|&(lat, _)| lat <= lat0 + band);
    out.clear();
    out.extend(
        by_lat[lo..hi]
            .iter()
            .map(|&(_, i)| i)
            .filter(|&i| all[i].side != obs.side),
    );
    out.sort_unstable();
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub(crate) fn optical_pass(
    all: &[Observed<'_>],
    ecef: &[[f64; 3]],
    tick: Tick,
    env: Environment,
    rng: &mut RngStreams,
    contacts: &mut ContactTable,
    log: &mut EventLog,
    bearings: &mut BearingTable,
    fresh: &mut FreshBearings,
    terrain: Option<&sim_geo::terrain::Terrain>,
) {
    let mut by_lat: Vec<(f64, usize)> = all
        .iter()
        .enumerate()
        .map(|(i, t)| (t.position.point.lat_deg, i))
        .collect();
    by_lat.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    #[allow(clippy::cast_precision_loss)]
    let unix_s = env.start_unix_s as f64 + tick.seconds();
    let visibility_m = env.visibility_km * 1000.0;
    let mut cands: Vec<usize> = Vec::new();
    for (oi, obs) in all.iter().enumerate() {
        for s in &obs.sensors.0 {
            if is_optical(s.kind) {
                let Some(h_s) =
                    sensor_height_m(s.mount_height_m, obs.domain, obs.position.altitude_m)
                else {
                    continue;
                };
                let lt = light(sun_elevation_deg(
                    unix_s,
                    obs.position.point.lat_deg,
                    obs.position.point.lon_deg,
                ));
                candidates(&by_lat, all, obs, s.max_range_m, &mut cands);
                for &i in &cands {
                    let tgt = &all[i];
                    // Submerged targets cannot be seen.
                    let Some(h_t) = target_height_m(tgt.domain, tgt.position.altitude_m) else {
                        continue;
                    };
                    if tgt.position.altitude_m > s.max_target_altitude_m {
                        continue;
                    }
                    let horizon = radar_horizon_m(h_s, h_t);
                    let bound = s.max_range_m.min(horizon);
                    if chord_m(ecef[oi], ecef[i]) > bound + 1.0 {
                        continue;
                    }
                    let inv = inverse(obs.position.point, tgt.position.point);
                    let range = inv.distance_m;
                    if range < s.min_range_m || range > bound {
                        continue;
                    }
                    // Terrain masking: land between the sensor and the target.
                    if crate::detection::terrain_masks(
                        terrain,
                        obs.position.point,
                        h_s + obs.ground_m,
                        tgt.position.point,
                        h_t + tgt.ground_m,
                        range,
                    ) {
                        continue;
                    }
                    let size = size_factor(s.kind, tgt.domain == Domain::Air, tgt.length_m);
                    let r50 = optical_r50_m(s.kind, s.max_range_m, size, lt, visibility_m);
                    let pd = pd_per_scan(range, r50);
                    let draw = rng.uniform(StreamId::Sensors);
                    let detected = draw < pd;
                    let level = optical_identification(range, r50);
                    let err = measurement_error(s.kind);
                    let normals = detected.then(|| {
                        gaussian_pair(
                            rng.uniform(StreamId::Sensors),
                            rng.uniform(StreamId::Sensors),
                        )
                    });
                    let (range_error, bearing_error) = match (normals, err.range_sigma_m) {
                        (Some((n1, n2)), Some(sr)) => {
                            (Some(n1 * sr), Some(n2 * err.bearing_sigma_deg))
                        }
                        (Some((n1, _)), None) => (None, Some(n1 * err.bearing_sigma_deg)),
                        _ => (None, None),
                    };
                    log.push(
                        tick,
                        SimEventKind::OpticalAttempt {
                            observer: obs.id,
                            sensor: s.sensor_id.clone(),
                            target: tgt.id,
                            range_m: range,
                            horizon_m: horizon,
                            r50_m: r50,
                            light: light_code(lt),
                            pd,
                            draw,
                            detected,
                            identified: detected.then_some(level),
                            range_error_m: range_error,
                            bearing_error_deg: bearing_error,
                        },
                    );
                    let Some(bearing_err) = bearing_error else {
                        continue;
                    };
                    let ident = identification(level, tgt);
                    if let (Some(sr), Some(range_err)) = (err.range_sigma_m, range_error) {
                        let (point, cov, _, _) = measured_point(
                            obs.position.point,
                            range,
                            inv.azimuth1_deg,
                            sr,
                            err.bearing_sigma_deg,
                            range_err / sr,
                            bearing_err / err.bearing_sigma_deg,
                        );
                        let m = Measurement {
                            point,
                            cov,
                            classification: ident.classification,
                            category: ident.category,
                            platform: ident.platform,
                            domain: tgt.domain,
                            observer: obs.id,
                            sensor_id: &s.sensor_id,
                        };
                        if let Some(number) = contacts.record(obs.side, tgt.id, tick, &m) {
                            log.push(
                                tick,
                                SimEventKind::ContactGained {
                                    side: obs.side,
                                    number,
                                    target: tgt.id,
                                },
                            );
                        }
                    } else {
                        let line = BearingLine {
                            from: obs.position.point,
                            bearing_deg: normalize_azimuth_deg(inv.azimuth1_deg + bearing_err),
                            sigma_deg: err.bearing_sigma_deg,
                        };
                        let label = if s.kind == SensorKind::Ir { "IR" } else { "EO" };
                        note_bearing(
                            fresh, bearings, obs.side, obs.id, tgt.id, i, line, tick, label, ident,
                        );
                    }
                }
            } else if s.kind == SensorKind::Mad && obs.domain == Domain::Air {
                candidates(&by_lat, all, obs, s.max_range_m, &mut cands);
                for &i in &cands {
                    let tgt = &all[i];
                    if tgt.domain != Domain::Subsurface {
                        continue;
                    }
                    if chord_m(ecef[oi], ecef[i]) > s.max_range_m + 1.0 {
                        continue;
                    }
                    let inv = inverse(obs.position.point, tgt.position.point);
                    let slant = slant_range_m(
                        inv.distance_m,
                        obs.position.altitude_m - tgt.position.altitude_m,
                    );
                    if slant > s.max_range_m {
                        continue;
                    }
                    let r50 = mad_r50_m(s.max_range_m);
                    let pd = pd_per_scan(slant, r50);
                    let draw = rng.uniform(StreamId::Sensors);
                    let detected = draw < pd;
                    log.push(
                        tick,
                        SimEventKind::MadAttempt {
                            observer: obs.id,
                            target: tgt.id,
                            slant_range_m: slant,
                            r50_m: r50,
                            pd,
                            draw,
                            detected,
                        },
                    );
                    if !detected {
                        continue;
                    }
                    // "About here": the true position with an isotropic 300 m error.
                    let (n_east, n_north) = gaussian_pair(
                        rng.uniform(StreamId::Sensors),
                        rng.uniform(StreamId::Sensors),
                    );
                    let offset = libm::hypot(n_east, n_north) * MAD_POSITION_SIGMA_M;
                    let point = if offset > 0.0 {
                        direct(
                            tgt.position.point,
                            libm::atan2(n_east, n_north).to_degrees(),
                            offset,
                        )
                        .point
                    } else {
                        tgt.position.point
                    };
                    let m = Measurement {
                        point,
                        cov: Cov2::isotropic(MAD_POSITION_SIGMA_M),
                        classification: Classification::Domain,
                        category: None,
                        platform: None,
                        domain: tgt.domain,
                        observer: obs.id,
                        sensor_id: &s.sensor_id,
                    };
                    if let Some(number) = contacts.record(obs.side, tgt.id, tick, &m) {
                        log.push(
                            tick,
                            SimEventKind::ContactGained {
                                side: obs.side,
                                number,
                                target: tgt.id,
                            },
                        );
                    }
                }
            }
        }
    }
}
