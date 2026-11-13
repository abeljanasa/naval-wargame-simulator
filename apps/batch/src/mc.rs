//! Monte Carlo batch (Phase 4 slice 1, `docs/plan/phase-4.md`): run one scenario for many seeds
//! in parallel, measure each run, and summarise the measures of effectiveness (MOEs).
//!
//! Each run is a whole, independent, deterministic simulation (one per thread at a time), so a
//! run's results depend only on its seed, never on the thread count or the order runs finish.
//! Results are sorted by seed before anything is written.
//!
//! **Default MOEs** (until the client names its own; Phase 4 plan decision 5), per side:
//! - `losses`: units destroyed;
//! - `first_detection_s`: game time of the side's first contact on another side's unit;
//! - `weapons_per_kill`: strike weapons fired per enemy unit destroyed;
//! - `transits`: the side's entries into a choke point.
//!
//! Plus, for the whole run, `cargo_delivered_t`. A measure that does not occur in a run (no
//! kill, no detection) is left out of that run's statistics, and `n` says how many runs had it.

use anyhow::{Context, Result};
use serde::Serialize;
use sim_aar::AarReport;
use sim_core::{SideId, SimEventKind, Tick};
use sim_scenario::Scenario;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

/// One run's measures, by name (e.g. `Blue.losses`). `None`: did not occur in this run.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RunMoes {
    pub seed: u64,
    pub final_hash: String,
    pub moes: BTreeMap<String, Option<f64>>,
}

/// Summary statistics of one measure over the runs where it occurred.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Stat {
    pub moe: String,
    /// Runs in which the measure occurred.
    pub n: usize,
    pub mean: f64,
    /// Sample standard deviation (n − 1); 0 for a single run.
    pub sd: f64,
    /// 95 % confidence interval of the mean, normal approximation: mean ± 1.96 · sd / √n.
    pub ci95: [f64; 2],
    pub min: f64,
    pub median: f64,
    pub max: f64,
    /// Ten equal-width bins from min to max: counts.
    pub histogram: Vec<usize>,
}

/// Measures one finished run.
fn measure(scenario: &Scenario, sim: &mut sim_core::Simulation, seed: u64, end: Tick) -> RunMoes {
    let sides: Vec<(SideId, String)> = scenario
        .sides
        .iter()
        .map(|s| (s.id, s.name.clone()))
        .collect();
    let report = AarReport::build(&sim.command_log(), sim.events(), &sides, end);
    let side_of: BTreeMap<_, _> = scenario.units.iter().map(|u| (u.id, u.side)).collect();
    let mut moes = BTreeMap::new();
    for (side, name) in &sides {
        let summary = report.sides.iter().find(|s| s.side == *side);
        let losses = summary.map_or(0, |s| s.destroyed);
        let fired = summary.map_or(0, |s| s.weapons_fired);
        let kills = summary.map_or(0, |s| s.kills);
        let first = report
            .detections
            .iter()
            .filter(|d| d.side == *side && d.target_side != Some(*side))
            .map(|d| d.gained.0)
            .min();
        let transits = sim
            .events()
            .iter()
            .filter(|e| match &e.kind {
                SimEventKind::ChokePointTransit { unit, .. } => side_of.get(unit) == Some(side),
                _ => false,
            })
            .count();
        moes.insert(format!("{name}.losses"), Some(f64::from(losses)));
        moes.insert(format!("{name}.first_detection_s"), first.map(|t| t as f64));
        moes.insert(
            format!("{name}.weapons_per_kill"),
            (kills > 0).then(|| f64::from(fired) / f64::from(kills)),
        );
        moes.insert(format!("{name}.transits"), Some(transits as f64));
    }
    let cargo: f64 = sim
        .events()
        .iter()
        .filter_map(|e| match &e.kind {
            SimEventKind::CargoDelivered { tonnes, .. } => Some(*tonnes),
            _ => None,
        })
        .sum();
    moes.insert("cargo_delivered_t".into(), Some(cargo));
    RunMoes {
        seed,
        final_hash: format!("{:016x}", sim.state_hash()),
        moes,
    }
}

/// Runs `seeds` of `scenario` on up to `threads` threads; each thread opens its own platform
/// database with `open_db`. Results come back sorted by seed.
pub fn run(
    scenario: &Scenario,
    seeds: &[u64],
    end: Tick,
    threads: usize,
    open_db: &(dyn Fn() -> Result<sim_db::PlatformDb> + Sync),
    terrain: Option<&std::sync::Arc<sim_geo::terrain::Terrain>>,
) -> Result<Vec<RunMoes>> {
    run_with(
        scenario,
        seeds,
        end,
        threads,
        open_db,
        terrain,
        &|sim, seed| measure(scenario, sim, seed, end),
    )
}

