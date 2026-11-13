//! Ship collisions and collision avoidance (`docs/models/collisions.md`).
//!
//! Every ship and submarine is a hull: a length, a mass and a band of depth (a ship from its
//! superstructure to its keel, a submarine round its depth). Two hulls whose depth bands overlap
//! **collide** when they come within half their combined lengths. Each takes damage by the
//! closing speed and the other's share of their combined mass, through the same damage roll as a
//! weapon hit.
//!
//! Before that, each moving hull **avoids** others the rules of the road make it give way to: the
//! closest point of approach on its intended course is checked against a safe passing distance,
//! and the give-way hull takes the smallest turn to starboard that clears every hull it knows
//! about, or slows if none up to 90° does. At close quarters, every hull acts.
//!
//! All arithmetic is plain or `libm`, over a local flat frame (the distances are a few
//! kilometres), so results are the same on every OS.

use crate::components::{Domain, Kinematics, Position, Unit};
use crate::detection::Signature;
use crate::ids::{SideId, UnitId};
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use sim_geo::GeoPoint;
use std::collections::BTreeMap;

/// How far ahead hulls look for a close passage, s.
pub const LOOKAHEAD_S: f64 = 360.0;
/// Least safe passing distance, m (more for long hulls: twice their combined lengths).
pub const MIN_PASSING_M: f64 = 500.0;
/// Close quarters: a passage this soon makes every hull act, stand-on or not, s.
pub const CLOSE_QUARTERS_S: f64 = 90.0;
/// Closing speed at which a hull struck by an equal mass takes a quarter of its damage points,
/// and by a far heavier one all of them, m/s (about 27 kn).
pub const V_CRIT_MPS: f64 = 14.0;
/// Below this share of its damage points, a collision dents a hull without a damage roll.
pub const MIN_DAMAGE_FRACTION: f64 = 0.01;
/// From this share up, a collision holes the hull below the waterline (flooding as a torpedo).
pub const HOLING_FRACTION: f64 = 0.1;
/// A hull that cannot clear by turning slows to this share of its top speed.
pub const SLOW_FACTOR: f64 = 0.25;
/// Draft of a ship without one in the database, m.
pub const DEFAULT_DRAFT_M: f64 = 6.0;
/// Height of a ship's superstructure above the water, m (for submarines rising under it).
const SHIP_ABOVE_M: f64 = 30.0;
/// A submarine's extent above (sail) and below (keel) its depth, m.
const SUB_ABOVE_M: f64 = 10.0;
const SUB_BELOW_M: f64 = 6.0;
/// Turn steps tried when giving way, degrees, and the most tried.
const TURN_STEP_DEG: f64 = 10.0;
const TURN_MAX_DEG: f64 = 90.0;
/// A hull already giving way keeps on until the passage clears this multiple of the safe
/// passing distance (hysteresis).
pub const HOLD_MARGIN: f64 = 1.25;
/// Below this speed a hull is stopped: it cannot give way, and others give way to it, m/s.
const STOPPED_MPS: f64 = 0.5;
/// Mean earth radius for the local flat frame, m.
const EARTH_R_M: f64 = 6_371_000.0;

/// A ship or submarine, as collisions and avoidance see it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Hull {
    pub id: UnitId,
    pub side: SideId,
    pub domain: Domain,
    pub point: GeoPoint,
    pub heading_deg: f64,
    pub speed_mps: f64,
    pub length_m: f64,
    pub mass_t: f64,
    /// Depth band, m, positive down (`top` < `bottom`; a ship's top is above the water).
    pub top_m: f64,
    pub bottom_m: f64,
}

/// Mass of a hull without a displacement in the database, t: 0.0025·L³ (generic; within about
/// 30 % of frigates, cruisers and carriers).
#[must_use]
pub fn default_mass_t(length_m: f64) -> f64 {
    (0.0025 * length_m * length_m * length_m).max(1.0)
}

