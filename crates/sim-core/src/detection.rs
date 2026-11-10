//! Detection and tracking system. It runs the radar model (Gate 1 interim, still in use until
//! the Phase 2 radar slice) and the track model, both from `sim-sensors` (ADR-0005). Model docs:
//! `docs/models/radar-detection-gate1.md` and `docs/models/tracks.md`.
//!
//! Every [`SENSOR_PERIOD_TICKS`] ticks, for each observer (in `UnitId` order), each radar it carries,
//! and each unit of another side (in `UnitId` order):
//!
//! 1. Skip it if the geometry rules out detection: the target is submerged, outside the radar's
//!    [min, max] range or above its altitude coverage, or beyond the 4/3-earth radar horizon
//!    between mount and target heights.
//! 2. R₅₀ = R_ref · (σ / σ_ref)^¼, capped at the radar's maximum range. R₅₀ is the range at which
//!    single-scan detection probability is 0.5.
//! 3. Pd = 1 / (1 + (r / R₅₀)⁸).
//! 4. Draw u ~ U[0,1) from the `Sensors` RNG stream. The target is detected if u < Pd. Every draw is
//!    logged with its inputs ([`SimEventKind::DetectionAttempt`]).
//!
//! A detection measures the target's position with range and bearing error (two more `Sensors`
//! draws, logged), and the measurement is fused into the observer side's track on that target
//! (`docs/models/tracks.md`). A track's error grows between updates by the fastest speed its
//! classification allows. A track not refreshed for [`CONTACT_TIMEOUT_TICKS`] is dropped.
//!
//! Not modelled yet: clutter, jamming, weather, terrain masking, ESM, sonar, visual/EO/IR,
//! velocity estimation, association errors (tracks are keyed by the true target), data links.

use crate::c2::C2State;
use crate::components::{Domain, Position, Unit};
use crate::events::{EventLog, SimEventKind};
use crate::ids::{SideId, UnitId};
use crate::rng::{RngStreams, StreamId};
use crate::time::{SimClock, Tick};
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use sim_c2::Emcon;
use sim_geo::{inverse, radar_horizon_m, GeoPoint};
pub use sim_sensors::{detection_range_m, pd_per_scan, SensorKind};
use sim_sensors::{
    gaussian_pair, measured_point, measurement_error, radar_classification, Classification, Cov2,
    ErrorEllipse, TrackEstimate,
};
use std::collections::BTreeMap;

/// Sensors run on every 5th tick (master prompt §6.2).
pub const SENSOR_PERIOD_TICKS: u64 = 5;
/// A contact that has not been re-detected for this many ticks (5 min) is dropped.
pub const CONTACT_TIMEOUT_TICKS: u64 = 300;
/// Height of a surface ship's radar-reflecting superstructure above the sea, for the horizon check.
pub const SURFACE_TARGET_HEIGHT_M: f64 = 10.0;
/// Height of a land target above ground, for the horizon check.
pub const LAND_TARGET_HEIGHT_M: f64 = 5.0;
/// Height of a surfaced submarine's sail, for the horizon check.
pub const SURFACED_SUBMARINE_HEIGHT_M: f64 = 2.0;
/// A lower bound on metres per degree of geodetic latitude anywhere on WGS-84. The minimum
/// meridional radius a(1−e²) = 6 335 439 m gives 110 574 m/°; rounding down keeps it a strict bound.
const MIN_METRES_PER_DEG_LAT: f64 = 110_000.0;

/// One fitted sensor, with the values the detection model needs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SensorSpec {
    pub sensor_id: String,
    pub kind: SensorKind,
    /// Above waterline / ground, or above the airframe for aircraft. Negative = below the waterline.
    pub mount_height_m: f64,
    pub min_range_m: f64,
    pub max_range_m: f64,
    /// Radar: R₅₀ against a target of `ref_rcs_m2`.
    pub ref_range_m: Option<f64>,
    pub ref_rcs_m2: Option<f64>,
    /// Highest target altitude the sensor covers.
    pub max_target_altitude_m: f64,
}

impl SensorSpec {
    pub(crate) fn validate(&self) -> Result<(), String> {
        let finite = [
            self.mount_height_m,
            self.min_range_m,
            self.max_range_m,
            self.max_target_altitude_m,
        ]
        .iter()
        .all(|v| v.is_finite());
        if !finite || self.min_range_m < 0.0 || self.max_range_m <= self.min_range_m {
            return Err(format!("sensor {} has invalid ranges", self.sensor_id));
        }
        let positive = |v: Option<f64>| v.is_none_or(|x| x.is_finite() && x > 0.0);
        if !positive(self.ref_range_m) || !positive(self.ref_rcs_m2) {
            return Err(format!(
                "sensor {} has an invalid radar reference",
                self.sensor_id
            ));
        }
        Ok(())
    }
}

