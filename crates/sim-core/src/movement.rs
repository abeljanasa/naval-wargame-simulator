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
        }
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
