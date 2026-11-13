#![allow(clippy::unwrap_used)] // test helpers: a failed setup should panic loudly

use proptest::prelude::*;
use sim_core::*;

fn spawn(id: u64, side: u16, lat: f64, lon: f64, route: Vec<GeoPoint>) -> Command {
    Command {
        tick: Tick(0),
        issuer: Issuer::Scenario,
        side: None,
        payload: CommandPayload::SpawnUnit(UnitSpawn {
            id: UnitId(id),
            name: format!("Unit {id}"),
            side: SideId(side),
            domain: Domain::Surface,
            position: GeoPoint::new(lat, lon),
            altitude_m: 0.0,
            heading_deg: 45.0,
            speed_mps: 8.0,
            max_speed_mps: 15.0,
            accel_mps2: 0.05,
            route,
            platform: None,
            sensors: vec![],
            rcs_m2: None,
            max_turn_rate_dps: None,
            min_turn_radius_m: None,
            group: None,
            doctrine: Doctrine::default(),
            jammer: None,
            radiated_noise_db: None,
            category: None,
            length_m: None,
            stores: Default::default(),
            damage_profile: None,
            mission: None,
            fuel: None,
            draft_m: None,
            displacement_t: None,
            air: None,
        }),
    }
}

fn scenario(seed: u64) -> Simulation {
    let mut sim = Simulation::new(seed);
    sim.submit(spawn(
        1,
        1,
        -33.85,
        151.25,
        vec![GeoPoint::new(-33.9, 151.5), GeoPoint::new(-34.2, 151.6)],
    ))
    .unwrap();
    sim.submit(spawn(2, 2, -34.0, 152.0, vec![])).unwrap();
    // A player command scheduled for later.
    sim.submit(Command {
        tick: Tick(600),
        issuer: Issuer::Player { seat: 1 },
        side: Some(SideId(1)),
        payload: CommandPayload::SetSpeed {
            unit: UnitId(1),
            speed_mps: 14.0,
        },
    })
    .unwrap();
    sim
}

#[test]
fn same_seed_same_hash() {
    let mut a = scenario(42);
    let mut b = scenario(42);
    a.run_ticks(3_600);
    b.run_ticks(3_600);
    assert_eq!(a.state_hash(), b.state_hash());
}

#[test]
fn different_seed_different_hash() {
    let mut a = scenario(1);
    let mut b = scenario(2);
    assert_ne!(a.state_hash(), b.state_hash());
}

#[test]
fn save_load_round_trip_continues_identically() {
    let mut original = scenario(7);
    original.run_ticks(500); // before the scheduled SetSpeed at T600
    let bytes = original.save().to_bytes().unwrap();

    let mut restored = Simulation::from_save(SaveGame::from_bytes(&bytes).unwrap()).unwrap();
    assert_eq!(restored.tick(), original.tick());
    assert_eq!(restored.state_hash(), original.state_hash());
    assert_eq!(restored.command_log(), original.command_log());

    original.run_ticks(2_000);
    restored.run_ticks(2_000);
    assert_eq!(restored.state_hash(), original.state_hash());
    assert_eq!(restored.events(), original.events());
    assert_eq!(
        restored.snapshot(Viewer::WhiteCell),
        original.snapshot(Viewer::WhiteCell)
    );
}

#[test]
fn replay_from_command_log_matches() {
    let mut live = scenario(11);
    live.run_ticks(300);
    // A mid-run command, as from the UI.
    live.submit(Command {
        tick: live.tick(),
        issuer: Issuer::Player { seat: 2 },
        side: Some(SideId(2)),
        payload: CommandPayload::SetRoute {
            unit: UnitId(2),
            waypoints: vec![GeoPoint::new(-34.5, 152.2)],
        },
    })
    .unwrap();
    live.run_ticks(1_500);

    let mut replayed = Simulation::replay(11, &live.command_log(), live.tick()).unwrap();
    assert_eq!(replayed.state_hash(), live.state_hash());
}

