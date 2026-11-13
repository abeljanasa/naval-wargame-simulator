//! Air operations in the running engine (docs/models/air-operations.md): aircraft parked at
//! airbases at a readiness, sorties launched through runway slots with a loadout for the mission,
//! recovery, turnaround, bingo fuel, tanking, and carriers.

#![allow(clippy::unwrap_used)] // test harness

use sim_core::{
    Command, CommandPayload, GeoPoint, Issuer, Readiness, SideId, SimEventKind, Simulation, Tick,
    UnitId, Viewer,
};
use sim_scenario::Scenario;
use std::collections::BTreeMap;

const BASE: (f64, f64) = (0.0, -170.0);

/// Blue's airbase 1 at `BASE` (one runway, room for four, 50 t of aviation fuel), plus `units`.
fn scenario(units: Vec<serde_json::Value>, fuel: bool) -> Scenario {
    Scenario::from_json(
        &serde_json::json!({
            "format": "nws-scenario", "format_version": 6, "name": "air", "seed": 4,
            "duration_s": 20000, "fuel": fuel,
            "environment": {"sea_state": 2, "start_unix_s": 1794301200},
            "sides": [
                {"id": 1, "name": "Blue", "doctrine": {"wcs": "hold", "emcon": "silent"}},
                {"id": 2, "name": "Red", "doctrine": {"wcs": "hold", "emcon": "silent"}}
            ],
            "facilities": [{
                "id": 1, "name": "Northfield", "side": 1, "kind": "airbase",
                "position": {"lat_deg": BASE.0, "lon_deg": BASE.1}, "radius_m": 2000,
                "air": {"runways": 1, "parking": 4, "aviation_fuel_t": 50}
            }],
            "units": units
        })
        .to_string(),
    )
    .unwrap()
}

fn fighter(id: u64, extra: serde_json::Value) -> serde_json::Value {
    let mut u = serde_json::json!({
        "id": id, "name": format!("Viper {id}"), "side": 1, "platform": "GEN-FTR-MR",
        "position": {"lat_deg": BASE.0, "lon_deg": BASE.1}, "base": 1,
        "loadout": "GEN-FTR-MR/CAP"
    });
    for (k, v) in extra.as_object().unwrap() {
        u[k] = v.clone();
    }
    u
}

fn build(s: &Scenario) -> Simulation {
    s.build(None, &sim_db::embedded::open_platform_db().unwrap())
        .unwrap()
}

fn order(sim: &mut Simulation, tick: u64, payload: CommandPayload) {
    sim.submit(Command {
        tick: Tick(tick),
        issuer: Issuer::WhiteCell {
            umpire: "test".into(),
        },
        side: None,
        payload,
    })
    .unwrap();
}

fn patrol(lat: f64, lon: f64) -> sim_core::Mission {
    sim_core::Mission::Patrol {
        centre: GeoPoint::new(lat, lon),
        radius_m: 20_000.0,
    }
}

/// (tick, unit, loadout) of each launch.
fn launches(sim: &Simulation) -> Vec<(u64, UnitId, Option<String>)> {
    sim.events()
        .iter()
        .filter_map(|e| match &e.kind {
            SimEventKind::SortieLaunched { unit, loadout, .. } => {
                Some((e.tick.0, *unit, loadout.clone()))
            }
            _ => None,
        })
        .collect()
}

fn count(sim: &Simulation, f: impl Fn(&SimEventKind) -> bool) -> usize {
    sim.events().iter().filter(|e| f(&e.kind)).count()
}

