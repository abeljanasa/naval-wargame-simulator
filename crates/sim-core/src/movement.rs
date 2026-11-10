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
//! Not yet modelled (see `docs/backlog.md`): turn-rate limits, land/shallow-water blocking,
//! sea-state penalties, fuel.

use crate::components::{Kinematics, Position, Route, Unit};
use crate::events::{EventLog, SimEventKind};
use crate::time::{SimClock, TICK_SECONDS};
use bevy_ecs::prelude::*;
use sim_geo::{direct, inverse};

/// Distance below which a waypoint counts as reached, metres. This absorbs floating-point residue.
const ARRIVAL_EPSILON_M: f64 = 1e-3;

pub(crate) fn movement_system(
    clock: Res<SimClock>,
    mut log: ResMut<EventLog>,
    mut units: Query<(Entity, &Unit, &mut Position, &mut Kinematics, &mut Route)>,
) {
    let tick = clock.tick;
    let mut order: Vec<_> = units.iter().map(|(e, u, ..)| (u.id, e)).collect();
    order.sort_unstable();

    for (_, entity) in order {
        let Ok((_, unit, mut pos, mut kin, mut route)) = units.get_mut(entity) else {
            continue;
        };
        step_unit(&mut pos, &mut kin, &mut route, |idx, done| {
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
    }
}

/// Advances one unit by one tick. `on_waypoint(index, route_finished)` is called for each arrival.
pub(crate) fn step_unit(
    pos: &mut Position,
    kin: &mut Kinematics,
    route: &mut Route,
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
        step_turn_limited(pos, kin, route, max_rate_dps, remaining, &mut on_waypoint);
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
    if let Some(wp) = route.current() {
        let desired = inverse(pos.point, wp).azimuth1_deg;
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
        };
        step_unit(&mut p, &mut k, &mut r, |_, _| {});
        // One second at 1.4324°/s: it cannot reverse course instantly.
        assert!(
            k.heading_deg < 2.0 || k.heading_deg > 358.0,
            "heading {}",
            k.heading_deg
        );
        // A 180° turn at 1.4324°/s takes about 126 s, then ~5 km to run: arrives well within 1,000 s.
        let mut arrived = None;
        for t in 1..1_000 {
            step_unit(&mut p, &mut k, &mut r, |_, done| {
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
        step_unit(&mut p, &mut k, &mut r, |_, _| {});
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
        step_unit(&mut p, &mut k, &mut r, |_, _| {});
        assert!((k.speed_mps - 0.1).abs() < 1e-12);
        // 0.1 m/s² from rest → 10 m/s after 100 ticks.
        for _ in 0..99 {
            step_unit(&mut p, &mut k, &mut r, |_, _| {});
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
        step_unit(&mut p, &mut k, &mut Route::default(), |_, _| {});
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
        };
        let mut arrivals = vec![];
        step_unit(&mut p, &mut k, &mut r, |i, d| arrivals.push((i, d)));
        assert_eq!(arrivals, vec![(0, false)]);
        let travelled = inverse(start, p.point).distance_m;
        assert!((travelled - 10.0).abs() < 1e-6, "travelled {travelled}");
        step_unit(&mut p, &mut k, &mut r, |i, d| arrivals.push((i, d)));
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
        };
        let mut ticks = 0;
        while r.is_active() {
            step_unit(&mut p, &mut k, &mut r, |_, _| {});
            ticks += 1;
            assert!(ticks <= 1001);
        }
        assert_eq!(ticks, 1000);
    }
}
