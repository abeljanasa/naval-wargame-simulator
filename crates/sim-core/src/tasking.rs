//! Automatic sense-zone tasking (Phase 4 slice 12; `docs/models/sensor-tasking.md`).
//!
//! A sense zone may carry a **coverage goal** (a share of its area) and, optionally, the units
//! the engine may use for it. Every [`TASKING_PERIOD_TICKS`], for each such zone in id order, the
//! engine works out the zone's coverage with the coverage model (`coverage.rs`). Below the goal,
//! it tasks sensors one at a time, greedily: the free unit and station that would cover the most
//! cells still uncovered, until the projected coverage meets the goal.
//!
//! - **Free units:** the zone's side's ships and submarines with no mission (or a completed one),
//!   operational and not in an action group, and aircraft parked at the side's airbases (not in
//!   maintenance). A ship takes a Support mission at the station; an aircraft is launched on a
//!   patrol sortie round it.
//! - **Release:** a unit is released when its task ends (another mission, a recall, bingo, loss),
//!   and the next pass tasks another in its place, so aircraft relieve each other on station.
//! - **Every decision is logged**: tasked, goal met, short of the goal with nothing left to send.
//!
//! Distances in the search are on a local flat frame (`libm`), so results are the same on every
//! OS; the coverage the engine reports is the coverage model's own.

use crate::ai::MissionState;
use crate::combat::Health;
use crate::command::{Command, CommandPayload, Issuer};
use crate::components::{Domain, Kinematics, Position, Unit};
use crate::coverage::{unit_reach_m, zone_coverage, zone_kinds};
use crate::detection::SensorSuite;
use crate::events::{EventLog, SimEventKind};
use crate::ids::{SideId, UnitId};
use crate::planning::{PlanningLayer, ZoneKind};
use crate::sim::UnitIndex;
use crate::time::{SimClock, Tick};
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use sim_ai::Mission;
use sim_geo::GeoPoint;
use std::collections::{BTreeMap, BTreeSet};

/// How often the engine reviews sense-zone coverage (5 minutes).
pub const TASKING_PERIOD_TICKS: u64 = 300;
/// Most units tasked to one zone in one pass.
pub const MAX_TASKS_PER_PASS: usize = 4;
/// A tasked ship holds within this distance of its station, m.
pub const STATION_RADIUS_M: f64 = 2_000.0;
/// A tasked aircraft patrols round its station within a quarter of its reach, between these, m.
pub const AIR_ORBIT_MIN_M: f64 = 5_000.0;
pub const AIR_ORBIT_MAX_M: f64 = 30_000.0;
/// Mean earth radius for the local flat frame, m.
const EARTH_R_M: f64 = 6_371_000.0;

/// A unit tasked to a zone, and the mission it was given.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Task {
    pub zone: u32,
    pub mission: Mission,
}

/// Sense-zone tasking state.
#[derive(Debug, Clone, Default, PartialEq, Resource, Serialize, Deserialize)]
pub struct Tasking {
    pub tasked: BTreeMap<UnitId, Task>,
    /// Zones short of their goal with nothing left to send (logged once until met).
    pub short: BTreeSet<u32>,
}

impl Tasking {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tasked.is_empty() && self.short.is_empty()
    }
}

/// A unit the engine could task.
struct Candidate {
    unit: UnitId,
    at: GeoPoint,
    reach_m: f64,
    aircraft: bool,
}

fn flat_m(a: GeoPoint, b: GeoPoint) -> f64 {
    let lat0 = ((a.lat_deg + b.lat_deg) / 2.0).to_radians();
    let mut dlon = b.lon_deg - a.lon_deg;
    if dlon > 180.0 {
        dlon -= 360.0;
    } else if dlon < -180.0 {
        dlon += 360.0;
    }
    libm::hypot(
        dlon.to_radians() * EARTH_R_M * libm::cos(lat0),
        (b.lat_deg - a.lat_deg).to_radians() * EARTH_R_M,
    )
}