#[test]
fn parked_aircraft_are_off_the_map_and_listed_at_their_base() {
    let s = scenario(
        vec![
            fighter(1, serde_json::json!({})),
            fighter(2, serde_json::json!({"readiness": "alert15"})),
        ],
        false,
    );
    let mut sim = build(&s);
    sim.run_until(Tick(5));
    let snap = sim.snapshot(Viewer::Side { side: SideId(1) });
    assert!(snap.units.is_empty(), "parked aircraft are not on the map");
    let base = &snap.air_bases[0];
    assert_eq!(base.name, "Northfield");
    let ready: BTreeMap<_, _> = base
        .aircraft
        .iter()
        .map(|a| (a.unit, a.readiness))
        .collect();
    assert_eq!(ready[&UnitId(1)], Readiness::Ready);
    assert_eq!(ready[&UnitId(2)], Readiness::Alert15);
    assert!(base.aircraft.iter().all(|a| a.loadouts.len() == 3));
    // The other side does not see Blue's base.
    assert!(sim
        .snapshot(Viewer::Side { side: SideId(2) })
        .air_bases
        .is_empty());
}

#[test]
fn launches_wait_for_readiness_and_a_runway_and_take_the_missions_loadout() {
    let s = scenario(
        vec![
            fighter(1, serde_json::json!({})),
            fighter(2, serde_json::json!({})),
            fighter(3, serde_json::json!({"readiness": "alert15"})),
        ],
        false,
    );
    let mut sim = build(&s);
    for u in 1..=3 {
        order(
            &mut sim,
            10,
            CommandPayload::LaunchSortie {
                unit: UnitId(u),
                mission: patrol(0.3, -170.0),
                loadout: None,
            },
        );
    }
    sim.run_until(Tick(1_200));
    let l = launches(&sim);
    assert_eq!(l.len(), 3, "{l:?}");
    // One runway, two minutes a launch: 1 at once, 2 two minutes later, 3 after its alert.
    assert_eq!((l[0].0, l[0].1), (10, UnitId(1)));
    assert_eq!((l[1].0, l[1].1), (130, UnitId(2)));
    assert_eq!((l[2].0, l[2].1), (910, UnitId(3)));
    assert!(l.iter().all(|x| x.2.as_deref() == Some("GEN-FTR-MR/CAP")));
    // In the air: on the map, at cruise altitude, with the CAP weapons.
    let snap = sim.snapshot(Viewer::Side { side: SideId(1) });
    let v = snap.units.iter().find(|u| u.id == UnitId(1)).unwrap();
    assert!((v.altitude_m - 8_000.0).abs() < 1e-9);
    assert_eq!(v.air.as_ref().unwrap().state, "airborne");
    assert!(v.stores.iter().any(|s| s.weapon == "WPN-AAM-BVR"));
}

#[test]
fn a_strike_takes_the_anti_ship_loadout_and_a_change_costs_rearming_time() {
    let s = scenario(vec![fighter(1, serde_json::json!({}))], false);
    let mut sim = build(&s);
    order(
        &mut sim,
        10,
        CommandPayload::LaunchSortie {
            unit: UnitId(1),
            mission: sim_core::Mission::Strike { contact: 1 },
            loadout: None,
        },
    );
    sim.run_until(Tick(2_000));
    let ordered = sim
        .events()
        .iter()
        .find_map(|e| match &e.kind {
            SimEventKind::SortieOrdered {
                ready_in_s,
                loadout,
                ..
            } => Some((*ready_in_s, loadout.clone())),
            _ => None,
        })
        .unwrap();
    // From the CAP loadout it carries to anti-ship: two minutes a weapon loaded.
    assert_eq!(ordered.1.as_deref(), Some("GEN-FTR-MR/ASUW"));
    let weapons: u32 = sim_db::embedded::open_platform_db()
        .unwrap()
        .loadouts_of("GEN-FTR-MR")
        .unwrap()
        .into_iter()
        .find(|l| l.0 == "GEN-FTR-MR/ASUW")
        .unwrap()
        .3
        .iter()
        .map(|w| w.1)
        .sum();
    assert!(
        (ordered.0 - f64::from(weapons) * 120.0).abs() < 1e-9,
        "{ordered:?}"
    );
    // It has no target contact, so the sortie is over as soon as it is up.
    assert!(
        count(
            &sim,
            |k| matches!(k, SimEventKind::AircraftReturning { reason, .. } if reason == "target gone")
        ) == 1
    );
}

