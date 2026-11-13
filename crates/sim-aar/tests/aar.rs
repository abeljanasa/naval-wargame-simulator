//! Replay and AAR over the shipped scenarios (`docs/models/replay-aar.md`).

#![allow(clippy::unwrap_used, clippy::expect_used)] // test harness

use sim_aar::{AarError, AarTable, ExerciseRecord, Replay, TimelineKind, RECORD_FORMAT_VERSION};
use sim_core::{
    Adjudication, Command, CommandPayload, Issuer, SeatRole, SideId, SimEventKind, Simulation,
    Tick, UnitId, Viewer,
};
use sim_scenario::Scenario;
use std::path::Path;

fn load(name: &str) -> (Scenario, String) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/scenarios")
        .join(name);
    let text = std::fs::read_to_string(path).unwrap();
    (Scenario::from_json(&text).unwrap(), text)
}

fn sides(s: &Scenario) -> Vec<(SideId, String)> {
    s.sides.iter().map(|x| (x.id, x.name.clone())).collect()
}

/// The Gate 2 surface action, with a white-cell override and a player order part-way through,
/// run for 3,600 s.
fn surface_action() -> (Scenario, String, Simulation) {
    let (s, text) = load("phase2-surface-action.json");
    let db = sim_db::embedded::open_platform_db().unwrap();
    let mut sim = s.build(None, &db).unwrap();
    sim.run_until(Tick(600));
    sim.submit(Command {
        tick: sim.tick(),
        issuer: Issuer::WhiteCell {
            umpire: "Umpire".into(),
        },
        side: None,
        payload: CommandPayload::Adjudicate {
            reason: "taken out for the test".into(),
            action: Adjudication::RemoveUnit { unit: UnitId(5) },
        },
    })
    .unwrap();
    sim.submit(Command {
        tick: sim.tick(),
        issuer: Issuer::Player { seat: 1 },
        side: Some(SideId(1)),
        payload: CommandPayload::SetSpeed {
            unit: UnitId(2),
            speed_mps: 8.0,
        },
    })
    .unwrap();
    sim.run_until(Tick(3_600));
    (s, text, sim)
}

#[test]
fn record_round_trips_and_replays_exactly() {
    let (s, text, mut live) = surface_action();
    let record = ExerciseRecord::capture(&mut live, &s.name, &text);
    let bytes = record.to_bytes().unwrap();
    let back = ExerciseRecord::from_bytes(&bytes).unwrap();
    assert_eq!(back, record);
    assert_eq!(back.end_tick, Tick(3_600));
    assert!(
        back.land_mask.is_some(),
        "the scenario navigates round land"
    );

    let replay = Replay::open(back, &sides(&s)).unwrap();
    assert!(
        replay.verified(),
        "the rebuilt run reaches the recorded hash"
    );
    assert_eq!(replay.tick(), Tick(3_600));
}

#[test]
fn seeking_matches_a_straight_run() {
    let (s, text, mut live) = surface_action();
    let record = ExerciseRecord::capture(&mut live, &s.name, &text);
    let mut replay = Replay::open(record.clone(), &sides(&s)).unwrap();
    let straight = |t: u64| {
        let mut sim = Simulation::new(record.seed);
        sim.set_land_mask(sim_geo::land::embedded());
        for c in &record.commands {
            sim.submit(c.command.clone()).unwrap();
        }
        sim.run_until(Tick(t));
        sim.snapshot(Viewer::WhiteCell)
    };
    // Backwards (from a keyframe), forwards (running on), across the override, and past the end.
    for t in [1_234, 61, 1_900, 601, 3_599] {
        replay.seek(Tick(t)).unwrap();
        assert_eq!(replay.tick(), Tick(t));
        assert_eq!(replay.snapshot(Viewer::WhiteCell), straight(t), "tick {t}");
    }
    replay.seek(Tick(99_999)).unwrap();
    assert_eq!(replay.tick(), Tick(3_600));
    assert!(replay.at_end());
    replay.seek(Tick(3_000)).unwrap();
    assert_eq!(replay.advance(10_000), 600, "advance stops at the end");
}

#[test]
fn a_tampered_record_is_reported() {
    let (s, text, mut live) = surface_action();
    let mut record = ExerciseRecord::capture(&mut live, &s.name, &text);
    for c in &mut record.commands {
        if let CommandPayload::SetSpeed { speed_mps, .. } = &mut c.command.payload {
            *speed_mps = 3.0;
        }
    }
    let replay = Replay::open(record, &sides(&s)).unwrap();
    assert!(!replay.verified());
}