/// The best next task: (candidate index, station index, cells it would cover). Ties go to the
/// nearer candidate, then the lower unit id, then the earlier station.
fn best_task(
    candidates: &[Candidate],
    gaps: &[GeoPoint],
    open: &[bool],
) -> Option<(usize, usize, usize)> {
    let mut best: Option<(usize, usize, usize, f64)> = None;
    for (ci, c) in candidates.iter().enumerate() {
        for (si, s) in gaps.iter().enumerate() {
            if !open[si] {
                continue;
            }
            let gain = gaps
                .iter()
                .zip(open)
                .filter(|(g, o)| **o && flat_m(*s, **g) <= c.reach_m)
                .count();
            let transit = flat_m(c.at, *s);
            let better = match best {
                None => gain > 0,
                Some((bc, _, bg, bt)) => {
                    gain > bg
                        || (gain == bg
                            && (transit < bt || (transit == bt && candidates[bc].unit > c.unit)))
                }
            };
            if better {
                best = Some((ci, si, gain, transit));
            }
        }
    }
    best.map(|(c, s, g, _)| (c, s, g))
}

/// Free ships, submarines and parked aircraft of `side` (of `assets`, if given) that have a
/// sensor of `kinds` with some reach, in unit-id order.
fn candidates(
    world: &World,
    side: SideId,
    kinds: &[crate::detection::SensorKind],
    assets: &[UnitId],
    taken: &Tasking,
) -> Vec<Candidate> {
    let allowed =
        |u: UnitId| (assets.is_empty() || assets.contains(&u)) && !taken.tasked.contains_key(&u);
    let mut out = Vec::new();
    let index: Vec<(UnitId, Entity)> = world
        .resource::<UnitIndex>()
        .0
        .iter()
        .map(|(u, e)| (*u, *e))
        .collect();
    for (id, e) in index {
        let (Some(u), Some(p), Some(k), Some(s), Some(m), Some(h)) = (
            world.get::<Unit>(e),
            world.get::<Position>(e),
            world.get::<Kinematics>(e),
            world.get::<SensorSuite>(e),
            world.get::<MissionState>(e),
            world.get::<Health>(e),
        ) else {
            continue;
        };
        let free = (m.mission.is_none() || m.complete)
            && u.group.is_none()
            && h.status == sim_weapons::Status::Operational
            && matches!(u.domain, Domain::Surface | Domain::Subsurface)
            && k.max_speed_mps > 0.0;
        if u.side != side || !free || !allowed(id) {
            continue;
        }
        let reach_m = unit_reach_m(world, u, p, k.speed_mps, &s.0, kinds);
        if reach_m > 0.0 {
            out.push(Candidate {
                unit: id,
                at: p.point,
                reach_m,
                aircraft: false,
            });
        }
    }
    // Parked aircraft, flying at their cruise altitude and speed.
    let air = world.resource::<crate::air::Air>();
    for (id, r) in &air.parked {
        let Some(c) = air.aircraft.get(id) else {
            continue;
        };
        let ready = matches!(c.state, crate::air::AirState::Parked { .. })
            && c.readiness != crate::air::Readiness::Maintenance
            && r.health
                .as_ref()
                .is_none_or(|h| h.status == sim_weapons::Status::Operational);
        if r.unit.side != side || !ready || !allowed(*id) {
            continue;
        }
        let flying = Position {
            point: r.position.point,
            altitude_m: c.spec.cruise_altitude_m,
        };
        let reach_m = unit_reach_m(
            world,
            &r.unit,
            &flying,
            c.spec.cruise_mps,
            &r.sensors.0,
            kinds,
        );
        if reach_m > 0.0 {
            out.push(Candidate {
                unit: *id,
                at: r.position.point,
                reach_m,
                aircraft: true,
            });
        }
    }
    out.sort_by_key(|c| c.unit);
    out
}

/// Whether a task is still being carried out: the unit has its mission (on the map, or waiting
/// to launch with it).
fn still_on(world: &World, unit: UnitId, task: &Task) -> bool {
    if let Some(e) = world.resource::<UnitIndex>().0.get(&unit) {
        return world
            .get::<MissionState>(*e)
            .is_some_and(|m| !m.complete && m.mission.as_ref() == Some(&task.mission));
    }
    world
        .resource::<crate::air::Air>()
        .aircraft
        .get(&unit)
        .is_some_and(|a| {
            matches!(&a.state, crate::air::AirState::Launching { mission, .. } if *mission == task.mission)
        })
}