/// The hull of a ship or submarine; `None` for aircraft and land units.
pub(crate) fn hull(
    unit: &Unit,
    pos: &Position,
    kin: &Kinematics,
    sig: Option<&Signature>,
) -> Option<Hull> {
    if !matches!(unit.domain, Domain::Surface | Domain::Subsurface) {
        return None;
    }
    let length_m = sig
        .and_then(|s| s.length_m)
        .filter(|l| *l > 0.0)
        .unwrap_or_else(|| crate::optical::default_length_m(unit.domain));
    let (top_m, bottom_m) = if unit.domain == Domain::Subsurface {
        let depth = (-pos.altitude_m).max(0.0);
        (depth - SUB_ABOVE_M, depth + SUB_BELOW_M)
    } else {
        (-SHIP_ABOVE_M, unit.draft_m.unwrap_or(DEFAULT_DRAFT_M))
    };
    Some(Hull {
        id: unit.id,
        side: unit.side,
        domain: unit.domain,
        point: pos.point,
        heading_deg: kin.heading_deg,
        speed_mps: kin.speed_mps,
        length_m,
        mass_t: unit
            .displacement_t
            .filter(|m| *m > 0.0)
            .unwrap_or_else(|| default_mass_t(length_m)),
        top_m,
        bottom_m,
    })
}

/// East and north offset of `b` from `a`, m (local flat frame).
fn enu(a: GeoPoint, b: GeoPoint) -> (f64, f64) {
    let lat0 = ((a.lat_deg + b.lat_deg) / 2.0).to_radians();
    let mut dlon = b.lon_deg - a.lon_deg;
    if dlon > 180.0 {
        dlon -= 360.0;
    } else if dlon < -180.0 {
        dlon += 360.0;
    }
    (
        dlon.to_radians() * EARTH_R_M * libm::cos(lat0),
        (b.lat_deg - a.lat_deg).to_radians() * EARTH_R_M,
    )
}

fn velocity(heading_deg: f64, speed_mps: f64) -> (f64, f64) {
    let h = heading_deg.to_radians();
    (speed_mps * libm::sin(h), speed_mps * libm::cos(h))
}

fn distance_m(a: GeoPoint, b: GeoPoint) -> f64 {
    let (e, n) = enu(a, b);
    libm::hypot(e, n)
}

/// Angle from `from` to `to`, degrees in (−180, 180].
fn angle_diff(from: f64, to: f64) -> f64 {
    let mut d = (to - from) % 360.0;
    if d > 180.0 {
        d -= 360.0;
    } else if d <= -180.0 {
        d += 360.0;
    }
    d
}

/// Bearing of `p` from `h`, relative to `h`'s heading, degrees in (−180, 180] (+ starboard).
fn relative_bearing(h: &Hull, p: GeoPoint) -> f64 {
    let (e, n) = enu(h.point, p);
    angle_diff(h.heading_deg, libm::atan2(e, n).to_degrees())
}

/// Whether two hulls' depth bands overlap.
pub(crate) fn vertical_overlap(a: &Hull, b: &Hull) -> bool {
    a.top_m < b.bottom_m && b.top_m < a.bottom_m
}

/// Distance at which two hulls touch, m: half their combined lengths.
pub(crate) fn contact_m(a: &Hull, b: &Hull) -> f64 {
    (a.length_m + b.length_m) / 2.0
}

/// Safe passing distance, m.
fn passing_m(a: &Hull, b: &Hull) -> f64 {
    MIN_PASSING_M.max(2.0 * (a.length_m + b.length_m))
}

/// Time to and distance at the closest point of approach of `b` to `a` steering `course_deg` at
/// `speed_mps`; `None` if they are not closing.
fn cpa(a: &Hull, course_deg: f64, speed_mps: f64, b: &Hull) -> Option<(f64, f64)> {
    let (rx, ry) = enu(a.point, b.point);
    let (ax, ay) = velocity(course_deg, speed_mps);
    let (bx, by) = velocity(b.heading_deg, b.speed_mps);
    let (vx, vy) = (bx - ax, by - ay);
    let vv = vx * vx + vy * vy;
    if vv < 1e-4 {
        return None;
    }
    let t = -(rx * vx + ry * vy) / vv;
    if t <= 0.0 {
        return None;
    }
    Some((t, libm::hypot(rx + vx * t, ry + vy * t)))
}

/// Whether `a` knows where `b` is close by: a ship sees ships, and its own side's submarines
/// (water-space management), but not another side's submerged boat; a submarine hears all.
fn known(a: &Hull, b: &Hull) -> bool {
    a.domain == Domain::Subsurface || b.domain == Domain::Surface || a.side == b.side
}

