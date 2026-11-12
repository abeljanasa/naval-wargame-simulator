//! `nws-batch`: headless scenario runner.
//!
//! `run` reports final state hashes (the CI cross-OS determinism matrix); `aar` writes the AAR
//! tables of one run; `mc` runs many seeds in parallel and summarises the measures of
//! effectiveness (Phase 4 slice 1); `bench` measures throughput.

#![forbid(unsafe_code)]

mod bench;
mod mc;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use serde::Serialize;
use sim_aar::{AarTable, ExerciseRecord, Replay};
use sim_core::Tick;
use sim_scenario::Scenario;
use std::path::{Path, PathBuf};

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
    /// After-action review: replay an exercise record (or run a scenario and record it), check
    /// the replay against the recorded hash, and write the AAR tables as CSV files.
    Aar {
        /// An exercise record (`.nwsrec`, saved from the desktop app) or a scenario (`.json`).
        input: PathBuf,
        /// Directory for `sides.csv`, `losses.csv`, `weapons.csv`, `detections.csv`,
        /// `timeline.csv` (and `record.nwsrec` when a scenario was run).
        #[arg(long)]
        out: PathBuf,
        /// Scenario runs only: the seed (default: the scenario's own).
        #[arg(long)]
        seed: Option<u64>,
        /// Scenario runs only: the run length in ticks (default: the scenario's duration).
        #[arg(long)]
        ticks: Option<u64>,
        /// Open platform database file. Defaults to the embedded database.
        #[arg(long)]
        db: Option<PathBuf>,
    },
    /// Monte Carlo: run a scenario for many seeds in parallel and summarise its measures of
    /// effectiveness (mean, SD, 95 % CI, median, range, histogram). Writes `runs.csv`,
    /// `summary.csv` and `summary.json`.
    Mc {
        /// Scenario file (.json).
        scenario: PathBuf,
        /// Number of runs (seeds `seed`, `seed + 1`, …).
        #[arg(long, default_value_t = 30)]
        runs: u64,
        /// First seed (default: the scenario's own).
        #[arg(long)]
        seed: Option<u64>,
        /// Worker threads (default: the machine's cores).
        #[arg(long)]
        threads: Option<usize>,
        /// Override the run length in ticks (seconds).
        #[arg(long)]
        ticks: Option<u64>,
        /// Output directory.
        #[arg(long)]
        out: PathBuf,
        /// Open platform database file. Defaults to the embedded database.
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
            let platforms = platforms(db.as_deref())?;
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
        Cmd::Aar {
            input,
            out,
            seed,
            ticks,
            db,
        } => aar(&input, &out, seed, ticks, db.as_deref()),
        Cmd::Mc {
            scenario,
            runs,
            seed,
            threads,
            ticks,
            out,
            db,
        } => {
            let s = Scenario::load(&scenario)
                .with_context(|| format!("loading {}", scenario.display()))?;
            let first = seed.unwrap_or(s.seed);
            let seeds: Vec<u64> = (0..runs).map(|i| first.wrapping_add(i)).collect();
            let end = ticks.map_or_else(|| s.duration_ticks(), Tick);
            let threads = threads.unwrap_or_else(|| {
                std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get)
            });
            let open = || platforms(db.as_deref());
            let results = mc::run(&s, &seeds, end, threads, &open)?;
            let stats = mc::summarise(&results);
            mc::write(&out, &s.name, &results, &stats)?;
            println!(
                "{}",
                serde_json::json!({ "scenario": s.name, "runs": results.len(), "threads": threads, "out": out })
            );
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

fn platforms(db: Option<&Path>) -> Result<sim_db::PlatformDb> {
    match db {
        Some(path) => sim_db::PlatformDb::open(&sim_db::DbSource::Open(path.to_path_buf()))
            .with_context(|| format!("opening platform database {}", path.display())),
        None => {
            sim_db::embedded::open_platform_db().context("building the embedded platform database")
        }
    }
}

#[derive(Serialize)]
struct AarResult {
    scenario: String,
    seed: u64,
    ticks: u64,
    verified: bool,
    losses: usize,
    timeline: usize,
}

fn aar(
    input: &Path,
    out: &Path,
    seed: Option<u64>,
    ticks: Option<u64>,
    db: Option<&Path>,
) -> Result<()> {
    let bytes = std::fs::read(input).with_context(|| format!("reading {}", input.display()))?;
    std::fs::create_dir_all(out).with_context(|| format!("creating {}", out.display()))?;
    let record = match ExerciseRecord::from_bytes(&bytes) {
        Ok(r) => r,
        Err(sim_aar::AarError::NotARecord) => {
            // A scenario: run it and keep the record next to the tables.
            let text = String::from_utf8(bytes).context("input is neither a record nor text")?;
            let scenario = Scenario::from_json(&text)
                .with_context(|| format!("loading {}", input.display()))?;
            let mut sim = scenario.build(seed, &platforms(db)?)?;
            sim.run_until(ticks.map_or_else(|| scenario.duration_ticks(), Tick));
            let record = ExerciseRecord::capture(&mut sim, &scenario.name, text);
            let path = out.join("record.nwsrec");
            std::fs::write(&path, record.to_bytes()?)
                .with_context(|| format!("writing {}", path.display()))?;
            record
        }
        Err(e) => return Err(e).with_context(|| format!("reading {}", input.display())),
    };
    let scenario = Scenario::from_json(&record.scenario_json).context("the record's scenario")?;
    let sides: Vec<_> = scenario
        .sides
        .iter()
        .map(|s| (s.id, s.name.clone()))
        .collect();
    let replay = Replay::open(record, &sides)?;
    let report = replay.report();
    for table in AarTable::ALL {
        let path = out.join(format!("{}.csv", table.file_stem()));
        std::fs::write(&path, report.csv(table))
            .with_context(|| format!("writing {}", path.display()))?;
    }
    let r = AarResult {
        scenario: replay.record().scenario_name.clone(),
        seed: replay.record().seed,
        ticks: replay.end_tick().0,
        verified: replay.verified(),
        losses: report.losses.len(),
        timeline: report.timeline.len(),
    };
    println!("{}", serde_json::to_string(&r)?);
    if !replay.verified() {
        bail!("the replay did not reach the recorded final state (record altered, or a determinism fault)");
    }
    Ok(())
}
