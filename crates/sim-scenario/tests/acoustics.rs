//! Acoustics in the running engine (docs/models/acoustics.md): sound-velocity profiles set the
//! path sound takes, towed arrays listen only streamed, and sonobuoys hear where they float.

#![allow(clippy::unwrap_used)] // test harness

use sim_core::{
    AcousticPath, Command, CommandPayload, GeoPoint, Issuer, SideId, SimEventKind, Simulation,
    Tick, UnitId, Viewer, BUOY_LIFE_S, TOW_TRANSITION_S,
};
use sim_scenario::Scenario;

/// A surface layer 60 m deep over a deep sound channel near 1000 m.
fn ducting_profile() -> serde_json::Value {
    serde_json::json!([{
        "id": 1, "name": "Winter mixed layer",
        "points": [
            {"depth_m": 0, "speed_mps": 1500},
            {"depth_m": 60, "speed_mps": 1501},
            {"depth_m": 1000, "speed_mps": 1480},
            {"depth_m": 4000, "speed_mps": 1515}
        ]
    }])
}

/// `hunter` at the origin, a submarine at 200 m `range_km` east.
fn scenario(hunter: &str, altitude_m: f64, range_km: f64, profiles: serde_json::Value) -> Scenario {
    let lon = -170.0 + range_km / 111.32;
    Scenario::from_json(
        &serde_json::json!({
            "format": "nws-scenario", "format_version": 4, "name": "acoustics", "seed": 5,
            "duration_s": 20000,
            "environment": {"sea_state": 2, "start_unix_s": 1794301200},
            "sound_profiles": profiles,
            "sides": [
                {"id": 1, "name": "Blue", "doctrine": {"emcon": "silent"}},
                {"id": 2, "name": "Red", "doctrine": {"emcon": "silent"}}
            ],
            "units": [
                {"id": 1, "name": "Hunter", "side": 1, "platform": hunter,
                 "position": {"lat_deg": 0.0, "lon_deg": -170.0}, "altitude_m": altitude_m,
                 "heading_deg": 90.0, "speed_mps": 0.0},
                {"id": 2, "name": "Boat", "side": 2, "platform": "GEN-SSK",
                 "position": {"lat_deg": 0.0, "lon_deg": lon}, "altitude_m": -200.0,
                 "heading_deg": 0.0, "speed_mps": 3.0}
            ]
        })
        .to_string(),
    )
    .unwrap()
}

