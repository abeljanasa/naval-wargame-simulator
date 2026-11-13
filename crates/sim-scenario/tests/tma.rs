//! Single-observer TMA in the running engine (docs/models/tma.md): a submarine that manoeuvres
//! ranges a ship from its own passive bearings alone; one that does not, cannot.

#![allow(clippy::unwrap_used)] // test harness

use sim_core::{SaveGame, SideId, SimEventKind, Simulation, Tick, UnitId, Viewer};
use sim_geo::inverse;
use sim_scenario::Scenario;

/// A diesel submarine (side 1, silent) and a frigate about 10 km east-north-east (side 2, silent,
/// so no active sonar to hear), making 6 m/s west. `sub_route`: the submarine's waypoints.
fn scenario(sub_route: serde_json::Value, sub_speed: f64) -> Scenario {
    Scenario::from_json(
        &serde_json::json!({
            "format": "nws-scenario", "format_version": 4, "name": "tma", "seed": 11,
            "duration_s": 1800,
            "environment": {"sea_state": 2, "start_unix_s": 1794301200},
            "sides": [
                {"id": 1, "name": "Blue", "doctrine": {"emcon": "silent", "wcs": "hold"}},
                {"id": 2, "name": "Red", "doctrine": {"emcon": "silent", "wcs": "hold"}}
            ],
            "units": [
                {"id": 1, "name": "Boat", "side": 1, "platform": "GEN-SSK",
                 "position": {"lat_deg": 0.0, "lon_deg": -170.0}, "altitude_m": -60.0,
                 "heading_deg": 0.0, "speed_mps": sub_speed, "route": sub_route},
                {"id": 2, "name": "Frigate", "side": 2, "platform": "GEN-FFG",
                 "position": {"lat_deg": 0.0308, "lon_deg": -169.9155},
                 "heading_deg": 270.0, "speed_mps": 6.0,
                 "route": [{"lat_deg": 0.0308, "lon_deg": -170.5}]}
            ]
        })
        .to_string(),
    )
    .unwrap()
}

/// North 6 minutes at 3 m/s, then east: one turn makes the range observable.
fn dogleg() -> Scenario {
    scenario(
        serde_json::json!([
            {"lat_deg": 0.0097, "lon_deg": -170.0},
            {"lat_deg": 0.0097, "lon_deg": -169.95}
        ]),
        3.0,
    )
}

fn build(s: &Scenario) -> Simulation {
    let db = sim_db::embedded::open_platform_db().unwrap();
    s.build(None, &db).unwrap()
}

fn tma_solutions(sim: &Simulation) -> Vec<(u64, f64, f64)> {
    sim.events()
        .iter()
        .filter_map(|e| match e.kind {
            SimEventKind::TmaSolution {
                side: SideId(1),
                target: UnitId(2),
                observer: UnitId(1),
                range_m,
                error_m,
                ..
            } => Some((e.tick.0, range_m, error_m)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_manoeuvring_submarine_ranges_a_ship_from_its_own_bearings() {
    let mut sim = build(&dogleg());
    sim.run_until(Tick(1200));
    let sols = tma_solutions(&sim);
    assert_eq!(
        sols.len(),
        1,
        "one solution reported per bearing history: {sols:?}"
    );
    let (tick, range_m, error_m) = sols[0];
    // The reported range agrees with the true range at that moment, within 3σ of its error.
    let true_range = {
        let mut d = build(&dogleg());
        d.run_until(Tick(tick));
        let w = d.snapshot(Viewer::WhiteCell);
        let p = |id| {
            w.units
                .iter()
                .find(|u| u.id == UnitId(id))
                .unwrap()
                .position
        };
        inverse(p(1), p(2)).distance_m
    };
    assert!(
        (range_m - true_range).abs() < 3.0 * error_m,
        "TMA range {range_m:.0} m, true {true_range:.0} m, error {error_m:.0} m"
    );
    // After the turn at 6 minutes, never from the straight first leg alone.
    assert!(tick > 360, "solved at {tick} s");
    assert!(
        range_m > 3000.0 && range_m < 15000.0,
        "range {range_m:.0} m"
    );
    assert!(error_m < 0.3 * range_m, "error {error_m:.0} m");
    // The side now holds a contact on the frigate, near the truth.
    let truth = sim
        .snapshot(Viewer::WhiteCell)
        .units
        .iter()
        .find(|u| u.id == UnitId(2))
        .unwrap()
        .position;
    let held = sim.snapshot(Viewer::Side { side: SideId(1) });
    let c = held
        .contacts
        .iter()
        .find(|c| c.detected_by == UnitId(1))
        .expect("a contact from TMA");
    let miss = inverse(c.position, truth).distance_m;
    assert!(
        miss < 3.0 * c.error.semi_major_m.max(300.0),
        "contact {miss:.0} m from the truth, error ellipse {:.0} m",
        c.error.semi_major_m
    );
}

#[test]
fn a_stopped_submarine_cannot_range_a_ship() {
    // No own motion: the bearings never fix the range, so no TMA contact forms.
    let mut sim = build(&scenario(serde_json::json!([]), 0.0));
    sim.run_until(Tick(1200));
    assert!(tma_solutions(&sim).is_empty());
    assert!(sim
        .snapshot(Viewer::Side { side: SideId(1) })
        .contacts
        .is_empty());
}

#[test]
fn tma_histories_survive_a_save_and_run_on_identically() {
    let s = dogleg();
    let mut a = build(&s);
    a.run_until(Tick(300));
    let bytes = a.save().to_bytes().unwrap();
    let mut b = Simulation::from_save(SaveGame::from_bytes(&bytes).unwrap()).unwrap();
    b.set_land_mask(sim_geo::land::embedded());
    a.run_until(Tick(1200));
    b.run_until(Tick(1200));
    assert_eq!(a.state_hash(), b.state_hash());
}