#[test]
fn save_rejects_future_version_and_garbage() {
    let mut sim = scenario(1);
    let mut save = sim.save();
    save.format_version = SAVE_FORMAT_VERSION + 1;
    let bytes = save.to_bytes().unwrap();
    assert!(matches!(
        SaveGame::from_bytes(&bytes),
        Err(SimError::UnknownFutureVersion { .. })
    ));
    assert!(matches!(
        SaveGame::from_bytes(b"not a save"),
        Err(SimError::NotASave)
    ));
}

#[test]
fn commands_in_the_past_are_refused() {
    let mut sim = scenario(1);
    sim.run_ticks(10);
    let err = sim
        .submit(Command {
            tick: Tick(5),
            issuer: Issuer::Ai,
            side: Some(SideId(1)),
            payload: CommandPayload::SetSpeed {
                unit: UnitId(1),
                speed_mps: 1.0,
            },
        })
        .unwrap_err();
    assert!(matches!(err, SimError::CommandInPast { .. }));
}

#[test]
fn player_cannot_command_other_side() {
    let mut sim = scenario(1);
    let seq = sim
        .submit(Command {
            tick: Tick(0),
            issuer: Issuer::Player { seat: 1 },
            side: Some(SideId(1)),
            payload: CommandPayload::SetSpeed {
                unit: UnitId(2),
                speed_mps: 0.0,
            },
        })
        .unwrap();
    sim.step();
    assert!(sim.events().iter().any(|e| matches!(
        &e.kind,
        SimEventKind::CommandRejected { seq: s, .. } if *s == seq
    )));
}

#[test]
fn side_snapshot_hides_other_sides() {
    let mut sim = scenario(1);
    sim.step();
    let blue = sim.snapshot(Viewer::Side { side: SideId(1) });
    assert_eq!(blue.units.len(), 1);
    assert_eq!(blue.units[0].id, UnitId(1));
    assert_eq!(sim.snapshot(Viewer::WhiteCell).units.len(), 2);
}

#[test]
fn route_completes_and_unit_stops() {
    let mut sim = Simulation::new(0);
    sim.submit(spawn(1, 1, 0.0, 0.0, vec![GeoPoint::new(0.0, 0.01)]))
        .unwrap();
    sim.run_ticks(1_000);
    let s = sim.snapshot(Viewer::WhiteCell);
    assert_eq!(s.units[0].speed_mps, 0.0);
    assert_eq!(s.units[0].position, GeoPoint::new(0.0, 0.01));
    assert!(sim
        .events()
        .iter()
        .any(|e| matches!(e.kind, SimEventKind::RouteCompleted { unit } if unit == UnitId(1))));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    /// Invariants: units are conserved, speeds stay in [0, max], positions stay valid.
    #[test]
    fn movement_invariants(
        seed in any::<u64>(),
        lat in -60.0f64..60.0, lon in -180.0f64..180.0,
        wps in proptest::collection::vec((-60.0f64..60.0, -180.0f64..180.0), 0..5),
        speeds in proptest::collection::vec(0.0f64..40.0, 1..5),
    ) {
        let mut sim = Simulation::new(seed);
        let route = wps.iter().map(|&(a, b)| GeoPoint::new(a, b)).collect();
        sim.submit(spawn(1, 1, lat, lon, route)).unwrap();
        sim.submit(spawn(2, 2, -lat, lon, vec![])).unwrap();
        for (i, s) in speeds.iter().enumerate() {
            sim.submit(Command {
                tick: Tick(i as u64 * 50),
                issuer: Issuer::WhiteCell { umpire: "test".into() },
                side: None,
                payload: CommandPayload::SetSpeed { unit: UnitId(1), speed_mps: *s },
            }).unwrap();
        }
        for _ in 0..10 {
            sim.run_ticks(30);
            let snap = sim.snapshot(Viewer::WhiteCell);
            prop_assert_eq!(snap.units.len(), 2);
            for u in &snap.units {
                prop_assert!(u.speed_mps >= 0.0 && u.speed_mps <= u.max_speed_mps);
                prop_assert!(u.position.is_valid());
                prop_assert!((0.0..360.0).contains(&u.heading_deg));
            }
        }
    }
}