/// Whether the rules of the road make `a` give way to `b` (rules 13–15, simplified): to a
/// stopped hull; when overtaking; head-on (both); crossing with `b` on the starboard side.
pub(crate) fn gives_way(a: &Hull, b: &Hull) -> bool {
    if b.speed_mps < STOPPED_MPS {
        return true;
    }
    let of_a = relative_bearing(b, a.point); // where `a` is, seen from `b`
    let of_b = relative_bearing(a, b.point); // where `b` is, seen from `a`
    if of_a.abs() > 112.5 && a.speed_mps > b.speed_mps {
        return true; // `a` overtakes
    }
    if of_b.abs() > 112.5 && b.speed_mps > a.speed_mps {
        return false; // `b` overtakes
    }
    let reciprocal = angle_diff(a.heading_deg, b.heading_deg).abs() > 170.0;
    if reciprocal && of_b.abs() < 10.0 {
        return true; // head-on: both turn to starboard
    }
    of_b > 0.0
}

/// A hull's avoiding action this tick.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Steer {
    pub heading_deg: f64,
    pub slow: bool,
    /// The hull it is giving way to (the soonest).
    pub other: UnitId,
    pub turn_deg: f64,
}

/// The action `a` takes to pass `others` safely while making for `goal_deg`, or `None` if its
/// course is clear or it stands on. `giving_way_to` is the hull it was already giving way to:
/// it keeps doing so, whatever the rules say now, until the passage is safe with a margin
/// ([`HOLD_MARGIN`]), so it does not start and stop as the geometry wavers at a boundary.
pub(crate) fn avoid(
    a: &Hull,
    goal_deg: f64,
    others: &[Hull],
    giving_way_to: Option<UnitId>,
) -> Option<Steer> {
    if a.speed_mps < 0.1 {
        return None;
    }
    let near: Vec<&Hull> = others
        .iter()
        .filter(|b| b.id != a.id && vertical_overlap(a, b) && known(a, b))
        .collect();
    let held = |b: &Hull| giving_way_to == Some(b.id);
    let threat = |course: f64, b: &Hull| {
        let safe = passing_m(a, b) * if held(b) { HOLD_MARGIN } else { 1.0 };
        cpa(a, course, a.speed_mps, b).filter(|&(t, d)| t <= LOOKAHEAD_S && d < safe)
    };
    let first = near
        .iter()
        .filter_map(|b| {
            let (t, _) = threat(goal_deg, b)?;
            (held(b) || t <= CLOSE_QUARTERS_S || gives_way(a, b)).then_some((t, b.id))
        })
        .min_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)))?;
    let mut turn = TURN_STEP_DEG;
    while turn <= TURN_MAX_DEG {
        let h = sim_geo::normalize_azimuth_deg(goal_deg + turn);
        if near.iter().all(|b| threat(h, b).is_none()) {
            return Some(Steer {
                heading_deg: h,
                slow: false,
                other: first.1,
                turn_deg: turn,
            });
        }
        turn += TURN_STEP_DEG;
    }
    Some(Steer {
        heading_deg: sim_geo::normalize_azimuth_deg(goal_deg + TURN_MAX_DEG),
        slow: true,
        other: first.1,
        turn_deg: TURN_MAX_DEG,
    })
}

/// Pairs of hulls that came into contact this tick (not touching at its start), in id order,
/// with their closing speed, m/s.
pub(crate) fn new_contacts(before: &[Hull], after: &[Hull]) -> Vec<(UnitId, UnitId, f64)> {
    let was: BTreeMap<UnitId, &Hull> = before.iter().map(|h| (h.id, h)).collect();
    let mut out = Vec::new();
    for (i, a) in after.iter().enumerate() {
        for b in &after[i + 1..] {
            if !vertical_overlap(a, b) || distance_m(a.point, b.point) >= contact_m(a, b) {
                continue;
            }
            let (Some(pa), Some(pb)) = (was.get(&a.id), was.get(&b.id)) else {
                continue; // just spawned: already where it is
            };
            if distance_m(pa.point, pb.point) < contact_m(a, b) {
                continue; // touching already
            }
            let (ax, ay) = velocity(pa.heading_deg, pa.speed_mps);
            let (bx, by) = velocity(pb.heading_deg, pb.speed_mps);
            let (lo, hi) = if a.id < b.id {
                (a.id, b.id)
            } else {
                (b.id, a.id)
            };
            out.push((lo, hi, libm::hypot(ax - bx, ay - by)));
        }
    }
    out.sort_by_key(|x| (x.0, x.1));
    out
}

/// Share of its damage points a hull of mass `m_self` takes when struck by `m_other` at
/// `closing_mps`: (m_other / (m_self + m_other))² · (v / V_CRIT)². It is the hull's share of the
/// collision energy (by the other's mass) over its own kinetic energy at V_CRIT.
#[must_use]
pub fn damage_fraction(m_self: f64, m_other: f64, closing_mps: f64) -> f64 {
    let share = m_other / (m_self + m_other);
    let v = closing_mps / V_CRIT_MPS;
    share * share * v * v
}