#[test]
fn bad_records_are_refused() {
    assert!(matches!(
        ExerciseRecord::from_bytes(b"not a record"),
        Err(AarError::NotARecord)
    ));
    let (s, text, mut live) = surface_action();
    let mut record = ExerciseRecord::capture(&mut live, &s.name, &text);
    record.format_version = RECORD_FORMAT_VERSION + 1;
    assert!(matches!(
        ExerciseRecord::from_bytes(&record.to_bytes().unwrap()),
        Err(AarError::FutureVersion { .. })
    ));
    record.format_version = RECORD_FORMAT_VERSION;
    record.land_mask = Some("no-such-coastline".into());
    assert!(matches!(
        Replay::open(record, &sides(&s)),
        Err(AarError::UnknownLandMask(_))
    ));
}

#[test]
fn report_accounts_for_every_shot_and_loss() {
    let (s, text, mut live) = surface_action();
    let events = live.events().to_vec();
    let record = ExerciseRecord::capture(&mut live, &s.name, &text);
    let replay = Replay::open(record, &sides(&s)).unwrap();
    let r = replay.report();
    let count = |f: &dyn Fn(&SimEventKind) -> bool| {
        u32::try_from(events.iter().filter(|e| f(&e.kind)).count()).unwrap()
    };

    // Weapons expended: every launch and every gun or mine hit roll, each on one row.
    let launched = count(&|k| matches!(k, SimEventKind::WeaponLaunched { .. }));
    let instant = count(&|k| {
        matches!(
            k,
            SimEventKind::HitRoll {
                weapon_id: None,
                ..
            }
        )
    });
    let strike: u32 = r
        .weapons
        .iter()
        .filter(|w| !w.defensive)
        .map(|w| w.fired)
        .sum();
    assert!(launched > 0);
    assert_eq!(strike, launched + instant);
    let hits = count(&|k| matches!(k, SimEventKind::HitRoll { hit: true, .. }));
    assert_eq!(
        r.weapons
            .iter()
            .filter(|w| !w.defensive)
            .map(|w| w.hits)
            .sum::<u32>(),
        hits
    );
    let shots = count(&|k| matches!(k, SimEventKind::PointDefenceShot { .. }));
    assert_eq!(
        r.weapons
            .iter()
            .filter(|w| w.defensive)
            .map(|w| w.fired)
            .sum::<u32>(),
        shots
    );
    let intercepts = count(&|k| matches!(k, SimEventKind::InterceptRoll { killed: true, .. }));
    assert_eq!(
        r.weapons.iter().map(|w| w.intercepted).sum::<u32>(),
        intercepts
    );
    for w in &r.weapons {
        assert!(w.hits + w.intercepted <= w.fired, "{w:?}");
    }

    // Losses: every destruction, plus the removal, which is not credited to anyone.
    let destroyed = count(&|k| matches!(k, SimEventKind::UnitDestroyed { .. }));
    assert!(destroyed >= 1);
    assert_eq!(
        r.losses.iter().filter(|l| !l.removed).count(),
        destroyed as usize
    );
    let removal = r.losses.iter().find(|l| l.removed).unwrap();
    assert_eq!(removal.unit, UnitId(5));
    assert_eq!(removal.by_side, None);
    assert!(removal.cause.contains("taken out for the test"));
    let kills: u32 = r.sides.iter().map(|x| x.kills).sum();
    assert!(kills >= 1, "a kill is credited: {:?}", r.losses);
    assert_eq!(kills, r.weapons.iter().map(|w| w.kills).sum::<u32>());
    for side in &r.sides {
        assert_eq!(
            side.remaining + side.destroyed + side.removed,
            side.units_start + side.reinforcements,
            "{side:?}"
        );
    }

    // Detections: one row per contact gained.
    let gained = count(&|k| matches!(k, SimEventKind::ContactGained { .. }));
    assert_eq!(r.detections.len(), gained as usize);
    assert!(r.detections.iter().all(|d| d.held_s >= 0.0));

    // Timeline: in time order, with the player order and the override, attributed.
    assert!(r.timeline.windows(2).all(|w| w[0].tick <= w[1].tick));
    assert!(r
        .timeline
        .iter()
        .any(|e| e.kind == TimelineKind::Order && e.side == Some(SideId(1))));
    assert!(r
        .timeline
        .iter()
        .any(|e| e.kind == TimelineKind::Override && e.side.is_none()));
    // A side sees only its own lines.
    let blue = r.timeline_for(&SeatRole::Side { side: SideId(1) });
    assert!(!blue.is_empty() && blue.iter().all(|e| e.side == Some(SideId(1))));
    assert_eq!(r.timeline_for(&SeatRole::WhiteCell).len(), r.timeline.len());

    // CSV: a header and one line per row in every table.
    let rows = [
        (AarTable::Sides, r.sides.len()),
        (AarTable::Losses, r.losses.len()),
        (AarTable::Weapons, r.weapons.len()),
        (AarTable::Detections, r.detections.len()),
        (AarTable::Transits, r.transits.len()),
        (AarTable::Timeline, r.timeline.len()),
    ];
    for (table, n) in rows {
        let csv = r.csv(table);
        assert_eq!(csv.matches("\r\n").count(), n + 1, "{table:?}");
    }
}

