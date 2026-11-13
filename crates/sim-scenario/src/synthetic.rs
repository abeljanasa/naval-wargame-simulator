//! Synthetic scenarios for benchmarks (master prompt §12 performance budgets).
//!
//! N copies of one database class, alternating between two sides, at seeded random positions in a
//! 20° × 20° sea box (45°S–25°S, 140°E–160°E), on random courses at the class cruise speed.
//! The generator is fixed: changing it changes every benchmark hash.

use crate::{Scenario, ScenarioUnit, SideDef, SCENARIO_FORMAT, SCENARIO_FORMAT_VERSION};
use rand_chacha::rand_core::{RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;
use sim_core::{Doctrine, GeoPoint, SideId, UnitId};

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
                group: None,
                doctrine: Doctrine::default(),
                jammer: None,
                mission: None,
                reserve: false,
                fuel_level: None,
            }
        })
        .collect();
    Scenario {
        format: SCENARIO_FORMAT.to_owned(),
        format_version: SCENARIO_FORMAT_VERSION,
        name: format!("benchmark: {platform}"),
        description: String::new(),
        seed,
        duration_s: 3600,
        sides: vec![
            SideDef {
                id: SideId(1),
                name: "A".into(),
                postures: vec![],
                doctrine: Doctrine::default(),
            },
            SideDef {
                id: SideId(2),
                name: "B".into(),
                postures: vec![],
                doctrine: Doctrine::default(),
            },
        ],
        groups: vec![],
        minefields: vec![],
        environment: Default::default(),
        units,
        events: vec![],
        injects: vec![],
        seats: vec![],
        reference_points: vec![],
        facilities: vec![],
        zones: vec![],
        action_groups: vec![],
        lanes: vec![],
        weather_zones: vec![],
        sound_profiles: vec![],
        fuel: false,
    }
}
