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
