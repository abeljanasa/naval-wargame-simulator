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
use crate::components::{Domain, Kinematics, Position, Unit};
use crate::esm::{esm_pass, resolve_bearings, BearingTable, EmitterLibrary, FreshBearings};
use crate::events::{EventLog, SimEventKind};
use crate::ids::{SideId, UnitId};
use crate::optical::optical_pass;
use crate::rng::{RngStreams, StreamId};
use crate::sonar::sonar_pass;
use crate::time::{SimClock, Tick};
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use sim_c2::Emcon;
use sim_geo::terrain::Terrain;
use sim_geo::{chord_m, ecef_surface, inverse, radar_horizon_m, GeoPoint};
use sim_sensors::{
    clutter_loss_db, detection_range_with_loss_m, gaussian_pair, in_main_beam, jamming_to_noise,
    measured_point, measurement_error, pd_jammed, radar_classification, Classification,
    ClutterBackground, Cov2, Environment, ErrorEllipse, JammerSpec, TrackEstimate,
    CLUTTER_CEILING_M, DEFAULT_BEAMWIDTH_DEG, DEFAULT_CLUTTER_DB_PER_SEA_STATE,
};
pub use sim_sensors::{detection_range_m, pd_per_scan, SensorKind};
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
    /// Radar: signal-to-clutter loss per sea-state step, dB (default 1.5).
    #[serde(default)]
    pub clutter_db_per_sea_state: Option<f64>,
    /// Radar: two-way half-power beamwidth, degrees (default 2).
    #[serde(default)]
    pub beamwidth_deg: Option<f64>,
    /// Sonar: capable of active transmission (database `is_active`).
    #[serde(default)]
    pub active: bool,
    /// Sonar: its own acoustic figures from the database (`None`: generic figures by kind).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sonar: Option<sim_sensors::SonarFigures>,
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
        let clutter_ok = self
            .clutter_db_per_sea_state
            .is_none_or(|x| x.is_finite() && x >= 0.0);
        let beam_ok = self
            .beamwidth_deg
            .is_none_or(|x| x.is_finite() && x > 0.0 && x <= 360.0);
        if !clutter_ok || !beam_ok {
            return Err(format!(
                "sensor {} has an invalid clutter figure or beamwidth",
                self.sensor_id
            ));
        }
        if !positive(self.ref_range_m) || !positive(self.ref_rcs_m2) {
            return Err(format!(
                "sensor {} has an invalid radar reference",
                self.sensor_id
            ));
        }
        if let Some(f) = &self.sonar {
            let finite = [f.di_db, f.dt_db, f.source_level_db, f.self_noise_db]
                .into_iter()
                .all(|v| v.is_none_or(f64::is_finite));
            if !finite || !positive(f.frequency_khz) {
                return Err(format!(
                    "sensor {} has invalid sonar figures",
                    self.sensor_id
                ));
            }
        }
        Ok(())
    }
}

/// A unit's noise jammer, if it carries one. It radiates whenever the unit's EMCON is active.
#[derive(Debug, Clone, Copy, Default, Component, Serialize, Deserialize)]
pub struct Jammer(pub Option<JammerSpec>);

/// Scenario-wide sea state and weather loss (set by the scenario or the white cell).
#[derive(Debug, Clone, Copy, Default, Resource, Serialize, Deserialize)]
pub struct EnvState(pub Environment);

/// The sensors a unit carries.
#[derive(Debug, Clone, Default, Component, Serialize, Deserialize)]
pub struct SensorSuite(pub Vec<SensorSpec>);

