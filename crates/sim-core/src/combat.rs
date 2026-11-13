//! Engagements, weapons in flight, hits, damage and damage control. The models are in
//! `sim-weapons` (ADR-0005); model doc `docs/models/weapons-damage.md`.
//!
//! * `Engage` checks the order, takes rounds from the magazine and launches. Flying weapons
//!   (missiles, torpedoes) become entries in [`WeaponsInFlight`]; guns, CIWS, bombs, depth
//!   charges and ASW rockets resolve at once.
//! * [`weapons_system`] (every tick, weapon-id order) steers each weapon to the side's current
//!   track estimate, gives a seeker one acquisition draw when its target comes within seeker
//!   range, and resolves impact with a Ph draw.
//! * Hits do damage ([`apply_hit`]); [`damage_control_system`] (every 60 s) burns, floods and
//!   fights damage. A unit at full damage is destroyed and removed.
//!
//! Draws: seeker and hit rolls on the `Weapons` stream; critical, fire, flooding and damage
//! control on the `Damage` stream. Every draw is logged with its inputs.

use crate::c2::C2State;
use crate::command::Command;
use crate::components::{Domain, Kinematics, Position, Unit};
use crate::detection::{ContactTable, EnvState, SensorSuite, Signature};
use crate::events::{EventLog, SimEventKind};
use crate::ids::{SideId, UnitId};
use crate::rng::{RngStreams, StreamId};
use crate::sim::UnitIndex;
use crate::time::{SimClock, Tick};
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use sim_c2::{may_engage, track_affiliation};
use sim_geo::{direct, inverse, GeoPoint};
use sim_weapons::{
    countermeasure_modifier, critical_system, fire_starts, flooding_starts, hit_damage_dp, ph,
    ph_sea_modifier, ph_size_modifier, ph_speed_modifier, DamageProfile, Guidance, Status, System,
    WeaponKind, WeaponSpec, CRITICAL_CHANCE, DC_FIRE_PER_MINUTE, DC_FLOOD_PER_MINUTE,
    FIRE_DP_PER_MINUTE, FLOOD_DP_PER_MINUTE, SEEKER_ACQUISITION_PD,
};
use std::collections::BTreeMap;

/// Damage control runs once a minute.
pub const DAMAGE_CONTROL_PERIOD_TICKS: u64 = 60;
/// A flying weapon carries fuel for this multiple of its maximum range.
pub const FUEL_RANGE_FACTOR: f64 = 1.1;
/// A navigation-guided weapon hits only if its target is within this distance of the aim point.
pub const NAVIGATION_HIT_RADIUS_M: f64 = 200.0;

/// Rounds of each weapon a unit carries.
#[derive(Debug, Clone, Default, PartialEq, Eq, Component, Serialize, Deserialize)]
pub struct Magazine(pub BTreeMap<String, u32>);

/// A unit's damage state.
#[derive(Debug, Clone, PartialEq, Component, Serialize, Deserialize)]
pub struct Health {
    pub profile: DamageProfile,
    pub damage: f64,
    pub fires: u32,
    pub flooding: bool,
    /// Systems knocked out, sorted, without repeats.
    pub criticals: Vec<System>,
    pub status: Status,
}

impl Health {
    #[must_use]
    pub fn new(profile: DamageProfile) -> Self {
        Self {
            profile,
            damage: 0.0,
            fires: 0,
            flooding: false,
            criticals: Vec::new(),
            status: Status::Operational,
        }
    }

    fn reassess(&mut self) {
        self.status = Status::assess(&self.profile, self.damage, &self.criticals);
    }
}

/// Damage profile for a unit without a database profile (original design).
#[must_use]
pub fn default_damage_profile(domain: Domain) -> DamageProfile {
    let (dp, mk, fire, flood) = match domain {
        Domain::Surface => (300.0, 0.45, 0.5, 0.5),
        Domain::Subsurface => (300.0, 0.4, 0.4, 0.7),
        Domain::Air => (12.0, 0.3, 0.5, 0.0),
        Domain::Land => (150.0, 0.5, 0.3, 0.0),
    };
    DamageProfile {
        damage_points: dp,
        mission_kill_fraction: mk,
        fire_susceptibility: fire,
        flooding_susceptibility: flood,
    }
}

