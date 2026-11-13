//! Missions and automated tasking (Phase 2 slice 10). The rules are in `sim-ai` (ADR-0005);
//! model doc `docs/models/missions.md`.
//!
//! Every [`AI_PERIOD_TICKS`] (after detection, so decisions use this tick's picture), for each
//! operational unit with a mission, in unit-id order:
//! * **Patrol:** when its route is done, pick a new point in the area (two `Ai` draws, logged).
//! * **Ferry:** steer to the destination; complete within 1 km.
//! * **Escort:** steer to the station on the escorted unit; full speed when off station.
//! * **Strike:** steer to the side's estimate of the contact.
//! * **Support:** hold within the station's radius.
//! * **Mining:** go to the centre, lay the field, complete.
//! * **Mine clearing:** patrol the area at 8 kn; `mine_system` sweeps (`docs/models/missions.md`).
//! * **Cargo:** origin, load 30 min, destination, unload 30 min, complete.
//! * **Barrier patrol:** back and forth between two points.
//! * **Denial:** patrol inside the side's zones of a choke point.
//!
//! Then **automated engagement** (patrol, ferry, escort, strike, barrier patrol, denial): the
//! nearest of the side's contacts (only the strike target on a strike; only contacts believed
//! inside the choke point on denial) that is not already under attack by a weapon of the side,
//! with the first carried
//! weapon (weapon-id order) that passes every `Engage` check, release rule included. One round,
//! at most one engagement per unit per AI tick.

use crate::combat::{engage, Health, Magazine, WeaponsInFlight};
use crate::command::{Command, CommandPayload, Issuer};
use crate::components::{Kinematics, Position, Route, Unit};
use crate::detection::ContactTable;
use crate::events::{EventLog, SimEventKind};
use crate::ids::{SideId, UnitId};
use crate::rng::{RngStreams, StreamId};
use crate::sim::{plan_sea_route, UnitIndex};
use crate::time::{SimClock, Tick};
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use sim_ai::{
    escort_station, nearest_first, patrol_point, station_speed, Mission, AI_PERIOD_TICKS,
    ARRIVAL_M, CARGO_HANDLING_TICKS, MCM_SPEED_MPS,
};
use sim_events::Area;
use sim_geo::{inverse, GeoPoint};
use sim_weapons::{Status, WeaponKind};
use std::collections::BTreeSet;

/// A unit's mission.
#[derive(Debug, Clone, Default, PartialEq, Component, Serialize, Deserialize)]
pub struct MissionState {
    pub mission: Option<Mission>,
    pub complete: bool,
    /// Progress through a multi-step mission (barrier leg, cargo phase); 0 at the start.
    #[serde(default)]
    pub phase: u8,
    /// When the current phase's timed step began (loading, unloading).
    #[serde(default)]
    pub since: Option<Tick>,
}