/// A unit's observable signatures.
#[derive(Debug, Clone, Copy, Component, Serialize, Deserialize)]
pub struct Signature {
    /// Radar cross-section, m².
    pub rcs_m2: f64,
    /// Radiated noise at 10 kn, dB (sonar). `None` in saves before format 7: domain default.
    #[serde(default)]
    pub radiated_noise_db: Option<f64>,
    /// Length overall, metres (optical size). `None`: domain default.
    #[serde(default)]
    pub length_m: Option<f64>,
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

/// What a radar sees a target against: low targets compete with sea or land clutter. A low air
/// target over land (on the terrain packs) competes with land clutter; `altitude_m` is its height
/// above the ground or sea below it.
#[must_use]
pub fn clutter_background(domain: Domain, altitude_m: f64, over_land: bool) -> ClutterBackground {
    match domain {
        Domain::Land => ClutterBackground::Land,
        Domain::Surface | Domain::Subsurface => ClutterBackground::Sea,
        Domain::Air if altitude_m <= CLUTTER_CEILING_M && over_land => ClutterBackground::Land,
        Domain::Air if altitude_m <= CLUTTER_CEILING_M => ClutterBackground::Sea,
        Domain::Air => ClutterBackground::None,
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
    /// Platform category (e.g. `surface_combatant`), once classified to category.
    #[serde(default)]
    pub category: Option<String>,
    /// Platform class (e.g. `GEN-FFG`), once classified to class.
    #[serde(default)]
    pub platform: Option<String>,
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
pub(crate) struct Measurement<'a> {
    pub(crate) point: GeoPoint,
    pub(crate) cov: Cov2,
    pub(crate) classification: Classification,
    pub(crate) category: Option<String>,
    pub(crate) platform: Option<String>,
    pub(crate) domain: Domain,
    pub(crate) observer: UnitId,
    pub(crate) sensor_id: &'a str,
}

impl Contact {
    /// Climbs the classification ladder (never descends). Category and class come with the
    /// level that supports them.
    fn raise(
        &mut self,
        level: Classification,
        category: Option<&String>,
        platform: Option<&String>,
        domain: Domain,
    ) {
        if level > self.classification {
            self.classification = level;
            if level >= Classification::Category {
                self.category = category.cloned();
            }
            if level >= Classification::Class {
                self.platform = platform.cloned();
            }
        }
        if self.classification >= Classification::Domain {
            self.domain = Some(domain);
        }
    }
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
    pub(crate) fn record(
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
            c.raise(
                m.classification,
                m.category.as_ref(),
                m.platform.as_ref(),
                m.domain,
            );
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
                category: m
                    .category
                    .clone()
                    .filter(|_| m.classification >= Classification::Category),
                platform: m
                    .platform
                    .clone()
                    .filter(|_| m.classification >= Classification::Class),
                detected_by: m.observer,
                sensor_id: m.sensor_id.to_owned(),
            },
        );
        Some(number)
    }

