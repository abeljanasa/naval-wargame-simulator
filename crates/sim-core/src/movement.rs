//! Movement model, Phase 1 (`docs/models/geodesy-and-movement.md`).
//!
//! Each tick, for every unit in `UnitId` order:
//! 1. Speed moves toward the ordered speed (clamped to `[0, max]`) by at most `accel·dt`.
//! 2. The distance budget is `speed·dt` (semi-implicit Euler: the new speed is used).
//! 3. With an active route, the unit follows the WGS-84 geodesic to the current waypoint. If it
//!    reaches the waypoint inside the tick, the leftover distance carries onto the next leg. On the
//!    final waypoint it comes to all-stop (ordered and current speed set to 0).
//! 4. Without a route, it dead-reckons along its heading.
//!
//! Land: routes are planned round the coastline when ordered (`sim_geo::land`), and a ship or
//! submarine that would still move onto land stops instead (`LandAhead`). With terrain packs, one
//! that would move into water shallower than its draft stops too (`ShoalAhead`,
//! `docs/models/shallow-water.md`).
//! Fuel (`docs/models/fuel.md`): a unit with a fuel model burns fuel by speed after it moves; out
//! of fuel, its ordered speed counts as zero and it coasts to a stop.
//! Heavy seas (`docs/models/weather.md`) cut a surface hull's top speed near its weather limit.
//! Collisions (`docs/models/collisions.md`): before stepping, each ship or submarine gives way to
//! the hulls the rules of the road make it give way to, turning to starboard (or slowing), then
//! comes back to its route or ordered heading; after stepping, hulls that came into contact are
//! handed to the collision system for damage.

use crate::components::{Domain, Kinematics, Position, Route, Unit};
use crate::events::{EventLog, SimEventKind};
use crate::fuel::{burn_t_per_s, Fuel};
use crate::sim::{LandMaskRes, TerrainRes};
use crate::time::{SimClock, TICK_SECONDS};
use bevy_ecs::prelude::*;
use sim_geo::{direct, inverse};

/// Distance below which a waypoint counts as reached, metres. This absorbs floating-point residue.
const ARRIVAL_EPSILON_M: f64 = 1e-3;

/// What the movement system reads and writes for each unit.
type MovingUnit = (
    Entity,
    &'static Unit,
    &'static mut Position,
    &'static mut Kinematics,
    &'static mut Route,
    Option<&'static mut Fuel>,
    Option<&'static crate::detection::Signature>,
);