/// The sensors a unit carries.
#[derive(Debug, Clone, Default, Component, Serialize, Deserialize)]
pub struct SensorSuite(pub Vec<SensorSpec>);

/// A unit's observable signatures.
#[derive(Debug, Clone, Copy, Component, Serialize, Deserialize)]
pub struct Signature {
    /// Radar cross-section, m².
    pub rcs_m2: f64,
}

/// RCS used when the platform database gives none (validation warns about those platforms).
#[must_use]
pub fn default_rcs_m2(domain: Domain) -> f64 {
    match domain {
        Domain::Surface => 1000.0,
        Domain::Subsurface | Domain::Land => 100.0,
        Domain::Air => 5.0,
    }
}

/// The fastest a target of this domain is assumed able to move, for track error growth.
/// Generic upper bounds (original design): 35 kn at sea, 30 m/s on land, Mach 1.8 in the air.
#[must_use]
pub fn domain_speed_bound_mps(domain: Option<Domain>) -> f64 {
    match domain {
        Some(Domain::Surface | Domain::Subsurface) => 18.0,
        Some(Domain::Land) => 30.0,
        Some(Domain::Air) | None => 600.0,
    }
}

/// Height used for the horizon check, or `None` if a radar cannot see this target at all.
#[must_use]
pub fn target_height_m(domain: Domain, altitude_m: f64) -> Option<f64> {
    match domain {
        Domain::Surface => Some(altitude_m.max(0.0) + SURFACE_TARGET_HEIGHT_M),
        Domain::Air => Some(altitude_m.max(0.0)),
        Domain::Land => Some(altitude_m.max(0.0) + LAND_TARGET_HEIGHT_M),
        Domain::Subsurface if altitude_m >= 0.0 => Some(SURFACED_SUBMARINE_HEIGHT_M),
        Domain::Subsurface => None,
    }
}

/// Radar antenna height above the surface, or `None` if the radar cannot be used (submerged).
#[must_use]
pub fn sensor_height_m(mount_height_m: f64, domain: Domain, altitude_m: f64) -> Option<f64> {
    match domain {
        Domain::Air => Some((mount_height_m + altitude_m.max(0.0)).max(0.0)),
        Domain::Subsurface if altitude_m < 0.0 => None,
        _ => Some(mount_height_m.max(0.0)),
    }
}

/// A side's contact on another side's unit. The holding side never sees the target's [`UnitId`];
/// snapshots expose only the contact number.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Contact {
    pub number: u32,
    pub target: UnitId,
    pub first_seen: Tick,
    pub last_seen: Tick,
    /// Estimated position at the last update.
    pub position: GeoPoint,
    /// Position covariance at the last update, m² (zero in v2 saves, which had no error model).
    #[serde(default)]
    pub cov: Cov2,
    #[serde(default)]
    pub classification: Classification,
    /// The target's domain, once the classification reaches [`Classification::Domain`].
    #[serde(default)]
    pub domain: Option<Domain>,
    pub detected_by: UnitId,
    pub sensor_id: String,
}

impl Contact {
    /// The estimate at `now`, with the error grown for the time since the last update.
    #[must_use]
    pub fn estimate_at(&self, now: Tick) -> TrackEstimate {
        let dt = now.0.saturating_sub(self.last_seen.0) as f64;
        TrackEstimate {
            position: self.position,
            cov: self.cov,
        }
        .predicted(domain_speed_bound_mps(self.domain), dt)
    }

    /// The one-sigma error ellipse at `now`.
    #[must_use]
    pub fn error_at(&self, now: Tick) -> ErrorEllipse {
        self.estimate_at(now).cov.ellipse()
    }
}

/// One detection, as the tracker receives it.
struct Measurement<'a> {
    point: GeoPoint,
    cov: Cov2,
    classification: Classification,
    domain: Domain,
    observer: UnitId,
    sensor_id: &'a str,
}

/// Every side's contacts.
#[derive(Debug, Clone, Default, Resource)]
pub struct ContactTable {
    pub(crate) next_number: BTreeMap<SideId, u32>,
    pub(crate) by_side: BTreeMap<SideId, BTreeMap<UnitId, Contact>>,
}

/// A contact with its holding side, as saved.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContactRecord {
    pub side: SideId,
    pub contact: Contact,
}