    /// Raises an existing track's classification from an identification that carries no
    /// position (a single ESM bearing). Does nothing if the side holds no track on the target.
    pub(crate) fn classify(
        &mut self,
        side: SideId,
        target: UnitId,
        ident: &sim_sensors::Identification,
        domain: Domain,
    ) {
        if let Some(c) = self.by_side.get_mut(&side).and_then(|m| m.get_mut(&target)) {
            c.raise(
                ident.classification,
                ident.category.as_ref(),
                ident.platform.as_ref(),
                domain,
            );
        }
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

/// Ground height under a point on the terrain packs, metres above sea level (0 at sea or with
/// no packs).
pub(crate) fn ground_m(terrain: Option<&Terrain>, p: GeoPoint) -> f64 {
    terrain
        .and_then(|t| t.height_m(p))
        .map_or(0.0, |h| h.max(0.0))
}

/// Whether terrain blocks the line of sight between two points, heights above sea level
/// (`docs/models/terrain-masking.md`). Never without terrain packs.
pub(crate) fn terrain_masks(
    terrain: Option<&Terrain>,
    a: GeoPoint,
    h_a: f64,
    b: GeoPoint,
    h_b: f64,
    distance_m: f64,
) -> bool {
    terrain.is_some_and(|t| t.masks(a, h_a, b, h_b, distance_m))
}

/// The weather zones in force (`docs/models/weather.md`); empty: the scenario-wide weather only.
#[derive(Debug, Clone, Default, Resource)]
pub struct Weather(pub Vec<sim_events::WeatherZone>);

/// The scenario-wide environment with the sea state and visibility of the weather at `p`.
#[must_use]
pub fn local_env(env: Environment, zones: &[sim_events::WeatherZone], p: GeoPoint) -> Environment {
    if zones.is_empty() {
        return env;
    }
    let w = sim_events::weather_at(&env, zones, p);
    Environment {
        sea_state: w.sea_state,
        visibility_km: w.visibility_km,
        ..env
    }
}

pub(crate) struct Observed<'a> {
    pub(crate) id: UnitId,
    pub(crate) side: SideId,
    pub(crate) domain: Domain,
    pub(crate) position: Position,
    pub(crate) sensors: &'a SensorSuite,
    rcs_m2: f64,
    /// EMCON allows active emissions (radar, jammer).
    pub(crate) emits: bool,
    pub(crate) jammer: Option<JammerSpec>,
    pub(crate) speed_mps: f64,
    /// Radiated noise at 10 kn, dB.
    pub(crate) radiated_noise_db: f64,
    pub(crate) platform: Option<&'a str>,
    pub(crate) category: Option<&'a str>,
    pub(crate) length_m: f64,
    /// The ground under a land unit (terrain packs), added to its heights for terrain masking:
    /// a site's mast stands on its hill. 0 for every other domain.
    pub(crate) ground_m: f64,
}

// A bevy system takes its world access as parameters; grouping them would only hide that.
#[allow(clippy::too_many_arguments)]
pub(crate) fn detection_system(
    clock: Res<SimClock>,
    mut rng: ResMut<RngStreams>,
    mut contacts: ResMut<ContactTable>,
    mut log: ResMut<EventLog>,
    c2: Res<C2State>,
    planning: Res<crate::planning::PlanningLayer>,
    env: Res<EnvState>,
    mut bearings: ResMut<BearingTable>,
    library: Res<EmitterLibrary>,
    terrain: Res<crate::sim::TerrainRes>,
    weather: Res<Weather>,
    acoustics: Res<crate::acoustics::Acoustics>,
    units: Query<(
        &Unit,
        &Position,
        &SensorSuite,
        &Signature,
        &Jammer,
        &Kinematics,
    )>,
) {
    let env = env.0;
    let tick = clock.tick;
    if !tick.is_multiple_of(SENSOR_PERIOD_TICKS) {
        return;
    }
    let terrain = terrain.0.as_deref();
    let zones = weather.0.as_slice();
    let mut all: Vec<Observed<'_>> = units
        .iter()
        .map(|(u, p, s, sig, j, k)| Observed {
            id: u.id,
            side: u.side,
            domain: u.domain,
            position: *p,
            sensors: s,
            rcs_m2: sig.rcs_m2,
            emits: planning.doctrine_for(&c2, u, p.point).emcon == Emcon::Active,
            jammer: j.0,
            speed_mps: k.speed_mps,
            radiated_noise_db: sig
                .radiated_noise_db
                .unwrap_or_else(|| crate::sonar::default_radiated_noise_db(u.domain)),
            platform: u.platform.as_deref(),
            category: u.category.as_deref(),
            length_m: sig
                .length_m
                .unwrap_or_else(|| crate::optical::default_length_m(u.domain)),
            ground_m: if u.domain == Domain::Land {
                ground_m(terrain, p.point)
            } else {
                0.0
            },
        })
        .collect();
    all.sort_by_key(|o| o.id);
    // Surface ECEF positions for the exact chord pre-check (performance only).
    let ecef: Vec<[f64; 3]> = all.iter().map(|o| ecef_surface(o.position.point)).collect();

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
    // Radiating jammers, in UnitId order: (index into `all`, spec).
    let jammers: Vec<(usize, JammerSpec)> = all
        .iter()
        .enumerate()
        .filter(|(_, o)| o.emits)
        .filter_map(|(i, o)| o.jammer.map(|j| (i, j)))
        .collect();

    for (obs_index, obs) in all.iter().enumerate() {
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
            // Jammers of other sides within line of sight of this radar: (unit index, distance,
            // bearing from the radar, spec). Jamming only raises the noise floor, so the broad
            // phase below is unaffected.
            let in_view: Vec<(usize, f64, f64, JammerSpec)> = jammers
                .iter()
                .filter(|(i, _)| all[*i].side != obs.side)
                .filter_map(|&(i, spec)| {
                    let j = &all[i];
                    let h_j = target_height_m(j.domain, j.position.altitude_m)?;
                    let inv = inverse(obs.position.point, j.position.point);
                    (inv.distance_m <= radar_horizon_m(h_s, h_j)).then_some((
                        i,
                        inv.distance_m,
                        inv.azimuth1_deg,
                        spec,
                    ))
                })
                .collect();
            let beamwidth = sensor.beamwidth_deg.unwrap_or(DEFAULT_BEAMWIDTH_DEG);
            let clutter_per_step = sensor
                .clutter_db_per_sea_state
                .unwrap_or(DEFAULT_CLUTTER_DB_PER_SEA_STATE);
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
                // Exact: the chord is never longer than the geodesic (1 m margin for rounding).
                if chord_m(ecef[obs_index], ecef[i]) > bound + 1.0 {
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
                // Terrain masking (terrain packs): land between the radar and the target.
                if terrain_masks(
                    terrain,
                    obs.position.point,
                    h_s + obs.ground_m,
                    tgt.position.point,
                    h_t + tgt.ground_m,
                    range,
                ) {
                    continue;
                }
                let ground_below = if tgt.domain == Domain::Air {
                    ground_m(terrain, tgt.position.point)
                } else {
                    0.0
                };
                // Sea clutter from the sea where the target is; rain loss along the path.
                let clutter_db = clutter_loss_db(
                    clutter_background(
                        tgt.domain,
                        tgt.position.altitude_m - ground_below,
                        ground_below > 0.0,
                    ),
                    local_env(env, zones, tgt.position.point).sea_state,
                    clutter_per_step,
                );
                let weather_db = env.weather_loss_db
                    + sim_events::weather::path_rain_loss_db(
                        &env,
                        zones,
                        obs.position.point,
                        tgt.position.point,
                        range,
                    );
                let r50 = detection_range_with_loss_m(
                    ref_range,
                    ref_rcs,
                    tgt.rcs_m2,
                    sensor.max_range_m,
                    clutter_db + weather_db,
                );
                // Summed in UnitId order (fixed f64 reduction order, ADR-0003). A jammer on the
                // target itself is always in the main beam (self-screening).
                let jn: f64 = in_view
                    .iter()
                    .map(|&(j, dist, bearing, spec)| {
                        let main = j == i || in_main_beam(inv.azimuth1_deg, bearing, beamwidth);
                        jamming_to_noise(spec, dist, main)
                    })
                    .sum();
                let pd = pd_jammed(range, r50, jn);
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
                        clutter_db,
                        weather_db,
                        jn,
                        range_error_m: measurement.map(|m| m.2),
                        bearing_error_deg: measurement.map(|m| m.3),
                    },
                );
                if let Some((point, cov, _, _)) = measurement {
                    let m = Measurement {
                        point,
                        cov,
                        classification: radar_classification(),
                        category: None,
                        platform: None,
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
    let mut fresh = FreshBearings::new();
    esm_pass(
        &all,
        &ecef,
        tick,
        &mut rng,
        &mut log,
        &mut bearings,
        &library,
        &mut fresh,
        terrain,
    );
    sonar_pass(
        &all,
        &ecef,
        tick,
        env,
        &mut rng,
        &mut contacts,
        &mut log,
        &mut bearings,
        &mut fresh,
        zones,
        (&acoustics, terrain),
    );
    optical_pass(
        &all,
        &ecef,
        tick,
        env,
        &mut rng,
        &mut contacts,
        &mut log,
        &mut bearings,
        &mut fresh,
        terrain,
        zones,
    );
    resolve_bearings(fresh, &all, tick, &mut contacts, &mut log, &mut bearings);
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