pub(crate) fn set_route(world: &mut World, e: Entity, unit: UnitId, to: GeoPoint, tick: Tick) {
    let (domain, draft, from) = match (world.get::<Unit>(e), world.get::<Position>(e)) {
        (Some(u), Some(p)) => (u.domain, u.draft_m, p.point),
        _ => return,
    };
    match plan_sea_route(world, domain, draft, from, &[to]) {
        Ok(planned) => {
            if let Some(mut r) = world.get_mut::<Route>(e) {
                r.waypoints = planned.waypoints;
                r.next = 0;
                r.charted = planned.charted;
            }
            if planned.shallow_legs > 0 {
                #[allow(clippy::cast_possible_truncation)]
                world.resource_mut::<EventLog>().push(
                    tick,
                    SimEventKind::RouteShallow {
                        unit,
                        legs: planned.shallow_legs as u32,
                    },
                );
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
            Mission::Support { station, radius_m } => {
                if inverse(pos.point, *station).distance_m > *radius_m {
                    if route_done(world, e) {
                        set_route(world, e, id, *station, tick);
                        ensure_moving(world, e, 0.7);
                    }
                } else {
                    hold(world, e);
                }
            }
            Mission::Mining {
                centre,
                radius_m,
                mines,
            } => {
                if inverse(pos.point, *centre).distance_m <= ARRIVAL_M {
                    hold(world, e);
                    let carried = mines_carried(world, e);
                    let kind = if carried == 0 {
                        SimEventKind::MissionFailed {
                            unit: id,
                            reason: "carries no mines".into(),
                        }
                    } else {
                        let n = (*mines).min(carried);
                        match crate::mines::lay_mines(world, id, e, n, *radius_m) {
                            Ok(field) => SimEventKind::MissionMinesLaid {
                                unit: id,
                                field,
                                mines: n,
                            },
                            Err(reason) => SimEventKind::MissionFailed { unit: id, reason },
                        }
                    };
                    world.resource_mut::<EventLog>().push(tick, kind);
                    complete(world, e, id, tick);
                    continue;
                }
                if route_done(world, e) {
                    set_route(world, e, id, *centre, tick);
                    ensure_moving(world, e, 0.7);
                }
            }
            Mission::MineClearing { centre, radius_m } => {
                if inverse(pos.point, *centre).distance_m > *radius_m {
                    if route_done(world, e) {
                        set_route(world, e, id, *centre, tick);
                    }
                } else if route_done(world, e) {
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
                }
                if let Some(mut k) = world.get_mut::<Kinematics>(e) {
                    k.ordered_speed_mps = MCM_SPEED_MPS.min(k.max_speed_mps);
                }
            }
            Mission::Cargo {
                origin,
                destination,
                tonnes,
            } => {
                if cargo_step(
                    world,
                    (id, e),
                    pos.point,
                    (*origin, *destination, *tonnes),
                    tick,
                ) {
                    continue;
                }
            }
            Mission::BarrierPatrol { start, end } => {
                let phase = world.get::<MissionState>(e).map_or(0, |m| m.phase);
                let target = if phase == 0 { *start } else { *end };
                if inverse(pos.point, target).distance_m <= ARRIVAL_M {
                    let next = 1 - phase.min(1);
                    if let Some(mut m) = world.get_mut::<MissionState>(e) {
                        m.phase = next;
                    }
                    let to = if next == 0 { *start } else { *end };
                    set_route(world, e, id, to, tick);
                    ensure_moving(world, e, 0.5);
                } else if route_done(world, e) {
                    set_route(world, e, id, target, tick);
                    ensure_moving(world, e, 0.5);
                }
            }
            Mission::Denial { choke_point } => {
                let areas = choke_areas(world, unit.side, choke_point);
                if route_done(world, e) && !areas.is_empty() {
                    let (u1, u2) = {
                        let mut rng = world.resource_mut::<RngStreams>();
                        (rng.uniform(StreamId::Ai), rng.uniform(StreamId::Ai))
                    };
                    if let Some(point) = area_point(&areas, u1, u2) {
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
            }
        }
        if mission.engages() {
            let inside = match &mission {
                Mission::Denial { choke_point } => Some(choke_areas(world, unit.side, choke_point)),
                _ => None,
            };
            auto_engage(
                world,
                id,
                e,
                &unit,
                &mission,
                pos.point,
                inside.as_deref(),
                tick,
            );
        }
    }
}

/// Stops a unit where it is.
pub(crate) fn hold(world: &mut World, e: Entity) {
    if let Some(mut k) = world.get_mut::<Kinematics>(e) {
        k.ordered_speed_mps = 0.0;
    }
    if let Some(mut r) = world.get_mut::<Route>(e) {
        r.waypoints.clear();
        r.next = 0;
    }
}

fn complete(world: &mut World, e: Entity, id: UnitId, tick: Tick) {
    if let Some(mut m) = world.get_mut::<MissionState>(e) {
        m.complete = true;
    }
    world
        .resource_mut::<EventLog>()
        .push(tick, SimEventKind::MissionComplete { unit: id });
}

/// Rounds of the first mine type the unit carries (weapon-id order), as `lay_mines` picks it.
fn mines_carried(world: &World, e: Entity) -> u32 {
    let catalog = &world.resource::<crate::combat::WeaponCatalog>().0;
    world
        .get::<Magazine>(e)
        .and_then(|m| {
            m.0.iter()
                .find(|(w, n)| {
                    **n > 0 && catalog.get(*w).is_some_and(|s| s.kind == WeaponKind::Mine)
                })
                .map(|(_, n)| *n)
        })
        .unwrap_or(0)
}

/// Cargo: to the origin (if any), 30 minutes loading, to the destination, 30 minutes unloading,
/// complete. Phases 0–3 on the mission state. Returns true when the mission completed.
fn cargo_step(
    world: &mut World,
    (id, e): (UnitId, Entity),
    at: GeoPoint,
    (origin, destination, tonnes): (Option<GeoPoint>, GeoPoint, f64),
    tick: Tick,
) -> bool {
    let (mut phase, since) = world
        .get::<MissionState>(e)
        .map_or((0, None), |m| (m.phase, m.since));
    if phase == 0 && origin.is_none() {
        phase = 2;
    }
    let set = |world: &mut World, phase: u8, since: Option<Tick>| {
        if let Some(mut m) = world.get_mut::<MissionState>(e) {
            m.phase = phase;
            m.since = since;
        }
    };
    let handled = |since: Option<Tick>| {
        since.is_some_and(|s| tick.0.saturating_sub(s.0) >= CARGO_HANDLING_TICKS)
    };
    match phase {
        0 | 2 => {
            let to = if phase == 0 {
                origin.unwrap_or(destination)
            } else {
                destination
            };
            if inverse(at, to).distance_m <= ARRIVAL_M {
                hold(world, e);
                set(world, phase + 1, Some(tick));
            } else {
                if route_done(world, e) {
                    set_route(world, e, id, to, tick);
                    ensure_moving(world, e, 0.7);
                }
                set(world, phase, None);
            }
        }
        1 if handled(since) => {
            world
                .resource_mut::<EventLog>()
                .push(tick, SimEventKind::CargoLoaded { unit: id, tonnes });
            set(world, 2, None);
        }
        3 if handled(since) => {
            world
                .resource_mut::<EventLog>()
                .push(tick, SimEventKind::CargoDelivered { unit: id, tonnes });
            complete(world, e, id, tick);
            return true;
        }
        _ => {}
    }
    false
}

/// The areas of `side`'s zones that belong to `choke_point`, in zone-id order.
fn choke_areas(world: &World, side: SideId, choke_point: &str) -> Vec<Area> {
    world
        .resource::<crate::planning::PlanningLayer>()
        .zones
        .iter()
        .filter(|z| z.side == side && z.choke_point.as_deref() == Some(choke_point))
        .map(|z| z.area.clone())
        .collect()
}

/// A random point in one of `areas`: u₁ picks the area (n·u₁) and its fraction is reused for the
/// point, with u₂. `None` if a concave polygon's point falls outside it.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn area_point(areas: &[Area], u1: f64, u2: f64) -> Option<GeoPoint> {
    let n = areas.len() as f64;
    let i = ((u1 * n).floor() as usize).min(areas.len() - 1);
    let frac = (u1 * n - i as f64).clamp(0.0, 1.0);
    match &areas[i] {
        Area::Circle { centre, radius_m } => Some(patrol_point(*centre, *radius_m, frac, u2)),
        a @ Area::Polygon { points } => {
            sim_ai::polygon_point(points, frac, u2).filter(|p| a.contains(*p))
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn auto_engage(
    world: &mut World,
    id: UnitId,
    e: Entity,
    unit: &Unit,
    mission: &Mission,
    from: GeoPoint,
    inside: Option<&[Area]>,
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
        .filter(|(_, c)| {
            inside.is_none_or(|areas| {
                let p = c.estimate_at(tick).position;
                areas.iter().any(|a| a.contains(p))
            })
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