#[test]
fn ttx_demo_timeline_shows_events() {
    let (s, text) = load("phase3-ttx-demo.json");
    let db = sim_db::embedded::open_platform_db().unwrap();
    let mut sim = s.build(None, &db).unwrap();
    sim.run_until(s.duration_ticks());
    let record = ExerciseRecord::capture(&mut sim, &s.name, &text);
    let replay = Replay::open(record, &sides(&s)).unwrap();
    assert!(replay.verified());
    let fired = replay
        .report()
        .timeline
        .iter()
        .filter(|e| e.kind == TimelineKind::Event && e.text.contains("e01-startex"))
        .count();
    assert_eq!(fired, 1);
}

/// The Gate 3 pilot scenario reaches every decision point its pilot plan relies on
/// (docs/gate3/ttx-pilot-plan.md §4), in the order the plan gives them.
#[test]
fn pilot_scenario_reaches_every_decision_point() {
    let (s, text) = load("phase3-pilot.json");
    let db = sim_db::embedded::open_platform_db().unwrap();
    let mut sim = s.build(None, &db).unwrap();
    sim.run_until(s.duration_ticks());
    let record = ExerciseRecord::capture(&mut sim, &s.name, &text);
    let replay = Replay::open(record, &sides(&s)).unwrap();
    assert!(replay.verified());
    let timeline = &replay.report().timeline;
    let first = |needle: &str| {
        timeline
            .iter()
            .position(|e| e.text.contains(needle))
            .unwrap_or_else(|| panic!("no timeline entry contains {needle:?}"))
    };
    let order = [
        "Warden diverted to facility 4 to refuel",
        "Tally loaded 800 t of cargo",
        "Ember laid 12 mines",
        "event e04-engage-zone fired",
        "command post 6 is down: link jammed",
        "Cinder destroyed",
        "command post 6 is back up",
        "Warden refuelled at facility 4",
        "event e06-gale fired",
        "Sieve sheltering from the weather",
        "Tally delivered 800 t of cargo",
        "event e07-gale-eases fired",
        "Tally holding at anchorage 3",
    ];
    let at: Vec<usize> = order.iter().map(|n| first(n)).collect();
    assert!(at.windows(2).all(|w| w[0] <= w[1]), "out of order: {at:?}");
}

/// The Philippine-area pilot (Mindoro Strait) reaches the same decision points, in order
/// (docs/gate3/ttx-pilot-plan.md §4).
#[test]
fn mindoro_pilot_reaches_every_decision_point() {
    let (s, text) = load("phase3-pilot-mindoro.json");
    let db = sim_db::embedded::open_platform_db().unwrap();
    let mut sim = s.build(None, &db).unwrap();
    sim.run_until(s.duration_ticks());
    let record = ExerciseRecord::capture(&mut sim, &s.name, &text);
    let replay = Replay::open(record, &sides(&s)).unwrap();
    assert!(replay.verified());
    let timeline = &replay.report().timeline;
    let first = |needle: &str| {
        timeline
            .iter()
            .position(|e| e.text.contains(needle))
            .unwrap_or_else(|| panic!("no timeline entry contains {needle:?}"))
    };
    let order = [
        "Warden diverted to facility 4 to refuel",
        "Tally loaded 800 t of cargo",
        "event e04-engage-zone fired",
        "Ember laid 12 mines",
        "command post 6 is down: link jammed",
        "Cinder destroyed",
        "command post 6 is back up",
        "event e06-weather fired",
        "Sieve sheltering from the weather",
        "Tally delivered 800 t of cargo",
        "event e07-weather-eases fired",
        "Tally holding at anchorage 3",
    ];
    let at: Vec<usize> = order.iter().map(|n| first(n)).collect();
    assert!(at.windows(2).all(|w| w[0] <= w[1]), "out of order: {at:?}");
}