fn build(s: &Scenario) -> Simulation {
    let db = sim_db::embedded::open_platform_db().unwrap();
    s.build(None, &db).unwrap()
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

/// (tick, sensor, path) of the hunter's (or its buoys') sonar attempts on the boat.
fn attempts(sim: &Simulation) -> Vec<(u64, String, Option<AcousticPath>)> {
    sim.events()
        .iter()
        .filter_map(|e| match &e.kind {
            SimEventKind::SonarAttempt {
                observer,
                sensor,
                target,
                path,
                ..
            } if *observer == UnitId(1) && *target == UnitId(2) => {
                Some((e.tick.0, sensor.clone(), *path))
            }
            _ => None,
        })
        .collect()
}

fn paths_of(sim: &Simulation, sensor: &str) -> Vec<Option<AcousticPath>> {
    attempts(sim)
        .into_iter()
        .filter(|a| a.1 == sensor)
        .map(|a| a.2)
        .collect()
}

#[test]
fn a_profile_sends_hull_sonar_across_the_layer_and_the_towed_array_direct() {
    let mut sim = build(&scenario("GEN-FFG-ASW", 0.0, 5.0, ducting_profile()));
    sim.run_until(Tick(300));
    let hull: Vec<_> = attempts(&sim)
        .into_iter()
        .filter(|a| a.1 != "SEN-SON-TAS")
        .map(|a| a.2)
        .collect();
    let towed = paths_of(&sim, "SEN-SON-TAS");
    assert!(!hull.is_empty() && !towed.is_empty());
    // The hull sonar at 6 m hears the boat at 200 m only across the layer.
    assert!(
        hull.iter().all(|p| *p == Some(AcousticPath::CrossLayer)),
        "{hull:?}"
    );
    // The array at 150 m is below the layer with it.
    assert!(
        towed.iter().all(|p| *p == Some(AcousticPath::Direct)),
        "{towed:?}"
    );

    // Beyond the direct-path reach the hull sonar is in the shadow zone.
    let mut far = build(&scenario("GEN-FFG-ASW", 0.0, 9.0, ducting_profile()));
    far.run_until(Tick(300));
    let shadowed = attempts(&far)
        .into_iter()
        .filter(|a| a.1 != "SEN-SON-TAS")
        .all(|a| a.2 == Some(AcousticPath::Shadow));
    assert!(shadowed);
}

#[test]
fn without_a_profile_the_phase_2_law_holds() {
    let mut sim = build(&scenario("GEN-FFG-ASW", 0.0, 5.0, serde_json::json!([])));
    sim.run_until(Tick(300));
    let all = attempts(&sim);
    assert!(!all.is_empty());
    assert!(all.iter().all(|a| a.2.is_none()));
}

#[test]
fn a_recovered_array_is_deaf_until_streamed_and_ready() {
    let mut sim = build(&scenario("GEN-FFG-ASW", 0.0, 5.0, serde_json::json!([])));
    let streamed = |unit, streamed| CommandPayload::StreamTowedArray { unit, streamed };
    order(&mut sim, 300, streamed(UnitId(1), false));
    order(&mut sim, 1000, streamed(UnitId(1), true));
    sim.run_until(Tick(1000 + TOW_TRANSITION_S + 600));

    let towed: Vec<u64> = attempts(&sim)
        .into_iter()
        .filter(|a| a.1 == "SEN-SON-TAS")
        .map(|a| a.0)
        .collect();
    assert!(towed.iter().any(|&t| t < 300), "streamed at the start");
    assert!(
        !towed
            .iter()
            .any(|&t| (301..1000 + TOW_TRANSITION_S).contains(&t)),
        "deaf while recovered and while streaming: {towed:?}"
    );
    assert!(
        towed.iter().any(|&t| t >= 1000 + TOW_TRANSITION_S),
        "ready again"
    );
    let orders = sim
        .events()
        .iter()
        .filter(|e| matches!(e.kind, SimEventKind::TowedArray { .. }))
        .count();
    assert_eq!(orders, 2);
}

#[test]
fn a_ship_without_an_array_cannot_stream_one() {
    let mut sim = build(&scenario("GEN-OPV", 0.0, 5.0, serde_json::json!([])));
    order(
        &mut sim,
        0,
        CommandPayload::StreamTowedArray {
            unit: UnitId(1),
            streamed: false,
        },
    );
    sim.run_until(Tick(5));
    let refused = sim.events().iter().any(|e| {
        matches!(&e.kind, SimEventKind::CommandRejected { reason, .. } if reason.contains("no towed array"))
    });
    assert!(refused);
    assert!(!sim
        .events()
        .iter()
        .any(|e| matches!(e.kind, SimEventKind::TowedArray { .. })));
}

#[test]
fn sonobuoys_are_laid_listen_for_their_side_survive_a_save_and_expire() {
    let mut sim = build(&scenario("GEN-HELO-ASW", 150.0, 6.0, ducting_profile()));
    order(
        &mut sim,
        0,
        CommandPayload::LaySonobuoys {
            unit: UnitId(1),
            points: vec![GeoPoint::new(0.0, -170.0), GeoPoint::new(0.0, -169.98)],
            deep: true,
        },
    );
    sim.run_until(Tick(600));

    let laid = sim
        .events()
        .iter()
        .filter(|e| matches!(e.kind, SimEventKind::SonobuoyLaid { .. }))
        .count();
    assert_eq!(laid, 2);
    let views =
        |sim: &mut Simulation, side| sim.snapshot(Viewer::Side { side: SideId(side) }).sonobuoys;
    assert_eq!(views(&mut sim, 1).len(), 2);
    assert!(
        views(&mut sim, 2).is_empty(),
        "the other side does not see them"
    );
    assert!(views(&mut sim, 1)
        .iter()
        .all(|b| (b.depth_m - 300.0).abs() < 1e-9));

    let buoy = attempts(&sim)
        .into_iter()
        .filter(|a| a.1.starts_with("SEN-SONOBUOY#"))
        .collect::<Vec<_>>();
    assert!(!buoy.is_empty(), "the buoys listen");
    // Both deep with the boat: below the layer, direct.
    assert!(
        buoy.iter().all(|a| a.2 == Some(AcousticPath::Direct)),
        "{buoy:?}"
    );

    // A save carries them and the run continues identically.
    let save = sim.save();
    let mut restored = Simulation::from_save(save).unwrap();
    // The host restores the land mask; it is not part of a save.
    restored.set_land_mask(sim_geo::land::embedded());
    assert_eq!(restored.state_hash(), sim.state_hash());
    assert_eq!(views(&mut restored, 1).len(), 2);

    restored.run_until(Tick(BUOY_LIFE_S + 120));
    let expired = restored
        .events()
        .iter()
        .filter(|e| matches!(e.kind, SimEventKind::SonobuoyExpired { .. }))
        .count();
    assert_eq!(expired, 2);
    assert!(views(&mut restored, 1).is_empty());
}