/// Every weapon type in the scenario (from the platform database).
#[derive(Debug, Clone, Default, Resource, Serialize, Deserialize)]
pub struct WeaponCatalog(pub BTreeMap<String, WeaponSpec>);

/// A missile or torpedo on its way.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeaponInFlight {
    pub id: u64,
    pub weapon: String,
    pub side: SideId,
    pub shooter: UnitId,
    /// The unit the engaged track belongs to (engine-internal; never sent to a side).
    pub target: UnitId,
    /// The shooter side's contact number for the track.
    pub contact: u32,
    pub position: GeoPoint,
    pub altitude_m: f64,
    pub heading_deg: f64,
    pub aim: GeoPoint,
    pub range_left_m: f64,
    pub seeker_tried: bool,
    pub homing: bool,
    pub launched: Tick,
    /// Seduction effect of a decoy the target fired at this weapon (lowers Ph at impact).
    #[serde(default)]
    pub decoy_effect: Option<f64>,
}

/// Weapons in flight, in launch (id) order.
#[derive(Debug, Clone, Default, Resource, Serialize, Deserialize)]
pub struct WeaponsInFlight {
    pub next_id: u64,
    pub list: Vec<WeaponInFlight>,
}

/// A weapon in flight that missed, for the outcome feed (where the weapon was).
pub(crate) fn note_miss(world: &mut World, w: &WeaponInFlight, tick: Tick, reason: &str) {
    crate::outcomes::note(
        world,
        crate::outcomes::Note {
            tick,
            weapon: w.weapon.clone(),
            result: crate::outcomes::OutcomeKind::Miss,
            position: Some(w.position),
            shooter: Some(w.shooter),
            shooter_side: Some(w.side),
            target: Some(w.target),
            reason: Some(reason.to_owned()),
        },
    );
}

fn entity_of(world: &World, unit: UnitId) -> Option<Entity> {
    world.resource::<UnitIndex>().0.get(&unit).copied()
}

fn weapon_altitude(kind: WeaponKind, target_alt: Option<f64>) -> f64 {
    match kind {
        WeaponKind::Torpedo => -50.0,
        WeaponKind::Asm | WeaponKind::Lacm => 10.0,
        _ => target_alt.unwrap_or(5_000.0),
    }
}