/// The tasking pass (see the module docs).
#[allow(clippy::too_many_lines)]
pub(crate) fn tasking_system(world: &mut World) {
    let tick = world.resource::<SimClock>().tick;
    if !tick.0.is_multiple_of(TASKING_PERIOD_TICKS) {
        return;
    }
    let zones: Vec<crate::planning::Zone> = world
        .resource::<PlanningLayer>()
        .zones
        .iter()
        .filter(|z| z.kind == ZoneKind::Sense && z.coverage_goal.is_some())
        .cloned()
        .collect();
    // Release units whose task has ended, or whose zone no longer asks for coverage.
    let ended: Vec<UnitId> = world
        .resource::<Tasking>()
        .tasked
        .iter()
        .filter(|(u, t)| !zones.iter().any(|z| z.id == t.zone) || !still_on(world, **u, t))
        .map(|(u, _)| *u)
        .collect();
    {
        let mut tasking = world.resource_mut::<Tasking>();
        for u in ended {
            tasking.tasked.remove(&u);
        }
        tasking.short.retain(|z| zones.iter().any(|g| g.id == *z));
    }
    if zones.is_empty() {
        return;
    }
    for z in zones {
        let goal = z.coverage_goal.unwrap_or(0.0);
        let Some(cov) = zone_coverage(world, z.id) else {
            continue;
        };
        if cov.fraction >= goal {
            if world.resource_mut::<Tasking>().short.remove(&z.id) {
                world.resource_mut::<EventLog>().push(
                    tick,
                    SimEventKind::CoverageGoalMet {
                        zone: z.id,
                        coverage: cov.fraction,
                    },
                );
            }
            continue;
        }
        let kinds = zone_kinds(&z);
        let mut pool = candidates(
            world,
            z.side,
            &kinds,
            &z.assets,
            world.resource::<Tasking>(),
        );
        let gaps: Vec<GeoPoint> = cov
            .cells
            .iter()
            .filter(|c| !c.covered)
            .map(|c| c.centre)
            .collect();
        let mut open = vec![true; gaps.len()];
        #[allow(clippy::cast_precision_loss)]
        let total = cov.cells.len().max(1) as f64;
        let mut covered = cov.cells.iter().filter(|c| c.covered).count();
        let mut sent = 0;
        #[allow(clippy::cast_precision_loss)]
        while (covered as f64) / total < goal && sent < MAX_TASKS_PER_PASS {
            let Some((ci, si, gain)) = best_task(&pool, &gaps, &open) else {
                break;
            };
            let c = pool.remove(ci);
            let station = gaps[si];
            for (g, o) in gaps.iter().zip(open.iter_mut()) {
                if *o && flat_m(station, *g) <= c.reach_m {
                    *o = false;
                }
            }
            let before = covered;
            covered += gain;
            let mission = if c.aircraft {
                Mission::Patrol {
                    centre: station,
                    radius_m: (c.reach_m / 4.0).clamp(AIR_ORBIT_MIN_M, AIR_ORBIT_MAX_M),
                }
            } else {
                Mission::Support {
                    station,
                    radius_m: STATION_RADIUS_M,
                }
            };
            let payload = if c.aircraft {
                CommandPayload::LaunchSortie {
                    unit: c.unit,
                    mission: mission.clone(),
                    loadout: None,
                }
            } else {
                CommandPayload::AssignMission {
                    unit: c.unit,
                    mission: Some(mission.clone()),
                }
            };
            let order = Command {
                tick,
                issuer: Issuer::Ai,
                side: Some(z.side),
                payload,
            };
            if crate::sim::apply_command(world, &order).is_err() {
                continue;
            }
            world.resource_mut::<Tasking>().tasked.insert(
                c.unit,
                Task {
                    zone: z.id,
                    mission,
                },
            );
            #[allow(clippy::cast_precision_loss)]
            world.resource_mut::<EventLog>().push(
                tick,
                SimEventKind::SensorTasked {
                    zone: z.id,
                    unit: c.unit,
                    station,
                    reach_m: c.reach_m,
                    coverage: before as f64 / total,
                    projected: covered as f64 / total,
                },
            );
            sent += 1;
        }
        #[allow(clippy::cast_precision_loss)]
        let projected = covered as f64 / total;
        if projected < goal && sent == 0 && world.resource_mut::<Tasking>().short.insert(z.id) {
            world.resource_mut::<EventLog>().push(
                tick,
                SimEventKind::CoverageShortfall {
                    zone: z.id,
                    coverage: cov.fraction,
                    goal,
                },
            );
        }
    }
}