#[allow(clippy::too_many_arguments)] // a Bevy system: each resource is a parameter
pub(crate) fn movement_system(
    clock: Res<SimClock>,
    land: Res<LandMaskRes>,
    terrain: Res<TerrainRes>,
    env: Res<crate::detection::EnvState>,
    weather: Res<crate::detection::Weather>,
    mut log: ResMut<EventLog>,
    mut avoidance: ResMut<crate::collision::Avoidance>,
    mut pending: ResMut<crate::collision::PendingCollisions>,
    mut units: Query<MovingUnit>,
) {
    let tick = clock.tick;
    let mut order: Vec<_> = units.iter().map(|(e, u, ..)| (u.id, e)).collect();
    order.sort_unstable();
    // Hulls as the tick starts (`docs/models/collisions.md`): what each one avoids, and where
    // each one was, to find hulls that come into contact during it.
    let hulls = |units: &Query<MovingUnit>| {
        let mut v: Vec<crate::collision::Hull> = units
            .iter()
            .filter_map(|(_, u, p, k, _, _, s)| crate::collision::hull(u, p, k, s))
            .collect();
        v.sort_by_key(|h| h.id);
        v
    };
    let start = hulls(&units);

    for (_, entity) in order {
        let Ok((_, unit, mut pos, mut kin, mut route, mut fuel, sig)) = units.get_mut(entity)
        else {
            continue;
        };
        let before = (*pos, kin.heading_deg);
        // Out of fuel: no propulsion this tick. The order is kept for when it is refuelled.
        let dry = fuel.as_ref().is_some_and(|f| f.empty());
        let ordered = kin.ordered_speed_mps;
        if dry {
            kin.ordered_speed_mps = 0.0;
        }
        let mut finished = false;
        // Heavy seas (`docs/models/weather.md`): a surface hull makes only part of its top speed
        // near and above its weather limit, in the sea where it is.
        let top = kin.max_speed_mps;
        if unit.domain == Domain::Surface {
            let sea = crate::detection::local_env(env.0, &weather.0, pos.point).sea_state;
            let length = sig
                .and_then(|s| s.length_m)
                .unwrap_or_else(|| crate::optical::default_length_m(unit.domain));
            kin.max_speed_mps = top
                * sim_events::weather::speed_factor(sea, crate::planning::weather_limit(length));
        }
        // Collision avoidance: give way by the rules of the road, then come back to the route,
        // or to the heading the unit was ordered to hold.
        let held = avoidance.0.get(&unit.id).copied();
        let routed = route.is_active();
        let goal = match route.current() {
            Some(wp) => inverse(pos.point, wp).azimuth1_deg,
            None => held.and_then(|h| h.resume_deg).unwrap_or(kin.heading_deg),
        };
        let steer = start
            .iter()
            .find(|h| h.id == unit.id)
            .and_then(|me| crate::collision::avoid(me, goal, &start, held.and_then(|h| h.other)));
        let mut heading = steer.map(|s| s.heading_deg);
        match steer {
            Some(s) => {
                let was = held.and_then(|h| h.other);
                avoidance.0.insert(
                    unit.id,
                    crate::collision::AvoidState {
                        other: Some(s.other),
                        resume_deg: (!routed).then_some(goal),
                    },
                );
                if was != Some(s.other) {
                    log.push(
                        tick,
                        SimEventKind::CollisionAvoidance {
                            unit: unit.id,
                            other: s.other,
                            turn_deg: s.turn_deg,
                            slowed: s.slow,
                        },
                    );
                }
                if s.slow {
                    kin.max_speed_mps = kin.max_speed_mps.min(top * crate::collision::SLOW_FACTOR);
                }
            }
            None => match held.and_then(|h| h.resume_deg) {
                Some(r)
                    if !routed
                        && crate::collision::heading_gap(kin.heading_deg, r).abs()
                            > crate::collision::RESUMED_DEG =>
                {
                    avoidance.0.insert(
                        unit.id,
                        crate::collision::AvoidState {
                            other: None,
                            resume_deg: Some(r),
                        },
                    );
                    heading = Some(r);
                }
                _ => {
                    avoidance.0.remove(&unit.id);
                }
            },
        }
        step_unit(&mut pos, &mut kin, &mut route, heading, |idx, done| {
            finished |= done;
            #[allow(clippy::cast_possible_truncation)] // routes are far shorter than u32::MAX
            log.push(
                tick,
                SimEventKind::WaypointReached {
                    unit: unit.id,
                    index: idx as u32,
                },
            );
            if done {
                log.push(tick, SimEventKind::RouteCompleted { unit: unit.id });
            }
        });
        // Grounding guard: a ship or submarine that would move from water onto land stops at the
        // last water position instead (routes are planned round land; this catches dead reckoning
        // and corners cut while turning).
        if let (Some(mask), Domain::Surface | Domain::Subsurface) = (&land.0, unit.domain) {
            if mask.is_land(pos.point) && !mask.is_land(before.0.point) {
                *pos = before.0;
                kin.heading_deg = before.1;
                kin.speed_mps = 0.0;
                kin.ordered_speed_mps = 0.0;
                log.push(tick, SimEventKind::LandAhead { unit: unit.id });
            }
        }
        // Shoal guard (terrain packs), off the navigable graph only: a ship or submarine with no
        // charted route (none, finished, uncharted, or one with a shallow leg) that would move
        // into water shallower than its draft, and shallower than where it was, stops at its last
        // position. It can still leave shallow water it is already in towards deeper water. A
        // charted route is trusted, including its harbour ends, which the planner exempts because
        // the bathymetry is coarsest at the coast.
        let on_graph = route.charted && route.is_active();
        if let (Some(t), Some(draft), Domain::Surface | Domain::Subsurface, false) =
            (&terrain.0, unit.draft_m, unit.domain, on_graph)
        {
            if let (Some(now), Some(was)) = (t.depth_m(pos.point), t.depth_m(before.0.point)) {
                if now < draft && now < was {
                    *pos = before.0;
                    kin.heading_deg = before.1;
                    kin.speed_mps = 0.0;
                    kin.ordered_speed_mps = 0.0;
                    log.push(
                        tick,
                        SimEventKind::ShoalAhead {
                            unit: unit.id,
                            depth_m: now,
                        },
                    );
                }
            }
        }
        kin.max_speed_mps = top;
        if dry && !finished {
            kin.ordered_speed_mps = ordered;
        }
        if let Some(f) = fuel.as_mut() {
            if !f.empty() {
                f.fuel_t = (f.fuel_t - burn_t_per_s(f, kin.speed_mps) * TICK_SECONDS).max(0.0);
                if f.empty() {
                    log.push(tick, SimEventKind::FuelExhausted { unit: unit.id });
                }
            }
        }
    }
    // Hulls gone (destroyed, removed) stop avoiding; contacts go to the collision system.
    let end = hulls(&units);
    avoidance.0.retain(|u, _| end.iter().any(|h| h.id == *u));
    for s in avoidance.0.values_mut() {
        if s.other.is_some_and(|o| !end.iter().any(|h| h.id == o)) {
            s.other = None;
        }
    }
    pending.0 = crate::collision::new_contacts(&start, &end);
}

