//! Weapons in flight as sensor targets (Phase 3 slice 15; `docs/models/weapons-damage.md`
//! §Weapons as sensor targets).
//!
//! Every tick, before point defence, each unit (unit-id order) looks with each of its sensors
//! (mount order) at every other side's weapon in flight (weapon-id order). The rules are
//! geometry and nominal ranges, with **no draws**, so the outcome is explainable at a glance:
//!
//! | Sensor | Sees | When |
//! |---|---|---|
//! | Radar, EMCON active | missiles | within R₅₀ against 0.1 m² and the radar horizon to the missile's height, and within the radar's altitude cover |
//! | IR, EO, visual | missiles | within the optical R₅₀ against the missile (IR: a hot plume, full size; EO and visual: a 5 m body) in the light at the sensor and the visibility, and within the horizon |
//! | ESM | missiles with an active radar seeker that is homing | within the horizon and the ESM's range |
//! | Hull, towed or dipping sonar (usable) | torpedoes | within the passive range against a 150 dB torpedo |
//!
//! A side's track on a weapon holds the first and last tick seen, the latest position, and the
//! unit and sensor that saw it last. It is dropped two seconds after the last sighting, or when
//! the weapon is gone. A new track is logged (`WeaponDetected`).

use crate::c2::C2State;
use crate::combat::{WeaponCatalog, WeaponsInFlight};
use crate::components::{Domain, Kinematics, Position, Unit};
use crate::detection::{sensor_height_m, EnvState, SensorKind, SensorSuite};
use crate::events::{EventLog, SimEventKind};
use crate::hash::StateHash;
use crate::ids::{SideId, UnitId};
use crate::planning::PlanningLayer;
use crate::sim::UnitIndex;
use crate::time::{SimClock, Tick};
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use sim_c2::Emcon;
use sim_geo::{inverse, radar_horizon_m, GeoPoint};
use sim_sensors::{
    ambient_noise_db, detection_range_m, light, noise_at_speed_db, optical_r50_m, power_sum_db,
    size_factor, sonar_params, sun_elevation_deg, transmission_loss_db,
};
use sim_weapons::{Guidance, WeaponKind, MISSILE_RCS_M2};
use std::collections::BTreeMap;

/// A weapon track is dropped this many ticks after the last sighting.
pub const WEAPON_TRACK_TIMEOUT_TICKS: u64 = 2;
/// Length of a missile body for EO and visual sighting, m.
pub const MISSILE_LENGTH_M: f64 = 5.0;
/// Radiated noise of a running torpedo, dB.
pub const TORPEDO_NOISE_DB: f64 = 150.0;

/// A side's track on another side's weapon in flight.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeaponTrack {
    pub first_seen: Tick,
    pub last_seen: Tick,
    pub position: GeoPoint,
    pub by: UnitId,
    pub sensor: SensorKind,
}

/// Every side's weapon tracks: side → weapon id → track.
#[derive(Debug, Clone, Default, PartialEq, Resource, Serialize, Deserialize)]
pub struct WeaponTracks(pub BTreeMap<SideId, BTreeMap<u64, WeaponTrack>>);

impl WeaponTracks {
    /// Whether `unit` (of `side`) sees weapon `id` this tick.
    #[must_use]
    pub fn seen_by(&self, side: SideId, id: u64, unit: UnitId, now: Tick) -> bool {
        self.0
            .get(&side)
            .and_then(|m| m.get(&id))
            .is_some_and(|t| t.by == unit && t.last_seen == now)
    }

    /// Whether `side` holds a fresh track on weapon `id` by any of `units` this tick.
    #[must_use]
    pub fn seen_by_any(&self, side: SideId, id: u64, units: &[UnitId], now: Tick) -> bool {
        self.0
            .get(&side)
            .and_then(|m| m.get(&id))
            .is_some_and(|t| t.last_seen == now && units.contains(&t.by))
    }

    pub(crate) fn hash(&self, h: &mut StateHash) {
        for (side, tracks) in &self.0 {
            for (id, t) in tracks {
                h.u16(side.0);
                h.u64(*id);
                h.u64(t.first_seen.0);
                h.u64(t.last_seen.0);
                h.f64(t.position.lat_deg);
                h.f64(t.position.lon_deg);
                h.u64(t.by.0);
                h.u8(t.sensor.code());
            }
        }
    }
}

