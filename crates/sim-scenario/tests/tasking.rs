//! Automatic sense-zone tasking in the running engine (docs/models/sensor-tasking.md): a sense
//! zone short of its coverage goal gets the side's free sensors, best first; busy units are left
//! alone; a released unit is replaced; parked aircraft are launched; every decision is logged.

#![allow(clippy::unwrap_used)] // test harness

use sim_core::{Command, CommandPayload, GeoPoint, Issuer, SimEventKind, Simulation, Tick, UnitId};
use sim_scenario::Scenario;

const CENTRE: (f64, f64) = (0.0, -170.0);

fn ship(id: u64, lon: f64, mission: Option<serde_json::Value>) -> serde_json::Value {
    let mut u = serde_json::json!({"id": id, "name": format!("Picket {id}"), "side": 1,
        "platform": "GEN-FFG", "position": {"lat_deg": CENTRE.0, "lon_deg": lon},
        "speed_mps": 0.0});
    if let Some(m) = mission {
        u["mission"] = m;
    }
    u
}

/// Blue's 25 km radar sense zone 1 round `CENTRE`, with `goal` and `assets`, plus `units`.
fn scenario(
    goal: f64,
    assets: &[u64],
    units: Vec<serde_json::Value>,
    extra: serde_json::Value,
) -> Scenario {
    let mut s = serde_json::json!({
        "format": "nws-scenario", "format_version": 6, "name": "tasking", "seed": 8,
        "duration_s": 20000,
        "environment": {"sea_state": 2, "start_unix_s": 1794301200},
        "sides": [{"id": 1, "name": "Blue", "doctrine": {"emcon": "active", "wcs": "hold"}}],
        "zones": [{"id": 1, "name": "Outer screen", "side": 1, "kind": "sense",
            "area": {"shape": "circle", "centre": {"lat_deg": CENTRE.0, "lon_deg": CENTRE.1},
                     "radius_m": 25000},
            "sensors": ["radar"], "coverage_goal": goal, "assets": assets}],
        "units": units
    });
    for (k, v) in extra.as_object().unwrap() {
        s[k] = v.clone();
    }
    Scenario::from_json(&s.to_string()).unwrap()
}

fn build(s: &Scenario) -> Simulation {
    s.build(None, &sim_db::embedded::open_platform_db().unwrap())
        .unwrap()
}

fn tasked(sim: &Simulation) -> Vec<(u64, UnitId, f64, f64)> {
    sim.events()
        .iter()
        .filter_map(|e| match &e.kind {
            SimEventKind::SensorTasked {
                unit,
                coverage,
                projected,
                ..
            } => Some((e.tick.0, *unit, *coverage, *projected)),
            _ => None,
        })
        .collect()
}

fn count(sim: &Simulation, f: impl Fn(&SimEventKind) -> bool) -> usize {
    sim.events().iter().filter(|e| f(&e.kind)).count()
}

fn patrol_elsewhere() -> serde_json::Value {
    serde_json::json!({"kind": "patrol", "centre": {"lat_deg": 2.0, "lon_deg": -170.0},
        "radius_m": 10000})
}

#[test]
fn a_zone_short_of_its_goal_gets_a_free_ship_which_covers_it() {
    let s = scenario(
        0.6,
        &[],
        vec![
            ship(1, -170.5, None),
            ship(2, -170.4, Some(patrol_elsewhere())),
        ],
        serde_json::json!({}),
    );
    let mut sim = build(&s);
    sim.run_until(Tick(1));
    let t = tasked(&sim);
    assert_eq!(t.len(), 1, "{t:?}");
    assert_eq!(t[0].1, UnitId(1), "the busy ship is left alone");
    assert!(t[0].2 < 0.6 && t[0].3 >= 0.6, "{t:?}");
    // It goes and the zone is covered.
    sim.run_until(Tick(9_000));
    let cov = sim.zone_coverage(1).unwrap().fraction;
    assert!(cov >= 0.6, "{cov}");
    assert_eq!(tasked(&sim).len(), 1, "no more tasking once covered");
    let snap = sim.snapshot(sim_core::Viewer::WhiteCell);
    assert_eq!(snap.sensor_tasking[0].units, vec![UnitId(1)]);
}

