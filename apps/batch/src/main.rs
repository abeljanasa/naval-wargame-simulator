//! `nws-batch`: headless scenario runner.
//!
//! Phase 1 scope: run a scenario for one or more seeds and report the final state hash, used by
//! the CI cross-OS determinism matrix. Monte Carlo statistics and Arrow/Parquet export arrive in Phase 4.

#![forbid(unsafe_code)]

mod bench;

use anyhow::{bail, Context, Result};
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
    /// Measure throughput (game seconds per wall second) against a performance budget.
    /// Build with `--release`; debug builds are 10–50× slower.
    Bench {
        /// Number of units (split evenly between two sides).
        #[arg(long, default_value_t = 1000)]
        units: usize,
        /// Ticks to time (game seconds).
        #[arg(long, default_value_t = 600)]
        ticks: u64,
        /// Platform database class to instantiate.
        #[arg(long, default_value = "GEN-FFG")]
        platform: String,
        /// Fail (exit 1) if the achieved ratio is below this.
        #[arg(long)]
        min_ratio: Option<f64>,
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
        Cmd::Bench {
            units,
            ticks,
            platform,
            min_ratio,
        } => {
            let db = sim_db::embedded::open_platform_db()?;
            let r = bench::run(&db, units, ticks, &platform, min_ratio)?;
            println!("{}", serde_json::to_string(&r)?);
            if !r.passed {
                bail!(
                    "budget missed: {:.1}x real time < required {:.1}x",
                    r.ratio,
                    r.min_ratio.unwrap_or_default()
                );
            }
            Ok(())
        }
    }
}
