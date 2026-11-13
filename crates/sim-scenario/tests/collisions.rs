//! Collisions and collision avoidance in the running engine (docs/models/collisions.md): ships
//! give way by the rules of the road, own side included; a hull nobody can avoid is struck, and
//! each takes damage by closing speed and the other's mass.

#![allow(clippy::unwrap_used)] // test harness

use sim_core::{SimEventKind, Simulation, Tick, UnitId, Viewer};
use sim_scenario::Scenario;

/// One unit for the scenario JSON: `route` east or west from `from`.
fn unit(
    id: u64,
    side: u32,
    platform: &str,
    from: (f64, f64),
    to: Option<(f64, f64)>,
    speed: f64,
    altitude_m: f64,
) -> serde_json::Value {
    let route: Vec<_> = to
        .into_iter()
        .map(|(lat, lon)| serde_json::json!({"lat_deg": lat, "lon_deg": lon}))
        .collect();
    serde_json::json!({
        "id": id, "name": format!("Unit {id}"), "side": side, "platform": platform,
        "position": {"lat_deg": from.0, "lon_deg": from.1}, "altitude_m": altitude_m,
        "heading_deg": 90.0, "speed_mps": speed, "route": route
    })
}

fn build(units: Vec<serde_json::Value>) -> Simulation {
    let s = Scenario::from_json(
        &serde_json::json!({
            "format": "nws-scenario", "format_version": 4, "name": "collisions", "seed": 3,
            "duration_s": 3600,
            "environment": {"sea_state": 2, "start_unix_s": 1794301200},
            "sides": [
                {"id": 1, "name": "Blue", "doctrine": {"wcs": "hold", "emcon": "silent"}},
                {"id": 2, "name": "Red", "doctrine": {"wcs": "hold", "emcon": "silent"}}
            ],
            "units": units
        })
        .to_string(),
    )
    .unwrap();
    s.build(None, &sim_db::embedded::open_platform_db().unwrap())
        .unwrap()
}

/// Runs to `ticks`, returning the least distance between units 1 and 2 seen at any tick.
fn run_closest(sim: &mut Simulation, ticks: u64) -> f64 {
    let mut least = f64::INFINITY;
    for t in 1..=ticks {
        sim.run_until(Tick(t));
        let units = sim.snapshot(Viewer::WhiteCell).units;
        let at = |id| {
            units
                .iter()
                .find(|u| u.id == UnitId(id))
                .map(|u| u.position)
        };
        if let (Some(a), Some(b)) = (at(1), at(2)) {
            least = least.min(sim_geo::inverse(a, b).distance_m);
        }
    }
    least
}

fn collisions(sim: &Simulation) -> Vec<(UnitId, UnitId, f64, f64, f64)> {
    sim.events()
        .iter()
        .filter_map(|e| match e.kind {
            SimEventKind::Collision {
                a,
                b,
                closing_mps,
                damage_fraction_a,
                damage_fraction_b,
            } => Some((a, b, closing_mps, damage_fraction_a, damage_fraction_b)),
            _ => None,
        })
        .collect()
}

fn avoiders(sim: &Simulation) -> Vec<UnitId> {
    sim.events()
        .iter()
        .filter_map(|e| match e.kind {
            SimEventKind::CollisionAvoidance { unit, .. } => Some(unit),
            _ => None,
        })
        .collect()
}

#[test]
fn own_side_ships_meeting_head_on_both_turn_to_starboard_and_pass() {
    let mut sim = build(vec![
        unit(
            1,
            1,
            "GEN-FFG",
            (0.0, -170.05),
            Some((0.0, -169.90)),
            10.0,
            0.0,
        ),
        unit(
            2,
            1,
            "GEN-FFG",
            (0.0, -169.95),
            Some((0.0, -170.10)),
            10.0,
            0.0,
        ),
    ]);
    let least = run_closest(&mut sim, 1200);
    assert!(collisions(&sim).is_empty());
    let who = avoiders(&sim);
    assert!(
        who.contains(&UnitId(1)) && who.contains(&UnitId(2)),
        "{who:?}"
    );
    assert!(least > 300.0, "passed {least:.0} m apart");
}

#[test]
fn crossing_the_ship_with_the_other_on_its_starboard_side_gives_way() {
    // 1 heads west; 2 comes south from its starboard bow on a collision course (both reach the
    // crossing point after 780 s).
    let mut sim = build(vec![
        unit(
            1,
            1,
            "GEN-FFG",
            (0.0, -169.95),
            Some((0.0, -170.20)),
            10.0,
            0.0,
        ),
        unit(
            2,
            2,
            "GEN-OPV",
            (0.07, -170.02),
            Some((-0.2, -170.02)),
            10.0,
            0.0,
        ),
    ]);
    let least = run_closest(&mut sim, 1400);
    assert!(collisions(&sim).is_empty());
    assert_eq!(
        avoiders(&sim).first(),
        Some(&UnitId(1)),
        "1 gives way first"
    );
    assert!(!avoiders(&sim).contains(&UnitId(2)), "2 stands on");
    assert!(least > 300.0, "passed {least:.0} m apart");
}