/// Carries out an `Engage` order. Every check is a reason to refuse it; nothing is consumed
/// unless the order is valid.
#[allow(clippy::too_many_lines)]
pub(crate) fn engage(
    world: &mut World,
    cmd: &Command,
    shooter: UnitId,
    contact: u32,
    weapon: &str,
    rounds: u32,
    entity: Entity,
) -> Result<(), String> {
    let tick = world.resource::<SimClock>().tick;
    if rounds == 0 {
        return Err("rounds must be at least 1".into());
    }
    let unit = world
        .get::<Unit>(entity)
        .cloned()
        .ok_or_else(|| format!("{shooter} has no identity"))?;
    let from = world
        .get::<Position>(entity)
        .copied()
        .ok_or_else(|| format!("{shooter} has no position"))?;
    if let Some(h) = world.get::<Health>(entity) {
        if h.status != Status::Operational {
            return Err(format!("{shooter} is not operational ({:?})", h.status));
        }
        if h.criticals.contains(&System::Weapons) {
            return Err(format!("{shooter}'s weapons are knocked out"));
        }
    }
    let spec = world
        .resource::<WeaponCatalog>()
        .0
        .get(weapon)
        .cloned()
        .ok_or_else(|| format!("unknown weapon {weapon}"))?;
    if !spec.kind.engageable() {
        return Err(format!("{weapon} cannot be fired at a track"));
    }
    let held = world
        .get::<Magazine>(entity)
        .and_then(|m| m.0.get(weapon).copied())
        .unwrap_or(0);
    if held < rounds {
        return Err(format!("{shooter} has {held} × {weapon}, not {rounds}"));
    }
    let side = unit.side;
    let track = world
        .resource::<ContactTable>()
        .iter()
        .find(|(s, c)| *s == side && c.number == contact)
        .map(|(_, c)| c.clone())
        .ok_or_else(|| format!("side {} holds no contact C{contact}", side.0))?;
    let domain = track
        .domain
        .ok_or_else(|| format!("C{contact} is not classified to a domain"))?;
    let domain_name = match domain {
        Domain::Surface => "surface",
        Domain::Subsurface => "subsurface",
        Domain::Air => "air",
        Domain::Land => "land",
    };
    if !spec.target_domains.iter().any(|d| d == domain_name) {
        return Err(format!("{weapon} cannot engage {domain_name} targets"));
    }
    let estimate = track.estimate_at(tick);
    let sigma = estimate.cov.ellipse().semi_major_m;
    let limit = spec.guidance.max_track_sigma_m();
    if sigma > limit {
        return Err(format!(
            "C{contact} track error {sigma:.0} m exceeds the {limit:.0} m {weapon} needs"
        ));
    }
    let range = inverse(from.point, estimate.position).distance_m;
    if range < spec.min_range_m || range > spec.max_range_m {
        return Err(format!(
            "C{contact} at {:.1} km is outside {weapon}'s {:.1}–{:.1} km",
            range / 1000.0,
            spec.min_range_m / 1000.0,
            spec.max_range_m / 1000.0
        ));
    }
    // Weapons control status and ROE. The white cell may override (adjudication).
    if !cmd.issuer.is_privileged() {
        let c2 = world.resource::<C2State>();
        let doctrine = world
            .resource::<crate::planning::PlanningLayer>()
            .engage_doctrine(c2, &unit, from.point, estimate.position);
        let target_side = entity_of(world, track.target)
            .and_then(|e| world.get::<Unit>(e))
            .map(|u| u.side);
        let known =
            target_side.and_then(|t| track_affiliation(track.classification, c2.posture(side, t)));
        if !may_engage(doctrine, known) {
            return Err(format!(
                "weapons control status {:?} forbids engaging C{contact}",
                doctrine.wcs
            ));
        }
    }
    if let Some(mut m) = world.get_mut::<Magazine>(entity) {
        let left = held - rounds;
        if left == 0 {
            m.0.remove(weapon);
        } else {
            m.0.insert(weapon.to_owned(), left);
        }
    }
    for _ in 0..rounds {
        if spec.kind.flies() {
            let target_alt = entity_of(world, track.target)
                .and_then(|e| world.get::<Position>(e))
                .map(|p| p.altitude_m);
            let mut wif = world.resource_mut::<WeaponsInFlight>();
            let id = wif.next_id;
            wif.next_id += 1;
            wif.list.push(WeaponInFlight {
                id,
                weapon: weapon.to_owned(),
                side,
                shooter,
                target: track.target,
                contact,
                position: from.point,
                altitude_m: weapon_altitude(spec.kind, target_alt),
                heading_deg: inverse(from.point, estimate.position).azimuth1_deg,
                aim: estimate.position,
                range_left_m: FUEL_RANGE_FACTOR * spec.max_range_m,
                seeker_tried: false,
                homing: false,
                launched: tick,
                decoy_effect: None,
            });
            world.resource_mut::<EventLog>().push(
                tick,
                SimEventKind::WeaponLaunched {
                    weapon_id: id,
                    weapon: weapon.to_owned(),
                    shooter,
                    side,
                    contact,
                },
            );
        } else {
            // Guns, CIWS, bombs, depth charges, ASW rockets: resolved at once.
            apply_hit_roll(
                world,
                &spec,
                None,
                None,
                (Some(shooter), Some(side)),
                track.target,
                tick,
            );
        }
    }
    Ok(())
}