/// A hull's avoidance in progress.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AvoidState {
    /// The hull it is giving way to; `None` once clear, while it comes back to its course.
    pub other: Option<UnitId>,
    /// For a hull without a route, the heading it was ordered to hold, to resume once clear.
    pub resume_deg: Option<f64>,
}

/// Hulls giving way, or coming back to their course after giving way.
#[derive(Debug, Clone, Default, PartialEq, Resource, Serialize, Deserialize)]
pub struct Avoidance(pub BTreeMap<UnitId, AvoidState>);

/// Within this many degrees of its course, a hull coming back from giving way is there.
pub const RESUMED_DEG: f64 = 0.5;

/// Angle from `from` to `to`, degrees in (−180, 180] (for movement).
#[must_use]
pub(crate) fn heading_gap(from: f64, to: f64) -> f64 {
    angle_diff(from, to)
}

/// Contacts found by movement this tick, resolved by [`collision_system`] in the same tick.
#[derive(Debug, Clone, Default, Resource)]
pub(crate) struct PendingCollisions(pub Vec<(UnitId, UnitId, f64)>);

/// Resolves this tick's collisions in id order: both hulls stop, the event is logged, and each
/// takes its damage roll.
pub(crate) fn collision_system(world: &mut World) {
    let pending = std::mem::take(&mut world.resource_mut::<PendingCollisions>().0);
    if pending.is_empty() {
        return;
    }
    let tick = world.resource::<crate::time::SimClock>().tick;
    for (a, b, closing) in pending {
        let mass_and_dp = |world: &mut World, id: UnitId| {
            let e = *world.resource::<crate::sim::UnitIndex>().0.get(&id)?;
            let unit = world.get::<Unit>(e)?;
            let length = world
                .get::<Signature>(e)
                .and_then(|s| s.length_m)
                .filter(|l| *l > 0.0)
                .unwrap_or_else(|| crate::optical::default_length_m(unit.domain));
            let mass = unit
                .displacement_t
                .filter(|m| *m > 0.0)
                .unwrap_or_else(|| default_mass_t(length));
            let dp = world
                .get::<crate::combat::Health>(e)
                .map_or(0.0, |h| h.profile.damage_points);
            Some((e, mass, dp))
        };
        let (Some((ea, ma, dpa)), Some((eb, mb, dpb))) =
            (mass_and_dp(world, a), mass_and_dp(world, b))
        else {
            continue; // one was destroyed by an earlier collision this tick
        };
        let (fa, fb) = (
            damage_fraction(ma, mb, closing),
            damage_fraction(mb, ma, closing),
        );
        for e in [ea, eb] {
            if let Some(mut k) = world.get_mut::<Kinematics>(e) {
                k.speed_mps = 0.0;
                k.ordered_speed_mps = 0.0;
            }
        }
        world.resource_mut::<crate::events::EventLog>().push(
            tick,
            crate::events::SimEventKind::Collision {
                a,
                b,
                closing_mps: closing,
                damage_fraction_a: fa,
                damage_fraction_b: fb,
            },
        );
        for (id, f, dp) in [(a, fa, dpa), (b, fb, dpb)] {
            if f >= MIN_DAMAGE_FRACTION && dp > 0.0 {
                crate::combat::apply_damage(world, id, f * dp, f >= HOLING_FRACTION, tick);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(id: u64, lat: f64, lon: f64, heading: f64, speed: f64) -> Hull {
        Hull {
            id: UnitId(id),
            side: SideId(1),
            domain: Domain::Surface,
            point: GeoPoint::new(lat, lon),
            heading_deg: heading,
            speed_mps: speed,
            length_m: 130.0,
            mass_t: 4000.0,
            top_m: -SHIP_ABOVE_M,
            bottom_m: DEFAULT_DRAFT_M,
        }
    }

    /// USS Fitzgerald (8,300 t) and ACX Crystal (29,000 t), about 13 m/s closing: the destroyer
    /// takes half its damage points (a mission kill), the container ship a few per cent.
    #[test]
    fn damage_shares_follow_mass_and_speed() {
        let ddg = damage_fraction(8_300.0, 29_000.0, 13.0);
        let box_ship = damage_fraction(29_000.0, 8_300.0, 13.0);
        assert!((ddg - 0.5204).abs() < 1e-3, "{ddg}");
        assert!((box_ship - 0.0425).abs() < 1e-3, "{box_ship}");
        // Equal masses: a quarter at V_CRIT; a gentle 1 m/s bump is below the damage roll.
        assert!((damage_fraction(4000.0, 4000.0, V_CRIT_MPS) - 0.25).abs() < 1e-12);
        assert!(damage_fraction(4000.0, 4000.0, 1.0) < MIN_DAMAGE_FRACTION);
    }

    #[test]
    fn default_mass_is_near_real_ships() {
        assert!((default_mass_t(175.0) - 13_398.0).abs() < 1.0); // cruiser, 12,000 t
        assert!((default_mass_t(330.0) - 89_842.0).abs() < 1.0); // carrier, 100,000 t
    }

    #[test]
    fn rules_of_the_road() {
        // Head-on: both give way.
        let a = h(1, 0.0, 0.0, 90.0, 10.0);
        let b = h(2, 0.0, 0.05, 270.0, 10.0);
        assert!(gives_way(&a, &b) && gives_way(&b, &a));
        // Crossing: `a` heads west with `b` on its starboard bow (north-west, heading south), so
        // `a` gives way; `b` has `a` on its port bow and stands on.
        let a = h(1, 0.0, 0.0, 270.0, 10.0);
        let b = h(2, 0.03, -0.03, 180.0, 10.0);
        assert!(gives_way(&a, &b));
        assert!(!gives_way(&b, &a));
        // Overtaking: the faster hull astern gives way.
        let slow = h(1, 0.0, 0.01, 90.0, 5.0);
        let fast = h(2, 0.0, 0.0, 90.0, 12.0);
        assert!(gives_way(&fast, &slow) && !gives_way(&slow, &fast));
        // A stopped hull is always given way to.
        let stopped = h(3, 0.0, 0.02, 0.0, 0.0);
        assert!(gives_way(&slow, &stopped));
    }

    #[test]
    fn head_on_turns_to_starboard_and_open_water_does_not() {
        let a = h(1, 0.0, 0.0, 90.0, 10.0);
        let b = h(2, 0.0, 0.03, 270.0, 10.0);
        let s = avoid(&a, 90.0, &[a, b], None).unwrap();
        assert!(s.turn_deg > 0.0 && s.turn_deg <= TURN_MAX_DEG);
        assert_eq!(s.other, UnitId(2));
        assert!((s.heading_deg - (90.0 + s.turn_deg)).abs() < 1e-9);
        // Far apart, or diverging: nothing to do.
        let far = h(2, 0.0, 1.0, 270.0, 10.0);
        assert!(avoid(&a, 90.0, &[a, far], None).is_none());
        let away = h(2, 0.0, 0.03, 90.0, 12.0);
        assert!(avoid(&a, 90.0, &[a, away], None).is_none());
    }

    #[test]
    fn a_ship_does_not_see_another_sides_submarine_but_its_own() {
        let ship = h(1, 0.0, 0.0, 90.0, 10.0);
        let mut boat = h(2, 0.0, 0.02, 0.0, 0.0);
        boat.domain = Domain::Subsurface;
        boat.top_m = 2.0;
        boat.bottom_m = 18.0;
        boat.side = SideId(2);
        assert!(vertical_overlap(&ship, &boat));
        assert!(avoid(&ship, 90.0, &[ship, boat], None).is_none());
        boat.side = SideId(1);
        assert!(avoid(&ship, 90.0, &[ship, boat], None).is_some());
        // Deep, it passes under.
        boat.top_m = 190.0;
        boat.bottom_m = 206.0;
        assert!(!vertical_overlap(&ship, &boat));
    }

    #[test]
    fn contact_is_counted_once_when_hulls_meet() {
        let a0 = h(1, 0.0, 0.0, 90.0, 10.0);
        let b0 = h(2, 0.0, 0.003, 270.0, 10.0); // 334 m apart
        let a1 = h(1, 0.0, 0.0012, 90.0, 10.0);
        let b1 = h(2, 0.0, 0.0018, 270.0, 10.0); // 67 m apart: inside 130 m
        let hits = new_contacts(&[a0, b0], &[a1, b1]);
        assert_eq!(hits.len(), 1);
        assert!((hits[0].2 - 20.0).abs() < 1e-9);
        // Already touching: no new collision.
        assert!(new_contacts(&[a1, b1], &[a1, b1]).is_empty());
    }
}