#[test]
fn a_ship_runs_down_a_hovering_boat_it_cannot_see_and_both_are_damaged() {
    let mut sim = build(vec![
        unit(
            1,
            1,
            "GEN-FFG",
            (0.0, -170.05),
            Some((0.0, -169.90)),
            10.0,
            0.0,
        ),
        unit(2, 2, "GEN-SSK", (0.0, -170.0), None, 0.0, -12.0),
    ]);
    sim.run_until(Tick(900));
    let hits = collisions(&sim);
    assert_eq!(hits.len(), 1, "{hits:?}");
    let (a, b, closing, fa, fb) = hits[0];
    assert_eq!((a, b), (UnitId(1), UnitId(2)));
    assert!((closing - 10.0).abs() < 0.5, "{closing}");
    // 5,000 t frigate, 2,500 t boat: the boat takes (2/3)² / (1/3)² = 4× the frigate's share.
    let expect = |m_self: f64, m_other: f64| sim_core::damage_fraction(m_self, m_other, closing);
    assert!((fa - expect(5000.0, 2500.0)).abs() < 1e-9);
    assert!((fb - expect(2500.0, 5000.0)).abs() < 1e-9);
    assert!(fb > 3.9 * fa);
    let damaged: Vec<UnitId> = sim
        .events()
        .iter()
        .filter_map(|e| match e.kind {
            SimEventKind::DamageTaken { unit, .. } => Some(unit),
            _ => None,
        })
        .collect();
    assert!(damaged.contains(&UnitId(2)), "{damaged:?}");
    // Both stopped where they met.
    let units = sim.snapshot(Viewer::WhiteCell).units;
    assert!(units
        .iter()
        .all(|u| u.speed_mps == 0.0 && u.ordered_speed_mps == 0.0));
}

#[test]
fn a_deep_boat_passes_under_and_an_own_side_boat_is_avoided() {
    let ship = unit(
        1,
        1,
        "GEN-FFG",
        (0.0, -170.05),
        Some((0.0, -169.90)),
        10.0,
        0.0,
    );
    let mut deep = build(vec![
        ship.clone(),
        unit(2, 2, "GEN-SSK", (0.0, -170.0), None, 0.0, -200.0),
    ]);
    deep.run_until(Tick(900));
    assert!(collisions(&deep).is_empty());
    assert!(avoiders(&deep).is_empty());

    let mut own = build(vec![
        ship,
        unit(2, 1, "GEN-SSK", (0.0, -170.0), None, 0.0, -12.0),
    ]);
    let least = run_closest(&mut own, 900);
    assert!(collisions(&own).is_empty());
    assert_eq!(avoiders(&own), vec![UnitId(1)]);
    assert!(least > 200.0, "passed {least:.0} m off");
}

#[test]
fn avoidance_survives_a_save_and_runs_on_identically() {
    let units = vec![
        unit(
            1,
            1,
            "GEN-FFG",
            (0.0, -170.05),
            Some((0.0, -169.90)),
            10.0,
            0.0,
        ),
        unit(
            2,
            1,
            "GEN-FFG",
            (0.0, -169.95),
            Some((0.0, -170.10)),
            10.0,
            0.0,
        ),
    ];
    let mut a = build(units);
    // Mid-manoeuvre: both are giving way.
    let first = (1..=1200)
        .find(|&t| {
            a.run_until(Tick(t));
            avoiders(&a).len() >= 2
        })
        .unwrap();
    a.run_until(Tick(first + 5));
    let mut b = Simulation::from_save(a.save()).unwrap();
    b.set_land_mask(sim_geo::land::embedded());
    assert_eq!(a.state_hash(), b.state_hash());
    a.run_until(Tick(1200));
    b.run_until(Tick(1200));
    assert_eq!(a.state_hash(), b.state_hash());
}

#[test]
fn a_ship_holding_a_heading_gives_way_then_resumes_it() {
    // No routes: each holds the heading it was given, head-on.
    let mut a = unit(1, 1, "GEN-FFG", (0.0, -170.05), None, 10.0, 0.0);
    let mut b = unit(2, 1, "GEN-FFG", (0.0, -169.95), None, 10.0, 0.0);
    a["heading_deg"] = serde_json::json!(90.0);
    b["heading_deg"] = serde_json::json!(270.0);
    let mut sim = build(vec![a, b]);
    let least = run_closest(&mut sim, 1500);
    assert!(collisions(&sim).is_empty());
    assert!(avoiders(&sim).len() >= 2);
    assert!(least > 300.0, "passed {least:.0} m apart");
    let units = sim.snapshot(Viewer::WhiteCell).units;
    let heading = |id| {
        units
            .iter()
            .find(|u| u.id == UnitId(id))
            .unwrap()
            .heading_deg
    };
    assert!((heading(1) - 90.0).abs() < 1.0, "{}", heading(1));
    assert!((heading(2) - 270.0).abs() < 1.0, "{}", heading(2));
}