/// Runs `seeds` of `scenario` in parallel and measures each finished run with `measure`.
/// Results come back in seed order.
pub fn run_with<R: Send>(
    scenario: &Scenario,
    seeds: &[u64],
    end: Tick,
    threads: usize,
    open_db: &(dyn Fn() -> Result<sim_db::PlatformDb> + Sync),
    terrain: Option<&std::sync::Arc<sim_geo::terrain::Terrain>>,
    measure: &(dyn Fn(&mut sim_core::Simulation, u64) -> R + Sync),
) -> Result<Vec<R>> {
    let next = AtomicUsize::new(0);
    let results = Mutex::new(Vec::with_capacity(seeds.len()));
    let threads = threads.clamp(1, seeds.len().max(1));
    std::thread::scope(|scope| -> Result<()> {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| -> Result<()> {
                    let db = open_db()?;
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        let Some(&seed) = seeds.get(i) else {
                            return Ok(());
                        };
                        let mut sim = scenario.build(Some(seed), &db)?;
                        sim.set_terrain(terrain.cloned());
                        sim.run_until(end);
                        let r = measure(&mut sim, seed);
                        results
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .push((seed, r));
                    }
                })
            })
            .collect();
        for w in workers {
            w.join()
                .map_err(|_| anyhow::anyhow!("a Monte Carlo worker panicked"))??;
        }
        Ok(())
    })?;
    let mut out = results
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    out.sort_by_key(|(seed, _)| *seed);
    Ok(out.into_iter().map(|(_, r)| r).collect())
}

/// Statistics of every measure over the runs.
#[must_use]
pub fn summarise(runs: &[RunMoes]) -> Vec<Stat> {
    let names: Vec<String> = runs
        .first()
        .map(|r| r.moes.keys().cloned().collect())
        .unwrap_or_default();
    names
        .into_iter()
        .map(|moe| {
            let mut v: Vec<f64> = runs
                .iter()
                .filter_map(|r| r.moes.get(&moe).copied().flatten())
                .collect();
            stat(moe, &mut v)
        })
        .collect()
}

fn stat(moe: String, v: &mut [f64]) -> Stat {
    let n = v.len();
    if n == 0 {
        return Stat {
            moe,
            n,
            mean: f64::NAN,
            sd: f64::NAN,
            ci95: [f64::NAN, f64::NAN],
            min: f64::NAN,
            median: f64::NAN,
            max: f64::NAN,
            histogram: vec![0; 10],
        };
    }
    v.sort_by(f64::total_cmp);
    let mean = v.iter().sum::<f64>() / n as f64;
    let sd = if n > 1 {
        (v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1) as f64).sqrt()
    } else {
        0.0
    };
    let half = 1.96 * sd / (n as f64).sqrt();
    let (min, max) = (v[0], v[n - 1]);
    let median = if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    };
    let mut histogram = vec![0usize; 10];
    for x in v.iter() {
        let bin = if max > min {
            (((x - min) / (max - min)) * 10.0).floor() as usize
        } else {
            0
        };
        histogram[bin.min(9)] += 1;
    }
    Stat {
        moe,
        n,
        mean,
        sd,
        ci95: [mean - half, mean + half],
        min,
        median,
        max,
        histogram,
    }
}

fn num(x: f64) -> String {
    if x.is_nan() {
        String::new()
    } else {
        format!("{x}")
    }
}

/// Writes the runs as `runs.parquet` (Apache Parquet): `seed` (UInt64), `final_hash` (Utf8),
/// then one nullable Float64 column per measure (null where it did not occur).
fn write_parquet(path: &Path, runs: &[RunMoes]) -> Result<()> {
    use arrow_array::{ArrayRef, Float64Array, RecordBatch, StringArray, UInt64Array};
    use arrow_schema::{DataType, Field, Schema};
    use std::sync::Arc;
    let names: Vec<String> = runs
        .first()
        .map(|r| r.moes.keys().cloned().collect())
        .unwrap_or_default();
    let mut fields = vec![
        Field::new("seed", DataType::UInt64, false),
        Field::new("final_hash", DataType::Utf8, false),
    ];
    let mut columns: Vec<ArrayRef> = vec![
        Arc::new(UInt64Array::from_iter_values(runs.iter().map(|r| r.seed))),
        Arc::new(StringArray::from_iter_values(
            runs.iter().map(|r| r.final_hash.as_str()),
        )),
    ];
    for n in &names {
        fields.push(Field::new(n, DataType::Float64, true));
        columns.push(Arc::new(Float64Array::from_iter(
            runs.iter().map(|r| r.moes.get(n).copied().flatten()),
        )));
    }
    let batch = RecordBatch::try_new(Arc::new(Schema::new(fields)), columns)?;
    let file =
        std::fs::File::create(path).with_context(|| format!("creating {}", path.display()))?;
    let mut writer = parquet::arrow::ArrowWriter::try_new(file, batch.schema(), None)?;
    writer.write(&batch)?;
    writer.close()?;
    Ok(())
}