/// Weapon outcomes reach each seat as fog of war allows: the white cell sees who hit whom; the
/// firing side sees its hit but not the target's identity; the target's side sees the hit on
/// its own unit but not who fired (sim-core outcomes.rs).
#[test]
fn hit_and_miss_feedback_follows_fog_of_war() {
    let (s, _) = load("phase3-pilot.json");
    let db = sim_db::embedded::open_platform_db().unwrap();
    let mut sim = s.build(None, &db).unwrap();
    // Cinder is sunk at 02:06:30 (7590 s) in the reference run (since collision avoidance,
    // docs/models/collisions.md: Blue's ships give way to each other on the way in).
    sim.run_until(sim_core::Tick(7596));
    let wc = sim.snapshot(sim_core::Viewer::WhiteCell).outcomes;
    let sunk = wc
        .iter()
        .find(|o| {
            o.result == sim_core::OutcomeKind::Hit
                && o.target_name.as_deref() == Some("Red corvette (FFL) — Cinder")
        })
        .expect("the white cell sees the hit on Cinder");
    assert!(sunk.shooter_name.is_some());
    let blue = sim
        .snapshot(sim_core::Viewer::Side {
            side: sim_core::SideId(1),
        })
        .outcomes;
    assert!(!blue.is_empty(), "Blue sees its own shots");
    assert!(blue
        .iter()
        .all(|o| o.target.is_none() && o.target_name.is_none()));
    let red = sim
        .snapshot(sim_core::Viewer::Side {
            side: sim_core::SideId(2),
        })
        .outcomes;
    assert!(red
        .iter()
        .any(|o| o.target_name.as_deref() == Some("Red corvette (FFL) — Cinder")));
    assert!(red
        .iter()
        .all(|o| o.shooter.is_none() && o.shooter_name.is_none()));
}

/// A small synthetic pack (no data files in the repository).
fn tiny_terrain(depth: i16) -> std::sync::Arc<sim_geo::terrain::Terrain> {
    use sim_geo::terrain::{encode, PackHeader, Terrain, TerrainPack};
    let header = PackHeader {
        name: "test-shelf".into(),
        source: "test".into(),
        credit: "test".into(),
        lat0: 0,
        lon0: 0,
        cell: 36_000,
        rows: 2,
        cols: 2,
        min_m: depth,
        max_m: depth,
    };
    let pack = TerrainPack::decode(&encode(&header, &[depth; 4]).unwrap()).unwrap();
    std::sync::Arc::new(Terrain::new(vec![std::sync::Arc::new(pack)]).unwrap())
}

#[test]
fn terrain_is_hashed_recorded_and_needed_to_replay() {
    let (s, text, mut live) = surface_action();
    let without = live.state_hash();
    live.set_terrain(Some(tiny_terrain(-50)));
    assert_ne!(
        live.state_hash(),
        without,
        "the terrain id is in the state hash"
    );
    let record = ExerciseRecord::capture(&mut live, &s.name, &text);
    let id = live.terrain_id().unwrap();
    assert_eq!(record.terrain.as_deref(), Some(id.as_str()));
    let back = ExerciseRecord::from_bytes(&record.to_bytes().unwrap()).unwrap();
    assert_eq!(back.terrain, record.terrain);

    // Without the packs, or with different ones, the record does not open.
    assert!(matches!(
        Replay::open(record.clone(), &sides(&s)),
        Err(AarError::UnknownTerrain(_))
    ));
    assert!(matches!(
        Replay::open_with_terrain(record.clone(), &sides(&s), Some(tiny_terrain(-51))),
        Err(AarError::UnknownTerrain(_))
    ));
    // With the same packs it replays and verifies, also after seeking back to a keyframe.
    let mut replay =
        Replay::open_with_terrain(record, &sides(&s), Some(tiny_terrain(-50))).unwrap();
    assert!(replay.verified());
    replay.seek(Tick(1_000)).unwrap();
    replay.seek(Tick(3_600)).unwrap();
    assert_eq!(replay.sim_mut().state_hash(), back.final_hash);
}