/// A sense zone's tasking, as a seat sees it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TaskingView {
    pub zone: u32,
    pub goal: f64,
    /// Units tasked to it, in unit-id order.
    pub units: Vec<UnitId>,
    /// Short of its goal with nothing left to send.
    pub short: bool,
}

/// The tasking of `side`'s sense zones (all for the white cell).
pub(crate) fn views(world: &World, side: Option<SideId>) -> Vec<TaskingView> {
    let t = world.resource::<Tasking>();
    world
        .resource::<PlanningLayer>()
        .zones
        .iter()
        .filter(|z| z.kind == ZoneKind::Sense && side.is_none_or(|s| s == z.side))
        .filter_map(|z| {
            Some(TaskingView {
                zone: z.id,
                goal: z.coverage_goal?,
                units: t
                    .tasked
                    .iter()
                    .filter(|(_, k)| k.zone == z.id)
                    .map(|(u, _)| *u)
                    .collect(),
                short: t.short.contains(&z.id),
            })
        })
        .collect()
}

/// When the next review is due (for the panel).
#[must_use]
pub fn next_review(now: Tick) -> Tick {
    Tick((now.0 / TASKING_PERIOD_TICKS + 1) * TASKING_PERIOD_TICKS)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(unit: u64, lat: f64, lon: f64, reach_m: f64) -> Candidate {
        Candidate {
            unit: UnitId(unit),
            at: GeoPoint::new(lat, lon),
            reach_m,
            aircraft: false,
        }
    }

    /// A row of gaps 0.1° (11.1 km) apart along the equator.
    fn row(n: usize) -> Vec<GeoPoint> {
        #[allow(clippy::cast_precision_loss)]
        (0..n)
            .map(|i| GeoPoint::new(0.0, -170.0 + 0.1 * i as f64))
            .collect()
    }

    #[test]
    fn the_widest_reach_goes_where_it_covers_most() {
        let gaps = row(9);
        let open = vec![true; 9];
        // A 25 km sensor centred on the row covers five cells (±2 × 11.1 km); a 12 km one, three.
        let pool = [
            cand(1, 0.0, -171.0, 12_000.0),
            cand(2, 0.0, -172.0, 25_000.0),
        ];
        let (c, s, gain) = best_task(&pool, &gaps, &open).unwrap();
        assert_eq!(pool[c].unit, UnitId(2));
        assert_eq!(gain, 5);
        assert_eq!(s, 2, "the first station that covers five cells");
    }

    #[test]
    fn ties_go_to_the_nearer_unit_and_nothing_to_gain_is_no_task() {
        let gaps = row(3);
        let open = vec![true; 3];
        let pool = [
            cand(1, 1.0, -170.1, 15_000.0),
            cand(2, 0.2, -170.1, 15_000.0),
        ];
        let (c, _, _) = best_task(&pool, &gaps, &open).unwrap();
        assert_eq!(pool[c].unit, UnitId(2), "nearer");
        assert!(best_task(&pool, &gaps, &[false; 3]).is_none());
    }

    #[test]
    fn reviews_every_five_minutes() {
        assert_eq!(next_review(Tick(0)), Tick(300));
        assert_eq!(next_review(Tick(299)), Tick(300));
        assert_eq!(next_review(Tick(300)), Tick(600));
    }
}