/// Advances one unit by one tick. `on_waypoint(index, route_finished)` is called for each arrival.
/// `steer`: a heading to make good instead of the route this tick (collision avoidance).
pub(crate) fn step_unit(
    pos: &mut Position,
    kin: &mut Kinematics,
    route: &mut Route,
    steer: Option<f64>,
    mut on_waypoint: impl FnMut(usize, bool),
) {
    let dt = TICK_SECONDS;
    let target = kin.ordered_speed_mps.clamp(0.0, kin.max_speed_mps);
    let max_dv = kin.accel_mps2 * dt;
    kin.speed_mps += (target - kin.speed_mps).clamp(-max_dv, max_dv);
    kin.speed_mps = kin.speed_mps.max(0.0);

    let mut remaining = kin.speed_mps * dt;
    if remaining <= 0.0 {
        return;
    }

    if let Some(max_rate_dps) = kin.max_turn_rate_dps {
        step_turn_limited(
            pos,
            kin,
            route,
            max_rate_dps,
            remaining,
            steer,
            &mut on_waypoint,
        );
        return;
    }

    if let Some(h) = steer {
        let d = direct(pos.point, h, remaining);
        pos.point = d.point;
        kin.heading_deg = d.azimuth2_deg;
        return;
    }

    if !route.is_active() {
        let d = direct(pos.point, kin.heading_deg, remaining);
        pos.point = d.point;
        kin.heading_deg = d.azimuth2_deg;
        return;
    }

    // Each loop iteration either consumes the whole budget or advances `route.next`, so the loop
    // runs at most `waypoints.len() + 1` times.
    while let Some(wp) = route.current() {
        let leg = inverse(pos.point, wp);
        if leg.distance_m <= remaining + ARRIVAL_EPSILON_M {
            remaining = (remaining - leg.distance_m).max(0.0);
            pos.point = wp;
            if leg.distance_m > 0.0 {
                kin.heading_deg = leg.azimuth2_deg;
            }
            let idx = route.next;
            route.next += 1;
            let finished = !route.is_active();
            on_waypoint(idx, finished);
            if finished {
                kin.ordered_speed_mps = 0.0;
                kin.speed_mps = 0.0;
                return;
            }
            if remaining <= 0.0 {
                return;
            }
        } else {
            let d = direct(pos.point, leg.azimuth1_deg, remaining);
            pos.point = d.point;
            kin.heading_deg = d.azimuth2_deg;
            return;
        }
    }
}

/// Achievable turn rate (deg/s) at `speed`: min(max rate, speed / minimum turn radius).
/// With no radius limit the max rate applies at any speed above zero; at zero speed nothing turns.
#[must_use]
pub fn turn_rate_dps(max_rate_dps: f64, min_turn_radius_m: f64, speed_mps: f64) -> f64 {
    if speed_mps <= 0.0 {
        return 0.0;
    }
    if min_turn_radius_m > 0.0 {
        max_rate_dps.min((speed_mps / min_turn_radius_m).to_degrees())
    } else {
        max_rate_dps
    }
}

