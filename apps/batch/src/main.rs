//! `nws-batch`: headless scenario runner.
//!
//! Phase 1 scope: run a scenario for one or more seeds and report the final state hash, used by
//! the CI cross-OS determinism matrix. Monte Carlo statistics and Arrow/Parquet export arrive in Phase 4.

#![forbid(unsafe_code)]

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde::Serialize;
use sim_core::Tick;
use sim_scenario::Scenario;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "nws-batch",
    version,
    about = "Headless naval wargame scenario runner"
)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run scenarios and print one JSON line per (scenario, seed) with the final state hash.
    Run {
        /// Scenario files (.json).
        #[arg(required = true)]
        scenarios: Vec<PathBuf>,
        /// Seeds to run, comma-separated. Defaults to the scenario's own seed.
        #[arg(long, value_delimiter = ',')]
        seeds: Vec<u64>,
        /// Override the run length in ticks (seconds).
        #[arg(long)]
        ticks: Option<u64>,
        /// Also write the results as a JSON array to this file.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Open platform database file (built with `sim-db build`). Defaults to the database
        /// embedded in this binary.
        #[arg(long)]
        db: Option<PathBuf>,
    },
}

#[derive(Serialize)]
struct RunResult {
    scenario: String,
    seed: u64,
    ticks: u64,
    final_hash: String,
    units: usize,
    events: usize,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Cmd::Run {
            scenarios,
            seeds,
            ticks,
            out,
            db,
        } => {
            let platforms = match db {
                Some(path) => sim_db::PlatformDb::open(&sim_db::DbSource::Open(path.clone()))
                    .with_context(|| format!("opening platform database {}", path.display()))?,
                None => sim_db::embedded::open_platform_db()
                    .context("building the embedded platform database")?,
            };
            let mut results = vec![];
            for path in &scenarios {
                let scenario =
                    Scenario::load(path).with_context(|| format!("loading {}", path.display()))?;
                let name = path.file_stem().map_or_else(
                    || path.display().to_string(),
                    |s| s.to_string_lossy().into_owned(),
                );
                let run_seeds = if seeds.is_empty() {
                    vec![scenario.seed]
                } else {
                    seeds.clone()
                };
                let end = ticks.map_or_else(|| scenario.duration_ticks(), Tick);
                for seed in run_seeds {
                    let mut sim = scenario.build(Some(seed), &platforms)?;
                    sim.run_until(end);
                    let r = RunResult {
                        scenario: name.clone(),
                        seed,
                        ticks: end.0,
                        final_hash: format!("{:016x}", sim.state_hash()),
                        units: sim.snapshot(sim_core::Viewer::WhiteCell).units.len(),
                        events: sim.events().len(),
                    };
                    println!("{}", serde_json::to_string(&r)?);
                    results.push(r);
                }
            }
            if let Some(out) = out {
                std::fs::write(&out, serde_json::to_string_pretty(&results)? + "\n")
                    .with_context(|| format!("writing {}", out.display()))?;
            }
            Ok(())
        }
    }
}