#[test]
fn a_ferry_completes_returns_lands_and_turns_round() {
    let s = scenario(vec![fighter(1, serde_json::json!({}))], false);
    let mut sim = build(&s);
    order(
        &mut sim,
        10,
        CommandPayload::LaunchSortie {
            unit: UnitId(1),
            mission: sim_core::Mission::Ferry {
                destination: GeoPoint::new(0.4, -170.0),
            },
            loadout: None,
        },
    );
    sim.run_until(Tick(1_200));
    let returning = count(
        &sim,
        |k| matches!(k, SimEventKind::AircraftReturning { reason, .. } if reason == "mission complete"),
    );
    assert_eq!(returning, 1);
    let recovered: Vec<(u64, f64)> = sim
        .events()
        .iter()
        .filter_map(|e| match &e.kind {
            SimEventKind::AircraftRecovered { turnaround_s, .. } => Some((e.tick.0, *turnaround_s)),
            _ => None,
        })
        .collect();
    assert_eq!(recovered.len(), 1);
    // 20 min plus 2 min for each of the CAP loadout's six missiles.
    assert!((recovered[0].1 - 1_920.0).abs() < 1e-9);
    let snap = sim.snapshot(Viewer::WhiteCell);
    assert!(snap.units.is_empty(), "back in the hangar");
    let parked = &snap.air_bases[0].aircraft[0];
    let left = recovered[0].0 + 1_920 - 1_200;
    #[allow(clippy::cast_precision_loss)]
    let expect = left as f64;
    assert!(
        (parked.ready_in_s - expect).abs() < 1e-9,
        "{}",
        parked.ready_in_s
    );
    // Tasked again before the turnaround ends, it waits for it.
    order(
        &mut sim,
        1_201,
        CommandPayload::LaunchSortie {
            unit: UnitId(1),
            mission: patrol(0.2, -170.0),
            loadout: None,
        },
    );
    sim.run_until(Tick(recovered[0].0 + 1_920 + 5));
    assert_eq!(launches(&sim).last().unwrap().0, recovered[0].0 + 1_920);
}

#[test]
fn maintenance_holds_an_aircraft_until_it_ends() {
    let s = scenario(
        vec![fighter(
            1,
            serde_json::json!({"readiness": "maintenance", "maintenance_s": 600}),
        )],
        false,
    );
    let mut sim = build(&s);
    order(
        &mut sim,
        5,
        CommandPayload::LaunchSortie {
            unit: UnitId(1),
            mission: patrol(0.2, -170.0),
            loadout: None,
        },
    );
    sim.run_until(Tick(700));
    assert!(launches(&sim).is_empty(), "in maintenance at tick 5");
    assert!(sim.events().iter().any(|e| matches!(&e.kind,
        SimEventKind::CommandRejected { reason, .. } if reason.contains("in maintenance"))));
    assert_eq!(
        count(&sim, |k| matches!(
            k,
            SimEventKind::ReadinessChanged {
                readiness: Readiness::Ready,
                ..
            }
        )),
        1
    );
    order(
        &mut sim,
        701,
        CommandPayload::LaunchSortie {
            unit: UnitId(1),
            mission: patrol(0.2, -170.0),
            loadout: None,
        },
    );
    sim.run_until(Tick(710));
    assert_eq!(launches(&sim).len(), 1);
}

