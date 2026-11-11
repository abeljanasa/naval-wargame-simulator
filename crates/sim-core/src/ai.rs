//! Missions and automated tasking (Phase 2 slice 10). The rules are in `sim-ai` (ADR-0005);
//! model doc `docs/models/missions.md`.
//!
//! Every [`AI_PERIOD_TICKS`] (after detection, so decisions use this tick's picture), for each
//! operational unit with a mission, in unit-id order:
//! * **Patrol:** when its route is done, pick a new point in the area (two `Ai` draws, logged).
//! * **Ferry:** steer to the destination; complete within 1 km.
//! * **Escort:** steer to the station on the escorted unit; full speed when off station.
//! * **Strike:** steer to the side's estimate of the contact.
//!
//! Then **automated engagement**: the nearest of the side's contacts (only the strike target on a
//! strike) that is not already under attack by a weapon of the side, with the first carried
//! weapon (weapon-id order) that passes every `Engage` check, release rule included. One round,
//! at most one engagement per unit per AI tick.

use crate::combat::{engage, Health, Magazine, WeaponsInFlight};
use crate::command::{Command, CommandPayload, Issuer};
use crate::components::{Kinematics, Position, Route, Unit};
use crate::detection::ContactTable;
use crate::events::{EventLog, SimEventKind};
use crate::ids::UnitId;
use crate::rng::{RngStreams, StreamId};
use crate::sim::{plan_sea_route, UnitIndex};
use crate::time::{SimClock, Tick};
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use sim_ai::{
    escort_station, nearest_first, patrol_point, station_speed, Mission, AI_PERIOD_TICKS, ARRIVAL_M,
};
use sim_geo::{inverse, GeoPoint};
use sim_weapons::{Status, WeaponKind};
use std::collections::BTreeSet;

/// A unit's mission.
#[derive(Debug, Clone, Default, PartialEq, Component, Serialize, Deserialize)]
pub struct MissionState {
    pub mission: Option<Mission>,
    pub complete: bool,
}

pub(crate) fn set_route(world: &mut World, e: Entity, unit: UnitId, to: GeoPoint, tick: Tick) {
    let (domain, from) = match (world.get::<Unit>(e), world.get::<Position>(e)) {
        (Some(u), Some(p)) => (u.domain, p.point),
        _ => return,
    };
    match plan_sea_route(world, domain, from, &[to]) {
        Ok((planned, _)) => {
            if let Some(mut r) = world.get_mut::<Route>(e) {
                r.waypoints = planned;
                r.next = 0;
            }
        }
        Err(reason) => world
            .resource_mut::<EventLog>()
            .push(tick, SimEventKind::RouteBlocked { unit, reason }),
    }
}

pub(crate) fn ensure_moving(world: &mut World, e: Entity, fraction: f64) {
    if let Some(mut k) = world.get_mut::<Kinematics>(e) {
        if k.ordered_speed_mps == 0.0 {
            k.ordered_speed_mps = fraction * k.max_speed_mps;
        }
    }
}

fn route_done(world: &World, e: Entity) -> bool {
    world
        .get::<Route>(e)
        .is_none_or(|r| r.next >= r.waypoints.len())
}

