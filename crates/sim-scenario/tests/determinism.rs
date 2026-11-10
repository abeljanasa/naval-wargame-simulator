//! Determinism and golden-replay tests over every shipped scenario (ADR-0003 §5).
//!
//! Golden files live in `/tests/golden/<scenario>.json`. To update them deliberately
//! (and explain why in the PR), run: `NWS_BLESS=1 cargo test -p sim-scenario --test determinism`.

#![allow(clippy::unwrap_used, clippy::expect_used)] // test harness

use serde::{Deserialize, Serialize};
use sim_core::{SaveGame, Simulation, Tick};
use sim_scenario::Scenario;
use std::path::{Path, PathBuf};

fn db() -> sim_db::PlatformDb {
    sim_db::embedded::open_platform_db().unwrap()
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn shipped_scenarios() -> Vec<(String, Scenario)> {
    let dir = repo_root().join("data/scenarios");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "no scenarios in {}", dir.display());
    paths
        .into_iter()
        .map(|p| {
            let stem = p.file_stem().unwrap().to_string_lossy().into_owned();
            let s = Scenario::load(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
            (stem, s)
        })
        .collect()
}

fn run(s: &Scenario, seed: u64) -> u64 {
    let mut sim = s.build(Some(seed), &db()).unwrap();
    sim.run_until(s.duration_ticks());
    sim.state_hash()
}

#[test]
fn every_scenario_is_deterministic_per_seed() {
    for (name, s) in shipped_scenarios() {
        for seed in [s.seed, 1, 0xDEAD_BEEF] {
            assert_eq!(run(&s, seed), run(&s, seed), "{name} seed {seed}");
        }
    }
}

/// Gate 1 criterion ("units move and detect") on the shipped demonstrator.
#[test]
fn gate1_demo_units_move_and_detect() {
    let s = Scenario::load(repo_root().join("data/scenarios/phase1-strait-transit.json")).unwrap();
    let mut sim = s.build(None, &db()).unwrap();
    sim.step();
    let start = sim.snapshot(sim_core::Viewer::WhiteCell);
    sim.run_until(s.duration_ticks());
    let end = sim.snapshot(sim_core::Viewer::WhiteCell);
    for (a, b) in start.units.iter().zip(&end.units) {
        assert_ne!(a.position, b.position, "{} did not move", a.name);
    }
    let gained: Vec<(u16, u64)> = sim
        .events()
        .iter()
        .filter_map(|e| match e.kind {
            sim_core::SimEventKind::ContactGained { side, target, .. } => Some((side.0, target.0)),
            _ => None,
        })
        .collect();
    assert!(
        gained.contains(&(1, 2)),
        "frigate never detected the merchant: {gained:?}"
    );
    assert!(
        gained.contains(&(2, 1)),
        "merchant never detected the frigate: {gained:?}"
    );
    // Both ships stay at sea: no grounding stop and no refused route.
    let land_events: Vec<_> = sim
        .events()
        .iter()
        .filter(|e| {
            matches!(
                e.kind,
                sim_core::SimEventKind::LandAhead { .. }
                    | sim_core::SimEventKind::RouteBlocked { .. }
            )
        })
        .collect();
    assert!(land_events.is_empty(), "{land_events:?}");
}

#[test]
fn save_midway_and_resume_matches_uninterrupted_run() {
    for (name, s) in shipped_scenarios() {
        let end = s.duration_ticks();
        let mid = Tick(end.0 / 2);
        let mut a = s.build(None, &db()).unwrap();
        a.run_until(mid);
        let bytes = a.save().to_bytes().unwrap();
        let mut b = Simulation::from_save(SaveGame::from_bytes(&bytes).unwrap()).unwrap();
        b.set_land_mask(sim_geo::land::embedded());
        a.run_until(end);
        b.run_until(end);
        assert_eq!(a.state_hash(), b.state_hash(), "{name}");
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct Golden {
    scenario: String,
    seed: u64,
    ticks: u64,
    /// Hex string: JSON numbers cannot hold every u64 exactly in all tools.
    final_hash: String,
}

#[test]
fn golden_replays() {
    let bless = std::env::var("NWS_BLESS").is_ok_and(|v| v == "1");
    let golden_dir = repo_root().join("tests/golden");
    let mut failures = vec![];
    for (name, s) in shipped_scenarios() {
        let actual = Golden {
            scenario: name.clone(),
            seed: s.seed,
            ticks: s.duration_ticks().0,
            final_hash: format!("{:016x}", run(&s, s.seed)),
        };
        let path = golden_dir.join(format!("{name}.json"));
        if bless {
            std::fs::create_dir_all(&golden_dir).unwrap();
            let text = serde_json::to_string_pretty(&actual).unwrap() + "\n";
            std::fs::write(&path, text).unwrap();
            continue;
        }
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                let expected: Golden = serde_json::from_str(&text).unwrap();
                if expected != actual {
                    failures.push(format!("{name}: expected {expected:?}, got {actual:?}"));
                }
            }
            Err(_) => failures.push(format!(
                "{name}: no golden file at {} (run with NWS_BLESS=1 to create it)",
                path.display()
            )),
        }
    }
    assert!(
        failures.is_empty(),
        "golden replay mismatch:\n{}",
        failures.join("\n")
    );
}

/// Gate 2 (core combat): the shipped surface-action scenario exercises the whole chain with its
/// own seed. Both sides launch; layered point defence (interceptors and decoys) engages; hits
/// do damage and at least one unit is destroyed; ships run the minefield's transit risk; the
/// neutral merchant is never engaged.
#[test]
fn gate2_demo_detects_engages_defends_and_damages() {
    use sim_core::SimEventKind as K;
    let s = Scenario::from_json(
        &std::fs::read_to_string(repo_root().join("data/scenarios/phase2-surface-action.json"))
            .unwrap(),
    )
    .unwrap();
    let mut sim = s.build(None, &db()).unwrap();
    sim.run_until(s.duration_ticks());
    let ev = sim.events();
    let count = |f: &dyn Fn(&K) -> bool| ev.iter().filter(|e| f(&e.kind)).count();
    let launched_by =
        |side: u16| count(&|k| matches!(k, K::WeaponLaunched { side: s, .. } if s.0 == side));
    assert!(launched_by(1) > 0 && launched_by(2) > 0, "both sides fire");
    assert!(count(&|k| matches!(k, K::PointDefenceShot { .. })) > 0);
    assert!(count(&|k| matches!(k, K::InterceptRoll { killed: true, .. })) > 0);
    assert!(count(&|k| matches!(k, K::DecoyLaunched { .. })) > 0);
    assert!(count(&|k| matches!(k, K::HitRoll { hit: true, .. })) > 0);
    assert!(count(&|k| matches!(k, K::DamageTaken { .. })) > 0);
    assert!(count(&|k| matches!(k, K::UnitDestroyed { .. })) >= 1);
    assert!(count(&|k| matches!(k, K::MineEncounter { .. })) > 0);
    // The merchant (unit 5) is never shot at or damaged.
    assert_eq!(
        count(&|k| matches!(
            k,
            K::HitRoll { target, .. } | K::DamageTaken { unit: target, .. }
                if target.0 == 5
        )),
        0
    );
}

/// Gate 3 (facilitated exercise): the shipped TTX demonstrator's events brief the players,
/// report first contact, escalate to hostilities (after which the fight is decided), and prompt
/// the white cell; a Blue loss ruled by the umpire brings in the reserve frigate.
#[test]
fn gate3_demo_events_drive_the_exercise() {
    use sim_core::{
        Adjudication, Command, CommandPayload, Issuer, SideId, SimEventKind as K, UnitId, Viewer,
    };
    let s = Scenario::load(repo_root().join("data/scenarios/phase3-ttx-demo.json")).unwrap();
    let fired = |sim: &Simulation| -> Vec<String> {
        sim.events()
            .iter()
            .filter_map(|e| match &e.kind {
                K::ScenarioEventFired { event } => Some(event.clone()),
                _ => None,
            })
            .collect()
    };
    let mut sim = s.build(None, &db()).unwrap();
    sim.run_until(s.duration_ticks());
    let f = fired(&sim);
    for e in [
        "e01-startex",
        "e02-intel",
        "e03-first-contact",
        "e04-escalation",
    ] {
        assert_eq!(
            f.iter().filter(|x| *x == e).count(),
            1,
            "{e} fires once: {f:?}"
        );
    }
    assert_eq!(f.iter().filter(|x| *x == "e06-sitrep").count(), 3, "{f:?}");
    assert!(sim
        .events()
        .iter()
        .any(|e| matches!(e.kind, K::UnitDestroyed { .. })));
    let blue = sim.snapshot(Viewer::Side { side: SideId(1) });
    assert_eq!(blue.messages.len(), 5);
    assert_eq!(
        sim.snapshot(Viewer::Side { side: SideId(2) })
            .messages
            .len(),
        3
    );
    assert_eq!(sim.snapshot(Viewer::WhiteCell).messages.len(), 8);
    assert_eq!(sim.snapshot(Viewer::WhiteCell).injects.len(), 3);

    let mut sim = s.build(None, &db()).unwrap();
    sim.submit(Command {
        tick: Tick(1200),
        issuer: Issuer::WhiteCell {
            umpire: "TTX".into(),
        },
        side: None,
        payload: CommandPayload::Adjudicate {
            reason: "Bastion ruled lost to a mine".into(),
            action: Adjudication::DestroyUnit { unit: UnitId(2) },
        },
    })
    .unwrap();
    sim.run_until(Tick(1300));
    assert!(fired(&sim).contains(&"e05-reinforce".to_owned()));
    assert!(sim
        .snapshot(Viewer::WhiteCell)
        .units
        .iter()
        .any(|u| u.id == UnitId(11)));
}