impl ContactTable {
    /// Creates a track or fuses the measurement into the existing one. Returns the new contact
    /// number if a track was created.
    fn record(
        &mut self,
        side: SideId,
        target: UnitId,
        tick: Tick,
        m: &Measurement<'_>,
    ) -> Option<u32> {
        let contacts = self.by_side.entry(side).or_default();
        if let Some(c) = contacts.get_mut(&target) {
            let post = c.estimate_at(tick).fused(m.point, m.cov);
            c.position = post.position;
            c.cov = post.cov;
            c.last_seen = tick;
            c.classification = c.classification.max(m.classification);
            if c.classification >= Classification::Domain {
                c.domain = Some(m.domain);
            }
            c.detected_by = m.observer;
            m.sensor_id.clone_into(&mut c.sensor_id);
            return None;
        }
        let next = self.next_number.entry(side).or_insert(1);
        let number = *next;
        *next += 1;
        contacts.insert(
            target,
            Contact {
                number,
                target,
                first_seen: tick,
                last_seen: tick,
                position: m.point,
                cov: m.cov,
                classification: m.classification,
                domain: (m.classification >= Classification::Domain).then_some(m.domain),
                detected_by: m.observer,
                sensor_id: m.sensor_id.to_owned(),
            },
        );
        Some(number)
    }

    /// Drops stale contacts. Returns (side, number, target) of each dropped one, in key order.
    fn expire(&mut self, now: Tick) -> Vec<(SideId, u32, UnitId)> {
        let mut lost = vec![];
        for (side, contacts) in &mut self.by_side {
            contacts.retain(|target, c| {
                let keep = now.0.saturating_sub(c.last_seen.0) <= CONTACT_TIMEOUT_TICKS;
                if !keep {
                    lost.push((*side, c.number, *target));
                }
                keep
            });
        }
        lost
    }

    /// All contacts, ordered by (side, target).
    pub fn iter(&self) -> impl Iterator<Item = (SideId, &Contact)> {
        self.by_side
            .iter()
            .flat_map(|(side, m)| m.values().map(move |c| (*side, c)))
    }

    pub(crate) fn records(&self) -> Vec<ContactRecord> {
        self.iter()
            .map(|(side, c)| ContactRecord {
                side,
                contact: c.clone(),
            })
            .collect()
    }

    pub(crate) fn next_numbers(&self) -> Vec<(SideId, u32)> {
        self.next_number.iter().map(|(s, n)| (*s, *n)).collect()
    }

    pub(crate) fn restore(records: Vec<ContactRecord>, next: Vec<(SideId, u32)>) -> Self {
        let mut t = Self {
            next_number: next.into_iter().collect(),
            by_side: BTreeMap::new(),
        };
        for r in records {
            t.by_side
                .entry(r.side)
                .or_default()
                .insert(r.contact.target, r.contact);
        }
        t
    }
}

struct Observed<'a> {
    id: UnitId,
    side: SideId,
    domain: Domain,
    position: Position,
    sensors: &'a SensorSuite,
    rcs_m2: f64,
    /// EMCON allows active emissions (radar).
    emits: bool,
}