/// Rolls Ph for a weapon arriving at `target`, and applies the hit.
pub(crate) fn apply_hit_roll(
    world: &mut World,
    spec: &WeaponSpec,
    weapon_id: Option<u64>,
    decoy_effect: Option<f64>,
    (shooter, by_side): (Option<UnitId>, Option<SideId>),
    target: UnitId,
    tick: Tick,
) {
    let Some(e) = entity_of(world, target) else {
        return;
    };
    let domain = world.get::<Unit>(e).map_or(Domain::Surface, |u| u.domain);
    let speed = world.get::<Kinematics>(e).map_or(0.0, |k| k.speed_mps);
    let length = world
        .get::<Signature>(e)
        .and_then(|s| s.length_m)
        .unwrap_or_else(|| crate::optical::default_length_m(domain));
    // The sea where the target is (weather zones), else the scenario-wide sea state.
    let sea_state = {
        let env = world.resource::<EnvState>().0;
        let zones = &world.resource::<crate::detection::Weather>().0;
        world
            .get::<crate::components::Position>(e)
            .map_or(env.sea_state, |p| {
                crate::detection::local_env(env, zones, p.point).sea_state
            })
    };
    let air = domain == Domain::Air;
    let size = ph_size_modifier(air, length);
    let manoeuvre = ph_speed_modifier(air, speed);
    let sea = ph_sea_modifier(spec.kind, sea_state);
    // Soft kill: a decoy fired at this weapon, and the target's own jammer if it is radiating.
    let jamming = world
        .get::<crate::detection::Jammer>(e)
        .is_some_and(|j| j.0.is_some())
        && world.get::<Unit>(e).is_some_and(|u| {
            let at = world
                .get::<Position>(e)
                .map_or(GeoPoint::new(0.0, 0.0), |p| p.point);
            world
                .resource::<crate::planning::PlanningLayer>()
                .doctrine_for(world.resource::<C2State>(), u, at)
                .emcon
                == sim_c2::Emcon::Active
        });
    let countermeasures = countermeasure_modifier(spec.guidance, decoy_effect, jamming);
    let p = ph(spec.base_ph, size, manoeuvre, sea) * countermeasures;
    let draw = world
        .resource_mut::<RngStreams>()
        .uniform(StreamId::Weapons);
    let hit = draw < p;
    world.resource_mut::<EventLog>().push(
        tick,
        SimEventKind::HitRoll {
            weapon_id,
            weapon: spec.id.clone(),
            shooter,
            side: by_side,
            target,
            base_ph: spec.base_ph,
            size_modifier: size,
            speed_modifier: manoeuvre,
            sea_modifier: sea,
            countermeasure_modifier: countermeasures,
            ph: p,
            draw,
            hit,
        },
    );
    // Before the hit is applied: a destroyed target leaves the world.
    crate::outcomes::note(
        world,
        crate::outcomes::Note {
            tick,
            weapon: spec.id.clone(),
            result: if hit {
                crate::outcomes::OutcomeKind::Hit
            } else {
                crate::outcomes::OutcomeKind::Miss
            },
            position: None,
            shooter,
            shooter_side: by_side,
            target: Some(target),
            reason: None,
        },
    );
    if hit {
        apply_hit(
            world,
            target,
            spec.warhead_kg,
            matches!(spec.kind, WeaponKind::Torpedo | WeaponKind::Mine),
            tick,
        );
    }
}

/// Applies one hit: damage points, then four `Damage` draws in fixed order (critical?, which
/// system, fire?, flooding?). Destroys the unit if its damage reaches its damage points.
pub(crate) fn apply_hit(
    world: &mut World,
    target: UnitId,
    warhead_kg: f64,
    torpedo: bool,
    tick: Tick,
) {
    apply_damage(world, target, hit_damage_dp(warhead_kg), torpedo, tick);
}

/// Applies `dp` damage points to `target`, then four `Damage` draws in fixed order (critical?,
/// which system, fire?, flooding?; `torpedo` floods as an underwater hit or a holed hull does).
/// Destroys the unit if its damage reaches its damage points. Weapon hits and collisions both
/// come here.
pub(crate) fn apply_damage(world: &mut World, target: UnitId, dp: f64, torpedo: bool, tick: Tick) {
    let Some(e) = entity_of(world, target) else {
        return;
    };
    let [u_crit, u_system, u_fire, u_flood] = {
        let mut rng = world.resource_mut::<RngStreams>();
        [
            rng.uniform(StreamId::Damage),
            rng.uniform(StreamId::Damage),
            rng.uniform(StreamId::Damage),
            rng.uniform(StreamId::Damage),
        ]
    };
    let Some(mut h) = world.get_mut::<Health>(e) else {
        return;
    };
    h.damage += dp;
    let critical = (u_crit < CRITICAL_CHANCE).then(|| critical_system(u_system));
    let fire = u_fire < fire_starts(&h.profile);
    let flooding = u_flood < flooding_starts(&h.profile, torpedo);
    if fire {
        h.fires += 1;
    }
    if flooding {
        h.flooding = true;
    }
    let mut new_critical = false;
    if let Some(sys) = critical {
        if !h.criticals.contains(&sys) {
            h.criticals.push(sys);
            h.criticals.sort();
            new_critical = true;
        }
    }
    h.reassess();
    let (total, status) = (h.damage, h.status);
    if new_critical {
        apply_critical(world, e, critical);
    }
    world.resource_mut::<EventLog>().push(
        tick,
        SimEventKind::DamageTaken {
            unit: target,
            damage_dp: dp,
            total_dp: total,
            critical_draw: u_crit,
            critical,
            fire_draw: u_fire,
            fire_started: fire,
            flood_draw: u_flood,
            flooding_started: flooding,
            status,
        },
    );
    if status == Status::Destroyed {
        destroy(world, target, e, tick);
    }
}