#[test]
fn at_bingo_an_aircraft_returns_and_takes_fuel_from_the_base() {
    // A quarter of a tank, 300 km out: 300/2,778 of a tank home plus the 10 % reserve is 0.21.
    let s = scenario(
        vec![fighter(
            1,
            serde_json::json!({"airborne": true, "fuel_level": 0.25,
                "position": {"lat_deg": 2.7, "lon_deg": -170.0}, "altitude_m": 8000,
                "mission": {"kind": "patrol", "centre": {"lat_deg": 2.7, "lon_deg": -170.0},
                            "radius_m": 20000}}),
        )],
        true,
    );
    let mut sim = build(&s);
    sim.run_until(Tick(3_000));
    let bingo = count(&sim, |k| {
        matches!(k, SimEventKind::BingoFuel { tanker: None, .. })
    });
    assert_eq!(bingo, 1);
    let fuel: Vec<f64> = sim
        .events()
        .iter()
        .filter_map(|e| match &e.kind {
            SimEventKind::AircraftRecovered { fuel_t, .. } => Some(*fuel_t),
            _ => None,
        })
        .collect();
    assert_eq!(fuel.len(), 1, "it made it home");
    assert!(fuel[0] > 4.0, "it took most of a 7 t tank: {fuel:?}");
    let left = sim.snapshot(Viewer::WhiteCell).air_bases[0]
        .aviation_fuel_t
        .unwrap();
    assert!((left - (50.0 - fuel[0])).abs() < 1e-9);
}

#[test]
fn an_aircraft_with_nowhere_to_land_is_lost_when_its_fuel_runs_out() {
    let s = Scenario::from_json(
        &serde_json::json!({
            "format": "nws-scenario", "format_version": 6, "name": "dry", "seed": 4,
            "duration_s": 2000, "fuel": true,
            "sides": [{"id": 1, "name": "Blue"}],
            "units": [{"id": 1, "name": "Stray", "side": 1, "platform": "GEN-FTR-MR",
                "position": {"lat_deg": 0.0, "lon_deg": -170.0}, "altitude_m": 8000,
                "heading_deg": 90.0, "fuel_level": 0.02}]
        })
        .to_string(),
    )
    .unwrap();
    let mut sim = build(&s);
    sim.run_until(Tick(1_000));
    assert_eq!(
        count(
            &sim,
            |k| matches!(k, SimEventKind::AircraftLost { reason, .. } if reason == "out of fuel")
        ),
        1
    );
    assert!(sim.snapshot(Viewer::WhiteCell).units.is_empty());
}

#[test]
fn a_fighter_at_bingo_near_a_tanker_refuels_from_it_and_carries_on() {
    let far = (0.0, -165.0);
    let station = serde_json::json!({"kind": "support",
        "station": {"lat_deg": far.0, "lon_deg": far.1}, "radius_m": 20000});
    let s = scenario(
        vec![
            serde_json::json!({"id": 5, "name": "Texaco", "side": 1, "platform": "GEN-TANKER",
                "position": {"lat_deg": far.0, "lon_deg": far.1}, "altitude_m": 9000,
                "base": 1, "airborne": true, "mission": station}),
            fighter(
                1,
                serde_json::json!({"airborne": true, "fuel_level": 0.3,
                    "position": {"lat_deg": far.0, "lon_deg": far.1 - 0.05}, "altitude_m": 8000,
                    "mission": {"kind": "patrol", "centre": {"lat_deg": far.0, "lon_deg": far.1},
                                "radius_m": 20000}}),
            ),
        ],
        true,
    );
    let mut sim = build(&s);
    sim.run_until(Tick(1_200));
    assert_eq!(
        count(&sim, |k| matches!(
            k,
            SimEventKind::BingoFuel {
                tanker: Some(UnitId(5)),
                ..
            }
        )),
        1
    );
    assert_eq!(
        count(&sim, |k| matches!(k, SimEventKind::TankingStarted { .. })),
        1
    );
    let done: Vec<f64> = sim
        .events()
        .iter()
        .filter_map(|e| match &e.kind {
            SimEventKind::TankingComplete { taken_t, .. } => Some(*taken_t),
            _ => None,
        })
        .collect();
    assert_eq!(done.len(), 1);
    assert!(done[0] > 3.0, "{done:?}");
    // Back on patrol, not home.
    let snap = sim.snapshot(Viewer::WhiteCell);
    let f = snap.units.iter().find(|u| u.id == UnitId(1)).unwrap();
    assert_eq!(f.air.as_ref().unwrap().state, "airborne");
    assert!(f.fuel_fraction.unwrap() > 0.8);
    assert!(f.mission.is_some());
}

