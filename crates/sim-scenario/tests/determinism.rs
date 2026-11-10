//! Determinism and golden-replay tests over every shipped scenario (ADR-0003 §5).
//!
//! Golden files live in `/tests/golden/<scenario>.json`. To update them deliberately
//! (and explain why in the PR), run: `NWS_BLESS=1 cargo test -p sim-scenario --test determinism`.

#![allow(clippy::unwrap_used, clippy::expect_used)] // test harness

use serde::{Deserialize, Serialize};
use sim_core::{SaveGame, Simulation, Tick};
use sim_scenario::Scenario;
use std::path::{Path, PathBuf};

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
    let mut sim = s.build(Some(seed)).unwrap();
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

#[test]
fn save_midway_and_resume_matches_uninterrupted_run() {
    for (name, s) in shipped_scenarios() {
        let end = s.duration_ticks();
        let mid = Tick(end.0 / 2);
        let mut a = s.build(None).unwrap();
        a.run_until(mid);
        let bytes = a.save().to_bytes().unwrap();
        let mut b = Simulation::from_save(SaveGame::from_bytes(&bytes).unwrap()).unwrap();
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