/// What a critical hit does to the unit.
fn apply_critical(world: &mut World, e: Entity, critical: Option<System>) {
    match critical {
        Some(System::Propulsion) => {
            if let Some(mut k) = world.get_mut::<Kinematics>(e) {
                k.max_speed_mps *= 0.5;
                k.ordered_speed_mps = k.ordered_speed_mps.min(k.max_speed_mps);
            }
        }
        Some(System::Sensors) => {
            if let Some(mut s) = world.get_mut::<SensorSuite>(e) {
                s.0.clear();
            }
        }
        // Weapons are checked at `Engage`; flight deck and comms gain effects with air
        // operations and comms links.
        _ => {}
    }
}

pub(crate) fn destroy(world: &mut World, unit: UnitId, e: Entity, tick: Tick) {
    world.despawn(e);
    world.resource_mut::<UnitIndex>().0.remove(&unit);
    world
        .resource_mut::<EventLog>()
        .push(tick, SimEventKind::UnitDestroyed { unit });
}

/// Moves and resolves every weapon in flight (each tick, in weapon-id order).
#[allow(clippy::too_many_lines)]
pub(crate) fn weapons_system(world: &mut World) {
    let tick = world.resource::<SimClock>().tick;
    let list = std::mem::take(&mut world.resource_mut::<WeaponsInFlight>().list);
    let mut keep = Vec::with_capacity(list.len());
    for mut w in list {
        let Some(spec) = world.resource::<WeaponCatalog>().0.get(&w.weapon).cloned() else {
            continue;
        };
        let target = entity_of(world, w.target).and_then(|e| world.get::<Position>(e).copied());
        // The shooter side's current track on the target, for midcourse updates and for
        // weapons the shooter guides to impact.
        let track = world
            .resource::<ContactTable>()
            .iter()
            .find(|(s, c)| *s == w.side && c.target == w.target)
            .map(|(_, c)| c.estimate_at(tick).position);
        let g = spec.guidance;
        if w.homing {
            if let Some(p) = target {
                w.aim = p.point;
            }
        } else if g != Guidance::Navigation {
            if let Some(est) = track {
                w.aim = est;
            }
        }
        // Seeker: one acquisition draw when the real target first comes within seeker range.
        if let (Some(seeker), Some(p), false) = (g.seeker_range_m(), target, w.seeker_tried) {
            let range = inverse(w.position, p.point).distance_m;
            if range <= seeker {
                let draw = world
                    .resource_mut::<RngStreams>()
                    .uniform(StreamId::Weapons);
                let acquired = draw < SEEKER_ACQUISITION_PD;
                w.seeker_tried = true;
                w.homing = acquired;
                if acquired {
                    w.aim = p.point;
                }
                world.resource_mut::<EventLog>().push(
                    tick,
                    SimEventKind::SeekerAcquisition {
                        weapon_id: w.id,
                        range_m: range,
                        pd: SEEKER_ACQUISITION_PD,
                        draw,
                        acquired,
                    },
                );
            }
        }
        let step = spec.speed_mps;
        if inverse(w.position, w.aim).distance_m <= step {
            // Arrival: hit roll, a miss with a reason, or (a seeker that never acquired)
            // fly on along the same heading until the fuel runs out.
            let miss: Option<&str> = match (g, target) {
                (_, None) => Some("target no longer exists"),
                (Guidance::ActiveRadar | Guidance::Passive | Guidance::Acoustic, Some(_)) => {
                    if w.homing {
                        None
                    } else {
                        w.aim = direct(w.position, w.heading_deg, 1.0e7).point;
                        Some("")
                    }
                }
                (Guidance::SemiActive | Guidance::Command, Some(_)) => {
                    track.is_none().then_some("shooter lost the track")
                }
                (Guidance::Navigation, Some(p)) => (inverse(w.aim, p.point).distance_m
                    > NAVIGATION_HIT_RADIUS_M)
                    .then_some("target not at the aim point"),
                (Guidance::Unguided, Some(_)) => None,
            };
            match miss {
                None => {
                    apply_hit_roll(
                        world,
                        &spec,
                        Some(w.id),
                        w.decoy_effect,
                        (Some(w.shooter), Some(w.side)),
                        w.target,
                        tick,
                    );
                    continue;
                }
                Some("") => {}
                Some(reason) => {
                    world.resource_mut::<EventLog>().push(
                        tick,
                        SimEventKind::WeaponMissed {
                            weapon_id: w.id,
                            reason: reason.to_owned(),
                        },
                    );
                    note_miss(world, &w, tick, reason);
                    continue;
                }
            }
        }
        let bearing = inverse(w.position, w.aim).azimuth1_deg;
        w.heading_deg = bearing;
        w.position = direct(w.position, bearing, step).point;
        if !matches!(spec.kind, WeaponKind::Torpedo | WeaponKind::Asm) {
            if let Some(p) = target {
                w.altitude_m = p.altitude_m;
            }
        }
        w.range_left_m -= step;
        if w.range_left_m <= 0.0 {
            world.resource_mut::<EventLog>().push(
                tick,
                SimEventKind::WeaponMissed {
                    weapon_id: w.id,
                    reason: "out of fuel".into(),
                },
            );
            note_miss(world, &w, tick, "out of fuel");
            continue;
        }
        keep.push(w);
    }
    let mut wif = world.resource_mut::<WeaponsInFlight>();
    keep.extend(std::mem::take(&mut wif.list));
    keep.sort_by_key(|w| w.id);
    wif.list = keep;
}