/// A side's view of one weapon it tracks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct WeaponTrackView {
    pub side: SideId,
    #[cfg_attr(feature = "specta", specta(type = u32))]
    pub weapon_id: u64,
    pub position: GeoPoint,
    /// `air` (a missile) or `subsurface` (a torpedo).
    pub domain: Domain,
    pub sensor: SensorKind,
    pub first_seen_s: f64,
}

/// Passive range against a torpedo, by bisection (transmission loss grows with range).
fn torpedo_reach_m(
    kind: SensorKind,
    active: bool,
    max_range_m: f64,
    own_speed: f64,
    sea: u8,
) -> f64 {
    let p = sonar_params(kind, active);
    let nl = power_sum_db(
        ambient_noise_db(sea),
        noise_at_speed_db(p.self_noise_db, own_speed),
    );
    let allowed = TORPEDO_NOISE_DB - nl + p.di_db - p.dt_db;
    let tl = |r: f64| transmission_loss_db(r, p.frequency_khz);
    if tl(max_range_m) <= allowed {
        return max_range_m;
    }
    if tl(1.0) > allowed {
        return 0.0;
    }
    let (mut lo, mut hi) = (1.0, max_range_m);
    for _ in 0..60 {
        let mid = 0.5 * (lo + hi);
        if tl(mid) <= allowed {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo
}

struct Observer {
    id: UnitId,
    side: SideId,
    domain: Domain,
    position: Position,
    speed: f64,
    emits: bool,
    sensors: Vec<crate::detection::SensorSpec>,
}

#[allow(clippy::too_many_lines)]
pub(crate) fn weapon_tracks_system(world: &mut World) {
    let tick = world.resource::<SimClock>().tick;
    let weapons = world.resource::<WeaponsInFlight>().list.clone();
    if weapons.is_empty() {
        if !world.resource::<WeaponTracks>().0.is_empty() {
            world.resource_mut::<WeaponTracks>().0.clear();
        }
        return;
    }
    let catalog = world.resource::<WeaponCatalog>().0.clone();
    let env = world.resource::<EnvState>().0;
    let zones = world.resource::<crate::detection::Weather>().0.clone();
    #[allow(clippy::cast_precision_loss)]
    let unix_s = env.start_unix_s as f64 + tick.seconds();
    let c2 = world.resource::<C2State>().clone();
    let layer = world.resource::<PlanningLayer>().clone();
    let observers: Vec<Observer> = world
        .resource::<UnitIndex>()
        .0
        .values()
        .filter_map(|&e| {
            let u = world.get::<Unit>(e)?;
            let p = world.get::<Position>(e)?;
            let s = world.get::<SensorSuite>(e)?;
            let k = world.get::<Kinematics>(e)?;
            (!s.0.is_empty()).then(|| Observer {
                id: u.id,
                side: u.side,
                domain: u.domain,
                position: *p,
                speed: k.speed_mps,
                emits: layer.doctrine_for(&c2, u, p.point).emcon == Emcon::Active,
                sensors: s.0.clone(),
            })
        })
        .collect();
    let mut seen: Vec<(SideId, u64, GeoPoint, UnitId, SensorKind)> = Vec::new();
    for o in &observers {
        for w in weapons.iter().filter(|w| w.side != o.side) {
            let Some(spec) = catalog.get(&w.weapon) else {
                continue;
            };
            if seen
                .iter()
                .any(|s| s.0 == o.side && s.1 == w.id && s.3 == o.id)
            {
                continue;
            }
            let torpedo = spec.kind == WeaponKind::Torpedo;
            let range = inverse(o.position.point, w.position).distance_m;
            let alt = w.altitude_m.max(0.0);
            let hit = o.sensors.iter().find(|s| {
                if range > s.max_range_m {
                    return false;
                }
                let h = sensor_height_m(s.mount_height_m, o.domain, o.position.altitude_m);
                let horizon = h.map_or(0.0, |h| radar_horizon_m(h, alt));
                match s.kind {
                    SensorKind::Radar if !torpedo && o.emits => {
                        let (Some(rr), Some(rc)) = (s.ref_range_m, s.ref_rcs_m2) else {
                            return false;
                        };
                        alt <= s.max_target_altitude_m
                            && range
                                <= detection_range_m(rr, rc, MISSILE_RCS_M2, s.max_range_m)
                                    .min(horizon)
                    }
                    SensorKind::Ir | SensorKind::Eo | SensorKind::Visual if !torpedo => {
                        let sun = sun_elevation_deg(
                            unix_s,
                            o.position.point.lat_deg,
                            o.position.point.lon_deg,
                        );
                        let r50 = optical_r50_m(
                            s.kind,
                            s.max_range_m,
                            size_factor(s.kind, true, MISSILE_LENGTH_M),
                            light(sun),
                            crate::detection::local_env(env, &zones, o.position.point)
                                .visibility_km
                                * 1000.0,
                        );
                        range <= r50.min(horizon)
                    }
                    SensorKind::Esm if !torpedo => {
                        spec.guidance == Guidance::ActiveRadar && w.homing && range <= horizon
                    }
                    SensorKind::SonarHull | SensorKind::SonarTowed | SensorKind::SonarDipping
                        if torpedo =>
                    {
                        crate::sonar::sonar_usable(s.kind, o.domain, o.position.altitude_m, o.speed)
                            && range
                                <= torpedo_reach_m(
                                    s.kind,
                                    s.active,
                                    s.max_range_m,
                                    o.speed,
                                    crate::detection::local_env(env, &zones, o.position.point)
                                        .sea_state,
                                )
                    }
                    _ => false,
                }
            });
            if let Some(s) = hit {
                seen.push((o.side, w.id, w.position, o.id, s.kind));
            }
        }
    }
    // Merge: the last observer (unit-id order) to see a weapon is recorded as its tracker.
    let mut new: Vec<(SideId, u64, UnitId, SensorKind)> = Vec::new();
    {
        let mut tracks = world.resource_mut::<WeaponTracks>();
        for (side, id, position, by, sensor) in seen {
            let m = tracks.0.entry(side).or_default();
            match m.get_mut(&id) {
                Some(t) => {
                    t.last_seen = tick;
                    t.position = position;
                    t.by = by;
                    t.sensor = sensor;
                }
                None => {
                    m.insert(
                        id,
                        WeaponTrack {
                            first_seen: tick,
                            last_seen: tick,
                            position,
                            by,
                            sensor,
                        },
                    );
                    new.push((side, id, by, sensor));
                }
            }
        }
        let live: Vec<u64> = weapons.iter().map(|w| w.id).collect();
        for m in tracks.0.values_mut() {
            m.retain(|id, t| {
                live.contains(id)
                    && tick.0.saturating_sub(t.last_seen.0) <= WEAPON_TRACK_TIMEOUT_TICKS
            });
        }
        tracks.0.retain(|_, m| !m.is_empty());
    }
    for (side, weapon_id, by, sensor) in new {
        world.resource_mut::<EventLog>().push(
            tick,
            SimEventKind::WeaponDetected {
                side,
                weapon_id,
                by,
                sensor,
            },
        );
    }
}

/// The weapon tracks `viewer` may see (its side's; every side's for the white cell).
pub(crate) fn views(world: &World, side: Option<SideId>) -> Vec<WeaponTrackView> {
    let catalog = &world.resource::<WeaponCatalog>().0;
    let kind_of: BTreeMap<u64, WeaponKind> = world
        .resource::<WeaponsInFlight>()
        .list
        .iter()
        .filter_map(|w| catalog.get(&w.weapon).map(|s| (w.id, s.kind)))
        .collect();
    world
        .resource::<WeaponTracks>()
        .0
        .iter()
        .filter(|(s, _)| side.is_none_or(|v| v == **s))
        .flat_map(|(s, m)| {
            m.iter().map(|(id, t)| WeaponTrackView {
                side: *s,
                weapon_id: *id,
                position: t.position,
                domain: if kind_of.get(id) == Some(&WeaponKind::Torpedo) {
                    Domain::Subsurface
                } else {
                    Domain::Air
                },
                sensor: t.sensor,
                first_seen_s: t.first_seen.seconds(),
            })
        })
        .collect()
}