#[test]
fn a_helicopter_deck_takes_helicopters_and_aircraft_are_lost_with_their_ship() {
    let s = Scenario::from_json(
        &serde_json::json!({
            "format": "nws-scenario", "format_version": 6, "name": "deck", "seed": 4,
            "duration_s": 2000,
            "sides": [{"id": 1, "name": "Blue"}],
            "facilities": [
                {"id": 2, "name": "Flight deck", "side": 1, "kind": "airbase", "unit": 10,
                 "position": {"lat_deg": 0.0, "lon_deg": -170.0}, "radius_m": 500,
                 "air": {"runways": 1, "parking": 2, "deck": true}}
            ],
            "units": [
                {"id": 10, "name": "Bastion", "side": 1, "platform": "GEN-FFG",
                 "position": {"lat_deg": 0.0, "lon_deg": -170.0}, "speed_mps": 0.0},
                {"id": 11, "name": "Seahawk", "side": 1, "platform": "GEN-HELO-ASW",
                 "position": {"lat_deg": 0.0, "lon_deg": -170.0}, "base": 2},
                {"id": 12, "name": "Viper", "side": 1, "platform": "GEN-FTR-MR",
                 "position": {"lat_deg": 0.1, "lon_deg": -170.0}, "altitude_m": 8000,
                 "base": 2, "airborne": true}
            ]
        })
        .to_string(),
    )
    .unwrap();
    let mut sim = build(&s);
    sim.run_until(Tick(5));
    // A jet cannot be recalled to a helicopter deck.
    order(
        &mut sim,
        6,
        CommandPayload::RecallAircraft {
            unit: UnitId(12),
            base: Some(2),
        },
    );
    sim.run_until(Tick(8));
    assert!(sim.events().iter().any(|e| matches!(&e.kind,
        SimEventKind::CommandRejected { reason, .. } if reason.contains("cannot use airbase 2"))));
    assert_eq!(
        sim.snapshot(Viewer::WhiteCell).air_bases[0].aircraft.len(),
        1
    );
    order(
        &mut sim,
        10,
        CommandPayload::Adjudicate {
            reason: "ship lost".into(),
            action: sim_core::Adjudication::DestroyUnit { unit: UnitId(10) },
        },
    );
    sim.run_until(Tick(20));
    assert_eq!(
        count(&sim, |k| matches!(
            k,
            SimEventKind::AircraftLost {
                unit: UnitId(11),
                ..
            }
        )),
        1
    );
    assert!(sim.snapshot(Viewer::WhiteCell).air_bases[0]
        .aircraft
        .is_empty());
}

#[test]
fn air_operations_survive_a_save_and_run_on_identically() {
    let s = scenario(
        vec![
            fighter(1, serde_json::json!({})),
            fighter(2, serde_json::json!({"readiness": "alert15"})),
        ],
        true,
    );
    let mut a = build(&s);
    for u in 1..=2 {
        order(
            &mut a,
            10,
            CommandPayload::LaunchSortie {
                unit: UnitId(u),
                mission: sim_core::Mission::Ferry {
                    destination: GeoPoint::new(0.5, -170.0),
                },
                loadout: None,
            },
        );
    }
    a.run_until(Tick(400));
    let mut b = Simulation::from_save(a.save()).unwrap();
    b.set_land_mask(sim_geo::land::embedded());
    assert_eq!(a.state_hash(), b.state_hash());
    a.run_until(Tick(3_000));
    b.run_until(Tick(3_000));
    assert_eq!(a.state_hash(), b.state_hash());
    assert_eq!(launches(&a), launches(&b));
}