/// Fires burn and compartments flood; damage control fights them (every 60 s, unit-id order).
pub(crate) fn damage_control_system(world: &mut World) {
    let tick = world.resource::<SimClock>().tick;
    if !tick.0.is_multiple_of(DAMAGE_CONTROL_PERIOD_TICKS) {
        return;
    }
    let mut affected: Vec<(UnitId, Entity)> = world
        .query::<(Entity, &Unit, &Health)>()
        .iter(world)
        .filter(|(_, _, h)| h.fires > 0 || h.flooding)
        .map(|(e, u, _)| (u.id, e))
        .collect();
    affected.sort_by_key(|(id, _)| *id);
    for (unit, e) in affected {
        let (fires, flooding) = match world.get::<Health>(e) {
            Some(h) => (h.fires, h.flooding),
            None => continue,
        };
        let mut draws = Vec::new();
        {
            let mut rng = world.resource_mut::<RngStreams>();
            for _ in 0..fires {
                draws.push(rng.uniform(StreamId::Damage));
            }
            if flooding {
                draws.push(rng.uniform(StreamId::Damage));
            }
        }
        let fire_dp = f64::from(fires) * FIRE_DP_PER_MINUTE;
        let flood_dp = if flooding { FLOOD_DP_PER_MINUTE } else { 0.0 };
        let n_fire = fires as usize;
        #[allow(clippy::cast_possible_truncation)]
        let fires_out = draws[..n_fire]
            .iter()
            .filter(|&&u| u < DC_FIRE_PER_MINUTE)
            .count() as u32;
        let flooding_stopped = flooding && draws[n_fire] < DC_FLOOD_PER_MINUTE;
        let Some(mut h) = world.get_mut::<Health>(e) else {
            continue;
        };
        h.damage += fire_dp + flood_dp;
        h.fires -= fires_out;
        if flooding_stopped {
            h.flooding = false;
        }
        h.reassess();
        let (total, status) = (h.damage, h.status);
        world.resource_mut::<EventLog>().push(
            tick,
            SimEventKind::DamageControl {
                unit,
                fire_dp,
                flood_dp,
                total_dp: total,
                draws,
                fires_out,
                flooding_stopped,
                status,
            },
        );
        if status == Status::Destroyed {
            destroy(world, unit, e, tick);
        }
    }
}