/// Writes `runs.csv`, `runs.parquet`, `summary.csv` and `summary.json` to `out`.
pub fn write(out: &Path, scenario: &str, runs: &[RunMoes], stats: &[Stat]) -> Result<()> {
    std::fs::create_dir_all(out).with_context(|| format!("creating {}", out.display()))?;
    let names: Vec<&String> = runs
        .first()
        .map(|r| r.moes.keys().collect())
        .unwrap_or_default();
    let mut csv = String::from("seed,final_hash");
    for n in &names {
        csv.push(',');
        csv.push_str(n);
    }
    csv.push('\n');
    for r in runs {
        csv.push_str(&format!("{},{}", r.seed, r.final_hash));
        for n in &names {
            csv.push(',');
            if let Some(Some(x)) = r.moes.get(*n) {
                csv.push_str(&num(*x));
            }
        }
        csv.push('\n');
    }
    std::fs::write(out.join("runs.csv"), csv)?;
    write_parquet(&out.join("runs.parquet"), runs)?;
    let mut sum = String::from("moe,n,mean,sd,ci95_low,ci95_high,min,median,max\n");
    for s in stats {
        sum.push_str(&format!(
            "{},{},{},{},{},{},{},{},{}\n",
            s.moe,
            s.n,
            num(s.mean),
            num(s.sd),
            num(s.ci95[0]),
            num(s.ci95[1]),
            num(s.min),
            num(s.median),
            num(s.max)
        ));
    }
    std::fs::write(out.join("summary.csv"), sum)?;
    #[derive(Serialize)]
    struct Summary<'a> {
        scenario: &'a str,
        runs: usize,
        seeds: [u64; 2],
        moes: &'a [Stat],
    }
    let seeds = [
        runs.first().map_or(0, |r| r.seed),
        runs.last().map_or(0, |r| r.seed),
    ];
    std::fs::write(
        out.join("summary.json"),
        serde_json::to_string_pretty(&Summary {
            scenario,
            runs: runs.len(),
            seeds,
            moes: stats,
        })? + "\n",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statistics_by_hand() {
        let s = stat("x".into(), &mut [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0]);
        assert_eq!(s.n, 8);
        assert!((s.mean - 5.0).abs() < 1e-12);
        // Sample variance: sum of squares 32 / 7.
        assert!((s.sd - (32.0f64 / 7.0).sqrt()).abs() < 1e-12);
        let half = 1.96 * s.sd / 8f64.sqrt();
        assert!((s.ci95[0] - (5.0 - half)).abs() < 1e-12);
        assert_eq!((s.min, s.median, s.max), (2.0, 4.5, 9.0));
        assert_eq!(s.histogram.iter().sum::<usize>(), 8);
        assert_eq!(s.histogram[0], 1);
        assert_eq!(s.histogram[9], 1);
        let empty = stat("y".into(), &mut []);
        assert_eq!(empty.n, 0);
        assert!(empty.mean.is_nan());
        let one = stat("z".into(), &mut [3.0]);
        assert_eq!((one.sd, one.ci95), (0.0, [3.0, 3.0]));
    }

    #[test]
    fn results_do_not_depend_on_the_thread_count() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/scenarios/phase2-surface-action.json");
        let scenario = Scenario::load(&path).unwrap();
        let open = || sim_db::embedded::open_platform_db().map_err(anyhow::Error::from);
        let seeds = [1, 2, 3, 4];
        let end = Tick(1200);
        let one = run(&scenario, &seeds, end, 1, &open, None).unwrap();
        let four = run(&scenario, &seeds, end, 4, &open, None).unwrap();
        assert_eq!(one, four);
        assert_eq!(one.iter().map(|r| r.seed).collect::<Vec<_>>(), seeds);
        assert!(one[0].moes.contains_key("cargo_delivered_t"));
        let stats = summarise(&one);
        assert_eq!(stats.len(), one[0].moes.len());
    }

    #[test]
    fn writes_a_readable_parquet_file() {
        use parquet::file::reader::{FileReader, SerializedFileReader};
        let mut moes = BTreeMap::new();
        moes.insert("Blue.losses".to_owned(), Some(1.0));
        moes.insert("Blue.weapons_per_kill".to_owned(), None);
        let runs = vec![
            RunMoes {
                seed: 7,
                final_hash: "ab".into(),
                moes: moes.clone(),
            },
            RunMoes {
                seed: 8,
                final_hash: "cd".into(),
                moes,
            },
        ];
        let dir = std::env::temp_dir().join(format!("nws-mc-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("runs.parquet");
        write_parquet(&path, &runs).unwrap();
        let reader = SerializedFileReader::new(std::fs::File::open(&path).unwrap()).unwrap();
        let meta = reader.metadata();
        assert_eq!(meta.file_metadata().num_rows(), 2);
        let cols: Vec<String> = meta
            .file_metadata()
            .schema_descr()
            .columns()
            .iter()
            .map(|c| c.name().to_owned())
            .collect();
        assert_eq!(
            cols,
            ["seed", "final_hash", "Blue.losses", "Blue.weapons_per_kill"]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
