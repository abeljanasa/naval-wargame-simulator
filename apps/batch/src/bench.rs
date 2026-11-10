//! Throughput benchmark against the master-prompt §12 performance budgets:
//! 1,000 active entities at ≥ 60× real time, and 10,000 entities at ≥ 1× real time.
//!
//! The workload is N units of a radar-equipped database class, split evenly between two sides,
//! placed at seeded random positions in a 20° × 20° sea box with random courses at cruise speed.
//! Every tick runs movement, and every 5th tick runs radar detection across all pairs.

use anyhow::{bail, Result};
use rand_chacha::rand_core::{RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::Serialize;
use sim_core::{GeoPoint, SideId, Tick, UnitId};
use sim_scenario::{Scenario, ScenarioUnit, SideDef};

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

fn unit_f64(rng: &mut ChaCha8Rng) -> f64 {
    #[allow(clippy::cast_precision_loss)] // exact: < 2^53
    let v = (rng.next_u64() >> 11) as f64 / (1u64 << 53) as f64;
    v
}

/// A synthetic scenario with `units` copies of `platform`.
#[must_use]
pub fn synthetic_scenario(units: usize, platform: &str, seed: u64) -> Scenario {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let units = (0..units)
        .map(|i| {
            let id = i as u64 + 1;
            ScenarioUnit {
                id: UnitId(id),
                name: format!("{platform} #{id}"),
                side: SideId(if i % 2 == 0 { 1 } else { 2 }),
                platform: Some(platform.to_owned()),
                domain: None,
                position: GeoPoint::new(
                    -45.0 + 20.0 * unit_f64(&mut rng),
                    140.0 + 20.0 * unit_f64(&mut rng),
                ),
                altitude_m: 0.0,
                heading_deg: 360.0 * unit_f64(&mut rng),
                speed_mps: None,
                max_speed_mps: None,
                accel_mps2: None,
                route: vec![],
            }
        })
        .collect();
    Scenario {
        format: sim_scenario::SCENARIO_FORMAT.to_owned(),
        format_version: sim_scenario::SCENARIO_FORMAT_VERSION,
        name: format!("benchmark: {platform}"),
        description: String::new(),
        seed,
        duration_s: 3600,
        sides: vec![
            SideDef {
                id: SideId(1),
                name: "A".into(),
            },
            SideDef {
                id: SideId(2),
                name: "B".into(),
            },
        ],
        units,
    }
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