pub(crate) fn detection_system(
    clock: Res<SimClock>,
    mut rng: ResMut<RngStreams>,
    mut contacts: ResMut<ContactTable>,
    mut log: ResMut<EventLog>,
    c2: Res<C2State>,
    units: Query<(&Unit, &Position, &SensorSuite, &Signature)>,
) {
    let tick = clock.tick;
    if !tick.is_multiple_of(SENSOR_PERIOD_TICKS) {
        return;
    }
    let mut all: Vec<Observed<'_>> = units
        .iter()
        .map(|(u, p, s, sig)| Observed {
            id: u.id,
            side: u.side,
            domain: u.domain,
            position: *p,
            sensors: s,
            rcs_m2: sig.rcs_m2,
            emits: c2.doctrine_for(u).emcon == Emcon::Active,
        })
        .collect();
    all.sort_by_key(|o| o.id);

    // Broad phase (performance only; results are bit-identical to checking every pair).
    // Radar-visible targets, sorted by latitude, plus the tallest target height this scan.
    let mut by_lat: Vec<(f64, usize)> = all
        .iter()
        .enumerate()
        .filter(|(_, t)| target_height_m(t.domain, t.position.altitude_m).is_some())
        .map(|(i, t)| (t.position.point.lat_deg, i))
        .collect();
    by_lat.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    let tallest = all
        .iter()
        .filter_map(|t| target_height_m(t.domain, t.position.altitude_m))
        .fold(0.0_f64, f64::max);
    let mut candidates: Vec<usize> = Vec::new();

    for obs in &all {
        // EMCON silent: radar does not transmit, so it detects nothing and draws nothing.
        if !obs.emits {
            continue;
        }
        for sensor in obs.sensors.0.iter().filter(|s| s.kind == SensorKind::Radar) {
            let (Some(ref_range), Some(ref_rcs)) = (sensor.ref_range_m, sensor.ref_rcs_m2) else {
                continue;
            };
            let Some(h_s) =
                sensor_height_m(sensor.mount_height_m, obs.domain, obs.position.altitude_m)
            else {
                continue;
            };
            // No target this scan can be seen beyond `reach`, so none can be detected outside
            // a latitude band of ±reach / (minimum metres per degree of latitude).
            let reach = sensor.max_range_m.min(radar_horizon_m(h_s, tallest));
            let lat0 = obs.position.point.lat_deg;
            let band = reach / MIN_METRES_PER_DEG_LAT;
            let lo = by_lat.partition_point(|&(lat, _)| lat < lat0 - band);
            let hi = by_lat.partition_point(|&(lat, _)| lat <= lat0 + band);
            candidates.clear();
            candidates.extend(
                by_lat[lo..hi]
                    .iter()
                    .map(|&(_, i)| i)
                    .filter(|&i| all[i].side != obs.side),
            );
            // `all` is in UnitId order, so index order = UnitId order: the draw order is unchanged.
            candidates.sort_unstable();
            for &i in &candidates {
                let tgt = &all[i];
                let Some(h_t) = target_height_m(tgt.domain, tgt.position.altitude_m) else {
                    continue;
                };
                if tgt.position.altitude_m > sensor.max_target_altitude_m {
                    continue;
                }
                // Exact per-pair lower bound: the geodesic is at least |Δlat| × the minimum
                // meridional radius, so this skips only pairs the range checks below would skip.
                let bound = sensor.max_range_m.min(radar_horizon_m(h_s, h_t));
                if (tgt.position.point.lat_deg - lat0).abs() * MIN_METRES_PER_DEG_LAT > bound {
                    continue;
                }
                let inv = inverse(obs.position.point, tgt.position.point);
                let range = inv.distance_m;
                if range < sensor.min_range_m || range > sensor.max_range_m {
                    continue;
                }
                let horizon = radar_horizon_m(h_s, h_t);
                if range > horizon {
                    continue;
                }
                let r50 = detection_range_m(ref_range, ref_rcs, tgt.rcs_m2, sensor.max_range_m);
                let pd = pd_per_scan(range, r50);
                let draw = rng.uniform(StreamId::Sensors);
                let detected = draw < pd;
                // A detection measures range and bearing with error: two more draws, always taken
                // in this order and only when detected.
                let measurement = detected.then(|| {
                    let err = measurement_error(SensorKind::Radar);
                    let (n_range, n_bearing) = gaussian_pair(
                        rng.uniform(StreamId::Sensors),
                        rng.uniform(StreamId::Sensors),
                    );
                    measured_point(
                        obs.position.point,
                        range,
                        inv.azimuth1_deg,
                        err.range_sigma_m.unwrap_or(0.0),
                        err.bearing_sigma_deg,
                        n_range,
                        n_bearing,
                    )
                });
                log.push(
                    tick,
                    SimEventKind::DetectionAttempt {
                        observer: obs.id,
                        sensor: sensor.sensor_id.clone(),
                        target: tgt.id,
                        range_m: range,
                        horizon_m: horizon,
                        r50_m: r50,
                        pd,
                        draw,
                        detected,
                        range_error_m: measurement.map(|m| m.2),
                        bearing_error_deg: measurement.map(|m| m.3),
                    },
                );
                if let Some((point, cov, _, _)) = measurement {
                    let m = Measurement {
                        point,
                        cov,
                        classification: radar_classification(),
                        domain: tgt.domain,
                        observer: obs.id,
                        sensor_id: &sensor.sensor_id,
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
    for (side, number, target) in contacts.expire(tick) {
        log.push(
            tick,
            SimEventKind::ContactLost {
                side,
                number,
                target,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn submerged_targets_and_radars_are_excluded() {
        assert_eq!(target_height_m(Domain::Subsurface, -50.0), None);
        assert_eq!(
            target_height_m(Domain::Subsurface, 0.0),
            Some(SURFACED_SUBMARINE_HEIGHT_M)
        );
        assert_eq!(sensor_height_m(6.0, Domain::Subsurface, -20.0), None);
        assert_eq!(sensor_height_m(6.0, Domain::Subsurface, 0.0), Some(6.0));
        assert_eq!(sensor_height_m(0.0, Domain::Air, 9000.0), Some(9000.0));
        assert_eq!(
            target_height_m(Domain::Surface, 0.0),
            Some(SURFACE_TARGET_HEIGHT_M)
        );
    }
}
