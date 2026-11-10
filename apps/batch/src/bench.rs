//! Throughput benchmark against the master-prompt §12 performance budgets:
//! 1,000 active entities at ≥ 60× real time, and 10,000 entities at ≥ 1× real time.
//!
//! The workload is N units of a radar-equipped database class, split evenly between two sides,
//! placed at seeded random positions in a 20° × 20° sea box with random courses at cruise speed.
//! Every tick runs movement, and every 5th tick runs radar detection across all pairs.

use anyhow::{bail, Result};
use serde::Serialize;
use sim_core::Tick;
use sim_scenario::synthetic::synthetic_scenario;

#[derive(Debug, Serialize)]
pub struct BenchResult {
    pub units: usize,
    pub platform: String,
    pub ticks: u64,
    pub wall_s: f64,
    /// Game seconds simulated per wall-clock second.
    pub ratio: f64,
    pub min_ratio: Option<f64>,
    pub passed: bool,
    pub detection_draws: usize,
    pub final_hash: String,
}

pub fn run(
    db: &sim_db::PlatformDb,
    units: usize,
    ticks: u64,
    platform: &str,
    min_ratio: Option<f64>,
) -> Result<BenchResult> {
    if units == 0 || ticks == 0 {
        bail!("units and ticks must be > 0");
    }
    let scenario = synthetic_scenario(units, platform, 1);
    let mut sim = scenario.build(None, db)?;
    sim.step(); // spawn tick, excluded from timing

    // Wall-clock measurement is the point of a benchmark; it never feeds back into the sim.
    #[allow(clippy::disallowed_methods)]
    let start = std::time::Instant::now();
    sim.run_until(Tick(ticks + 1));
    let wall_s = start.elapsed().as_secs_f64();

    #[allow(clippy::cast_precision_loss)]
    let ratio = ticks as f64 * sim_core::TICK_SECONDS / wall_s.max(1e-9);
    let detection_draws = sim
        .events()
        .iter()
        .filter(|e| matches!(e.kind, sim_core::SimEventKind::DetectionAttempt { .. }))
        .count();
    Ok(BenchResult {
        units,
        platform: platform.to_owned(),
        ticks,
        wall_s,
        ratio,
        min_ratio,
        passed: min_ratio.is_none_or(|m| ratio >= m),
        detection_draws,
        final_hash: format!("{:016x}", sim.state_hash()),
    })
}