#[test]
fn only_the_zones_assets_are_used_and_a_shortfall_is_logged_once() {
    let s = scenario(
        0.6,
        &[2],
        vec![
            ship(1, -170.5, None),
            ship(2, -170.4, Some(patrol_elsewhere())),
        ],
        serde_json::json!({}),
    );
    let mut sim = build(&s);
    sim.run_until(Tick(1_000));
    assert!(
        tasked(&sim).is_empty(),
        "ship 1 is not an asset; ship 2 is busy"
    );
    assert_eq!(
        count(&sim, |k| matches!(
            k,
            SimEventKind::CoverageShortfall { zone: 1, .. }
        )),
        1
    );
}

#[test]
fn a_released_unit_is_replaced_by_another() {
    let s = scenario(
        0.6,
        &[],
        vec![ship(1, -170.5, None), ship(3, -169.5, None)],
        serde_json::json!({}),
    );
    let mut sim = build(&s);
    sim.run_until(Tick(1));
    let first = tasked(&sim)[0].1;
    // The player takes the tasked ship for something else.
    sim.submit(Command {
        tick: Tick(100),
        issuer: Issuer::WhiteCell {
            umpire: "test".into(),
        },
        side: None,
        payload: CommandPayload::AssignMission {
            unit: first,
            mission: Some(sim_core::Mission::Ferry {
                destination: GeoPoint::new(3.0, -170.0),
            }),
        },
    })
    .unwrap();
    sim.run_until(Tick(301));
    let t = tasked(&sim);
    assert_eq!(t.len(), 2, "{t:?}");
    assert_eq!(t[1].0, 300);
    assert_ne!(t[1].1, first, "the other ship takes over");
}

#[test]
fn a_parked_patrol_aircraft_is_launched_to_watch_the_zone() {
    let s = scenario(
        0.9,
        &[],
        vec![
            serde_json::json!({"id": 7, "name": "Orion", "side": 1, "platform": "GEN-MPA-TP",
            "position": {"lat_deg": CENTRE.0, "lon_deg": -170.8}, "base": 1}),
        ],
        serde_json::json!({"facilities": [{"id": 1, "name": "Northfield", "side": 1,
            "kind": "airbase", "position": {"lat_deg": CENTRE.0, "lon_deg": -170.8},
            "radius_m": 2000, "air": {"runways": 1}}]}),
    );
    let mut sim = build(&s);
    sim.run_until(Tick(2_000));
    let t = tasked(&sim);
    assert_eq!(t.first().map(|x| x.1), Some(UnitId(7)), "{t:?}");
    let launched: Vec<String> = sim
        .events()
        .iter()
        .filter_map(|e| match &e.kind {
            SimEventKind::SortieLaunched { unit, mission, .. } if *unit == UnitId(7) => {
                Some(mission.clone())
            }
            _ => None,
        })
        .collect();
    assert_eq!(launched, vec!["patrol".to_owned()]);
}

#[test]
fn tasking_survives_a_save_and_runs_on_identically() {
    let s = scenario(
        0.6,
        &[],
        vec![ship(1, -170.5, None), ship(3, -169.5, None)],
        serde_json::json!({}),
    );
    let mut a = build(&s);
    a.run_until(Tick(450));
    let mut b = Simulation::from_save(a.save()).unwrap();
    b.set_land_mask(sim_geo::land::embedded());
    assert_eq!(a.state_hash(), b.state_hash());
    a.run_until(Tick(4_000));
    b.run_until(Tick(4_000));
    assert_eq!(a.state_hash(), b.state_hash());
    assert_eq!(tasked(&a), tasked(&b));
}
