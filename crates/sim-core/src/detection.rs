//! **Gate 1 interim radar detection model.** Deliberately minimal; Phase 2 replaces it with the full
//! sensor models in a `sim-sensors` crate. Model doc: `docs/models/radar-detection-gate1.md`.
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
//! Detections create or refresh a contact in the observer side's picture. A contact not refreshed
//! for [`CONTACT_TIMEOUT_TICKS`] is dropped.
//!
//! Not modelled yet: clutter, jamming, weather, terrain masking, ESM, sonar, visual/EO/IR,
//! position error, classification, track correlation, data-link sharing.

use crate::components::{Domain, Position, Unit};
use crate::events::{EventLog, SimEventKind};
use crate::ids::{SideId, UnitId};
use crate::rng::{RngStreams, StreamId};
use crate::time::{SimClock, Tick};
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use sim_geo::{inverse, radar_horizon_m, GeoPoint};
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

/// Sensor kinds, matching the platform database `sensors.kind` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum SensorKind {
    Radar,
    SonarHull,
    SonarTowed,
    SonarDipping,
    Sonobuoy,
    Esm,
    Ir,
    Eo,
    Visual,
    Mad,
}

impl SensorKind {
    /// Parses a platform-database sensor kind.
    #[must_use]
    pub fn from_db(kind: &str) -> Option<Self> {
        Some(match kind {
            "radar" => Self::Radar,
            "sonar_hull" => Self::SonarHull,
            "sonar_towed" => Self::SonarTowed,
            "sonar_dipping" => Self::SonarDipping,
            "sonobuoy" => Self::Sonobuoy,
            "esm" => Self::Esm,
            "ir" => Self::Ir,
            "eo" => Self::Eo,
            "visual" => Self::Visual,
            "mad" => Self::Mad,
            _ => return None,
        })
    }

    pub(crate) fn code(self) -> u8 {
        self as u8
    }
}

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

/// R₅₀: the range at which single-scan Pd is 0.5, scaled from the radar's reference value by
/// the fourth root of the RCS ratio and capped at the radar's maximum range.
#[must_use]
pub fn detection_range_m(ref_range_m: f64, ref_rcs_m2: f64, rcs_m2: f64, max_range_m: f64) -> f64 {
    // (σ/σ_ref)^¼ as two square roots: sqrt is correctly rounded, so this is bit-exact everywhere.
    let scale = (rcs_m2.max(0.0) / ref_rcs_m2).sqrt().sqrt();
    (ref_range_m * scale).min(max_range_m)
}

/// Single-scan probability of detection at range `r` for a radar whose R₅₀ is `r50`.
#[must_use]
pub fn pd_per_scan(range_m: f64, r50_m: f64) -> f64 {
    if r50_m <= 0.0 {
        return 0.0;
    }
    let x = range_m / r50_m;
    let x2 = x * x;
    let x4 = x2 * x2;
    1.0 / (1.0 + x4 * x4)
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
    /// Position at the last detection. Phase 1 has no position error.
    pub position: GeoPoint,
    pub detected_by: UnitId,
    pub sensor_id: String,
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
    /// Creates or refreshes a contact. Returns the new contact number if it was created.
    fn record(
        &mut self,
        side: SideId,
        target: UnitId,
        tick: Tick,
        position: GeoPoint,
        observer: UnitId,
        sensor_id: &str,
    ) -> Option<u32> {
        let contacts = self.by_side.entry(side).or_default();
        if let Some(c) = contacts.get_mut(&target) {
            c.last_seen = tick;
            c.position = position;
            c.detected_by = observer;
            sensor_id.clone_into(&mut c.sensor_id);
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
                position,
                detected_by: observer,
                sensor_id: sensor_id.to_owned(),
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
}

pub(crate) fn detection_system(
    clock: Res<SimClock>,
    mut rng: ResMut<RngStreams>,
    mut contacts: ResMut<ContactTable>,
    mut log: ResMut<EventLog>,
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
                let range = inverse(obs.position.point, tgt.position.point).distance_m;
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
                    },
                );
                if detected {
                    if let Some(number) = contacts.record(
                        obs.side,
                        tgt.id,
                        tick,
                        tgt.position.point,
                        obs.id,
                        &sensor.sensor_id,
                    ) {
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

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn pd_curve_hand_values() {
        assert!(close(pd_per_scan(100.0, 100.0), 0.5, 1e-15));
        // x = 0.8: x⁸ = 0.16777216 → 1/1.16777216 = 0.856329…
        assert!(close(pd_per_scan(80.0, 100.0), 1.0 / 1.167_772_16, 1e-12));
        // x = 1.2: x⁸ = 4.29981696 → 0.188686…
        assert!(close(pd_per_scan(120.0, 100.0), 1.0 / 5.299_816_96, 1e-12));
        assert!(close(pd_per_scan(0.0, 100.0), 1.0, 0.0));
        assert_eq!(pd_per_scan(10.0, 0.0), 0.0);
    }

    #[test]
    fn detection_range_scales_with_fourth_root_of_rcs() {
        // 16× the reference RCS doubles R₅₀: 120 km → 240 km.
        assert!(close(
            detection_range_m(120_000.0, 1.0, 16.0, 1e9),
            240_000.0,
            1e-6
        ));
        // 1/16 halves it.
        assert!(close(
            detection_range_m(120_000.0, 1.0, 1.0 / 16.0, 1e9),
            60_000.0,
            1e-6
        ));
        // Capped at the instrumented maximum.
        assert!(close(
            detection_range_m(120_000.0, 1.0, 10_000.0, 250_000.0),
            250_000.0,
            0.0
        ));
    }

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

    #[test]
    fn sensor_kinds_round_trip_from_database_names() {
        for k in [
            "radar",
            "sonar_hull",
            "sonar_towed",
            "sonar_dipping",
            "sonobuoy",
            "esm",
            "ir",
            "eo",
            "visual",
            "mad",
        ] {
            assert!(SensorKind::from_db(k).is_some(), "{k}");
        }
        assert_eq!(SensorKind::from_db("laser"), None);
    }
}