/// Signed smallest rotation from heading `from` to `to`, degrees in (−180, 180].
fn heading_delta(from: f64, to: f64) -> f64 {
    let d = (to - from) % 360.0;
    if d > 180.0 {
        d - 360.0
    } else if d <= -180.0 {
        d + 360.0
    } else {
        d
    }
}

/// Turn-limited movement: steer toward the current waypoint at the achievable turn rate, then move
/// along the new heading. A waypoint counts as reached within its arrival radius (at least the turn
/// radius, so a unit never circles a point it cannot hit); the unit is not snapped onto it.
fn step_turn_limited(
    pos: &mut Position,
    kin: &mut Kinematics,
    route: &mut Route,
    max_rate_dps: f64,
    distance_m: f64,
    steer: Option<f64>,
    on_waypoint: &mut impl FnMut(usize, bool),
) {
    let arrival_m = kin
        .min_turn_radius_m
        .max(MIN_ARRIVAL_RADIUS_M)
        .max(distance_m);
    // Retire every waypoint already inside the arrival radius (bounded by the route length).
    while let Some(wp) = route.current() {
        if inverse(pos.point, wp).distance_m > arrival_m {
            break;
        }
        let idx = route.next;
        route.next += 1;
        let finished = !route.is_active();
        on_waypoint(idx, finished);
        if finished {
            kin.ordered_speed_mps = 0.0;
            kin.speed_mps = 0.0;
            return;
        }
    }
    let desired = steer.or_else(|| {
        route
            .current()
            .map(|wp| inverse(pos.point, wp).azimuth1_deg)
    });
    if let Some(desired) = desired {
        let max_turn =
            turn_rate_dps(max_rate_dps, kin.min_turn_radius_m, kin.speed_mps) * TICK_SECONDS;
        let delta = heading_delta(kin.heading_deg, desired).clamp(-max_turn, max_turn);
        kin.heading_deg = sim_geo::normalize_azimuth_deg(kin.heading_deg + delta);
    }
    let d = direct(pos.point, kin.heading_deg, distance_m);
    pos.point = d.point;
    kin.heading_deg = d.azimuth2_deg;
}

/// Smallest arrival radius for turn-limited units, metres.
const MIN_ARRIVAL_RADIUS_M: f64 = 50.0;

#[cfg(test)]
mod tests {
    use super::*;
    use sim_geo::GeoPoint;

    fn kin(speed: f64) -> Kinematics {
        Kinematics {
            heading_deg: 0.0,
            speed_mps: speed,
            ordered_speed_mps: speed,
            max_speed_mps: 15.0,
            accel_mps2: 0.1,
            max_turn_rate_dps: None,
            min_turn_radius_m: 0.0,
        }
    }

    #[test]
    fn turn_rate_is_limited_by_radius_and_cap() {
        // 10 m/s on a 400 m radius: 0.025 rad/s = 1.4324°/s, under the 3°/s cap.
        assert!((turn_rate_dps(3.0, 400.0, 10.0) - 0.025_f64.to_degrees()).abs() < 1e-12);
        // Fast enough that the cap binds.
        assert!((turn_rate_dps(3.0, 100.0, 15.0) - 3.0).abs() < 1e-12);
        assert_eq!(turn_rate_dps(3.0, 400.0, 0.0), 0.0);
        assert!((heading_delta(350.0, 10.0) - 20.0).abs() < 1e-12);
        assert!((heading_delta(10.0, 350.0) + 20.0).abs() < 1e-12);
    }

    #[test]
    fn turn_limited_unit_turns_gradually_and_reaches_a_waypoint_behind_it() {
        // Heading north at 10 m/s with a 400 m turn radius; waypoint 5 km due south.
        let start = GeoPoint::new(0.0, 0.0);
        let wp = direct(start, 180.0, 5_000.0).point;
        let mut p = Position {
            point: start,
            altitude_m: 0.0,
        };
        let mut k = Kinematics {
            max_turn_rate_dps: Some(3.0),
            min_turn_radius_m: 400.0,
            ..kin(10.0)
        };
        let mut r = Route {
            waypoints: vec![wp],
            next: 0,
            charted: false,
        };
        step_unit(&mut p, &mut k, &mut r, None, |_, _| {});
        // One second at 1.4324°/s: it cannot reverse course instantly.
        assert!(
            k.heading_deg < 2.0 || k.heading_deg > 358.0,
            "heading {}",
            k.heading_deg
        );
        // A 180° turn at 1.4324°/s takes about 126 s, then ~5 km to run: arrives well within 1,000 s.
        let mut arrived = None;
        for t in 1..1_000 {
            step_unit(&mut p, &mut k, &mut r, None, |_, done| {
                if done {
                    arrived = Some(t);
                }
            });
            if arrived.is_some() {
                break;
            }
        }
        let t = arrived.expect("never reached the waypoint");
        assert!((500..700).contains(&t), "arrived at {t} s");
        assert!(inverse(p.point, wp).distance_m <= 400.0);
        assert_eq!(k.speed_mps, 0.0);
    }