#[allow(clippy::too_many_lines)]
pub(crate) fn ai_system(world: &mut World) {
    let tick = world.resource::<SimClock>().tick;
    if !tick.0.is_multiple_of(AI_PERIOD_TICKS) {
        return;
    }
    let mut tasked: Vec<(UnitId, Entity)> = world
        .query::<(Entity, &Unit, &MissionState, &Health)>()
        .iter(world)
        .filter(|(_, _, m, h)| {
            m.mission.is_some() && !m.complete && h.status == Status::Operational
        })
        .map(|(e, u, _, _)| (u.id, e))
        .collect();
    tasked.sort_by_key(|(id, _)| *id);
    for (id, e) in tasked {
        // Units destroyed earlier in this pass (by an automated engagement) are skipped.
        if !world.resource::<UnitIndex>().0.contains_key(&id) {
            continue;
        }
        let Some(mission) = world.get::<MissionState>(e).and_then(|m| m.mission.clone()) else {
            continue;
        };
        let Some(unit) = world.get::<Unit>(e).cloned() else {
            continue;
        };
        let Some(pos) = world.get::<Position>(e).copied() else {
            continue;
        };
        match &mission {
            Mission::Patrol { centre, radius_m } => {
                if route_done(world, e) {
                    let (u1, u2) = {
                        let mut rng = world.resource_mut::<RngStreams>();
                        (rng.uniform(StreamId::Ai), rng.uniform(StreamId::Ai))
                    };
                    let point = patrol_point(*centre, *radius_m, u1, u2);
                    world.resource_mut::<EventLog>().push(
                        tick,
                        SimEventKind::MissionWaypoint {
                            unit: id,
                            point,
                            draws: [u1, u2],
                        },
                    );
                    set_route(world, e, id, point, tick);
                    ensure_moving(world, e, 0.5);
                }
            }
            Mission::Ferry { destination } => {
                if inverse(pos.point, *destination).distance_m <= ARRIVAL_M {
                    if let Some(mut m) = world.get_mut::<MissionState>(e) {
                        m.complete = true;
                    }
                    world
                        .resource_mut::<EventLog>()
                        .push(tick, SimEventKind::MissionComplete { unit: id });
                    continue;
                }
                if route_done(world, e) {
                    set_route(world, e, id, *destination, tick);
                    ensure_moving(world, e, 0.7);
                }
            }
            Mission::Escort {
                escorted,
                bearing_deg,
                range_m,
            } => {
                let target = world
                    .resource::<UnitIndex>()
                    .0
                    .get(&UnitId(*escorted))
                    .copied();
                if let Some(t) = target {
                    let (Some(tp), Some(tk)) = (
                        world.get::<Position>(t).copied(),
                        world.get::<Kinematics>(t).copied(),
                    ) else {
                        continue;
                    };
                    let station = escort_station(tp.point, tk.heading_deg, *bearing_deg, *range_m);
                    let off = inverse(pos.point, station).distance_m;
                    set_route(world, e, id, station, tick);
                    if let Some(mut k) = world.get_mut::<Kinematics>(e) {
                        k.ordered_speed_mps = station_speed(off, tk.speed_mps, k.max_speed_mps);
                    }
                }
            }
            Mission::Strike { contact } => {
                let est = world
                    .resource::<ContactTable>()
                    .iter()
                    .find(|(s, c)| *s == unit.side && c.number == *contact)
                    .map(|(_, c)| c.estimate_at(tick).position);
                if let Some(est) = est {
                    if inverse(pos.point, est).distance_m > ARRIVAL_M {
                        set_route(world, e, id, est, tick);
                        ensure_moving(world, e, 1.0);
                    }
                }
            }
        }
        auto_engage(world, id, e, &unit, &mission, pos.point, tick);
    }
}

fn auto_engage(
    world: &mut World,
    id: UnitId,
    e: Entity,
    unit: &Unit,
    mission: &Mission,
    from: GeoPoint,
    tick: Tick,
) {
    let side = unit.side;
    let under_attack: BTreeSet<u32> = world
        .resource::<WeaponsInFlight>()
        .list
        .iter()
        .filter(|w| w.side == side)
        .map(|w| w.contact)
        .collect();
    let candidates: Vec<(u32, f64)> = world
        .resource::<ContactTable>()
        .iter()
        .filter(|(s, c)| *s == side && !under_attack.contains(&c.number))
        .filter(|(_, c)| match mission {
            Mission::Strike { contact } => c.number == *contact,
            _ => true,
        })
        .map(|(_, c)| {
            (
                c.number,
                inverse(from, c.estimate_at(tick).position).distance_m,
            )
        })
        .collect();
    if candidates.is_empty() {
        return;
    }
    for contact in nearest_first(candidates) {
        if let Some(weapon) = engage_first_weapon(world, id, e, side, contact, tick) {
            world.resource_mut::<EventLog>().push(
                tick,
                SimEventKind::MissionEngage {
                    unit: id,
                    contact,
                    weapon,
                },
            );
            return;
        }
    }
}

/// Fires one round at `contact` with the first carried weapon (weapon-id order) that passes
/// every `Engage` check, release rule included, as the AI. Returns the weapon fired.
pub(crate) fn engage_first_weapon(
    world: &mut World,
    id: UnitId,
    e: Entity,
    side: crate::ids::SideId,
    contact: u32,
    tick: Tick,
) -> Option<String> {
    let catalog = world.resource::<crate::combat::WeaponCatalog>().0.clone();
    let weapons: Vec<String> = world
        .get::<Magazine>(e)
        .map(|m| {
            m.0.iter()
                .filter(|(_, n)| **n > 0)
                .filter(|(w, _)| {
                    catalog
                        .get(*w)
                        .is_some_and(|s| s.kind.engageable() && s.kind != WeaponKind::Ciws)
                })
                .map(|(w, _)| w.clone())
                .collect()
        })
        .unwrap_or_default();
    for weapon in weapons {
        let cmd = Command {
            tick,
            issuer: Issuer::Ai,
            side: Some(side),
            payload: CommandPayload::Engage {
                shooter: id,
                contact,
                weapon: weapon.clone(),
                rounds: 1,
            },
        };
        if engage(world, &cmd, id, contact, &weapon, 1, e).is_ok() {
            return Some(weapon);
        }
    }
    None
}