    #[test]
    fn dead_reckoning_north_one_tick() {
        let mut p = Position {
            point: GeoPoint::new(0.0, 0.0),
            altitude_m: 0.0,
        };
        let mut k = kin(10.0);
        let mut r = Route::default();
        step_unit(&mut p, &mut k, &mut r, None, |_, _| {});
        let moved = inverse(GeoPoint::new(0.0, 0.0), p.point).distance_m;
        assert!((moved - 10.0).abs() < 1e-6, "moved {moved}");
        assert!(p.point.lon_deg.abs() < 1e-12);
    }

    #[test]
    fn acceleration_is_limited() {
        let mut p = Position {
            point: GeoPoint::new(0.0, 0.0),
            altitude_m: 0.0,
        };
        let mut k = kin(0.0);
        k.ordered_speed_mps = 10.0;
        let mut r = Route::default();
        step_unit(&mut p, &mut k, &mut r, None, |_, _| {});
        assert!((k.speed_mps - 0.1).abs() < 1e-12);
        // 0.1 m/s² from rest → 10 m/s after 100 ticks.
        for _ in 0..99 {
            step_unit(&mut p, &mut k, &mut r, None, |_, _| {});
        }
        assert!((k.speed_mps - 10.0).abs() < 1e-9);
    }

    #[test]
    fn ordered_speed_clamped_to_max() {
        let mut p = Position {
            point: GeoPoint::new(0.0, 0.0),
            altitude_m: 0.0,
        };
        let mut k = kin(15.0);
        k.ordered_speed_mps = 40.0;
        step_unit(&mut p, &mut k, &mut Route::default(), None, |_, _| {});
        assert!((k.speed_mps - 15.0).abs() < 1e-12);
    }

    #[test]
    fn leftover_distance_carries_to_next_leg_and_stops_at_end() {
        // Two waypoints 6 m and 12 m north of the start; speed 10 m/s.
        let start = GeoPoint::new(0.0, 0.0);
        let w1 = direct(start, 0.0, 6.0).point;
        let w2 = direct(start, 0.0, 12.0).point;
        let mut p = Position {
            point: start,
            altitude_m: 0.0,
        };
        let mut k = kin(10.0);
        let mut r = Route {
            waypoints: vec![w1, w2],
            next: 0,
            charted: false,
        };
        let mut arrivals = vec![];
        step_unit(&mut p, &mut k, &mut r, None, |i, d| arrivals.push((i, d)));
        assert_eq!(arrivals, vec![(0, false)]);
        let travelled = inverse(start, p.point).distance_m;
        assert!((travelled - 10.0).abs() < 1e-6, "travelled {travelled}");
        step_unit(&mut p, &mut k, &mut r, None, |i, d| arrivals.push((i, d)));
        assert_eq!(arrivals, vec![(0, false), (1, true)]);
        assert_eq!(p.point, w2);
        assert_eq!(k.speed_mps, 0.0);
        assert!(!r.is_active());
    }

    #[test]
    fn hand_calculated_transit_time() {
        // 10 km leg east along the equator at 10 m/s (already at speed) → 1000 ticks exactly.
        let start = GeoPoint::new(0.0, 0.0);
        let wp = direct(start, 90.0, 10_000.0).point;
        let mut p = Position {
            point: start,
            altitude_m: 0.0,
        };
        let mut k = kin(10.0);
        let mut r = Route {
            waypoints: vec![wp],
            next: 0,
            charted: false,
        };
        let mut ticks = 0;
        while r.is_active() {
            step_unit(&mut p, &mut k, &mut r, None, |_, _| {});
            ticks += 1;
            assert!(ticks <= 1001);
        }
        assert_eq!(ticks, 1000);
    }
}
