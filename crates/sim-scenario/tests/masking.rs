//! Terrain masking in the running engine (docs/models/terrain-masking.md): with terrain packs,
//! land between a sensor and its target blocks radar and optical detection, while targets high
//! enough to show over it are still seen, and low aircraft over land compete with land clutter.

#![allow(clippy::unwrap_used)] // test harness

use sim_core::{SimEventKind, Simulation, Tick, UnitId};
use sim_geo::terrain::{encode, PackHeader, Terrain, TerrainPack};
use sim_scenario::Scenario;
use std::sync::Arc;

/// Open central Pacific (the coastline has no land here), 0.01° cells over 1°S–1°N,
/// 171°W–169°W: 3,000 m deep, with an island of `peak` m along 170°W (169.98–170.02°W).
fn island(peak: i16) -> Arc<Terrain> {
    let (rows, cols) = (201u32, 201u32);
    let heights: Vec<i16> = (0..rows * cols)
        .map(|i| {
            let lon = -171.0 + 0.01 * f64::from(i % cols);
            if (lon + 170.0).abs() <= 0.02 {
                peak
            } else {
                -3000
            }
        })
        .collect();
    let header = PackHeader {
        name: "island".into(),
        source: "test".into(),
        credit: "test".into(),
        lat0: -36_000,
        lon0: -171 * 36_000,
        cell: 360,
        rows,
        cols,
        min_m: -3000,
        max_m: peak,
    };
    let pack = TerrainPack::decode(&encode(&header, &heights).unwrap()).unwrap();
    Arc::new(Terrain::new(vec![Arc::new(pack)]).unwrap())
}

/// A frigate 12 km west of the island looking east at `target` (platform, altitude) 12 km east.
fn scenario(target: &str, altitude_m: f64) -> Scenario {
    Scenario::from_json(
        &serde_json::json!({
            "format": "nws-scenario", "format_version": 4, "name": "masking", "seed": 7,
            "duration_s": 600,
            "environment": {"sea_state": 1, "start_unix_s": 1794268800},
            "sides": [
                {"id": 1, "name": "Blue", "doctrine": {"emcon": "active"}},
                {"id": 2, "name": "Red", "doctrine": {"emcon": "active"}}
            ],
            "units": [
                {"id": 1, "name": "Watcher", "side": 1, "platform": "GEN-FFG",
                 "position": {"lat_deg": 0.0, "lon_deg": -170.11}, "heading_deg": 90.0,
                 "speed_mps": 0.0},
                {"id": 2, "name": "Target", "side": 2, "platform": target,
                 "position": {"lat_deg": 0.0, "lon_deg": -169.89}, "altitude_m": altitude_m,
                 "heading_deg": 270.0, "speed_mps": 0.0}
            ]
        })
        .to_string(),
    )
    .unwrap()
}

fn run(s: &Scenario, terrain: Option<Arc<Terrain>>) -> Simulation {
    let db = sim_db::embedded::open_platform_db().unwrap();
    let mut sim = s.build(None, &db).unwrap();
    sim.set_terrain(terrain);
    sim.run_until(Tick(300));
    sim
}

/// The watcher's radar and optical detection attempts on the target.
fn attempts(sim: &Simulation) -> usize {
    sim.events()
        .iter()
        .filter(|e| {
            matches!(&e.kind, SimEventKind::DetectionAttempt { observer, target, .. }
                if *observer == UnitId(1) && *target == UnitId(2))
                || matches!(&e.kind, SimEventKind::OpticalAttempt { observer, target, .. }
                if *observer == UnitId(1) && *target == UnitId(2))
        })
        .count()
}

/// Clutter loss of the watcher's radar attempts on the target.
fn clutter(sim: &Simulation) -> Vec<f64> {
    sim.events()
        .iter()
        .filter_map(|e| match &e.kind {
            SimEventKind::DetectionAttempt {
                observer,
                target,
                clutter_db,
                ..
            } if *observer == UnitId(1) && *target == UnitId(2) => Some(*clutter_db),
            _ => None,
        })
        .collect()
}

#[test]
fn an_island_hides_a_ship_behind_it() {
    let s = scenario("GEN-FFG", 0.0);
    // Without terrain the frigate's radar and lookouts work on the target 24 km away.
    let open = run(&s, None);
    assert!(attempts(&open) > 0, "no attempts without terrain");
    // A 300 m island in between: nothing is even tried.
    let masked = run(&s, Some(island(300)));
    assert_eq!(attempts(&masked), 0);
    // A 1 m reef does not hide a ship's superstructure.
    let reef = run(&s, Some(island(1)));
    assert_eq!(attempts(&reef), attempts(&open));
}

#[test]
fn an_aircraft_above_the_island_is_still_seen() {
    let s = scenario("GEN-MPA-TP", 3000.0);
    let sim = run(&s, Some(island(300)));
    assert!(
        attempts(&sim) > 0,
        "an aircraft at 3,000 m shows over a 300 m island"
    );
}

#[test]
fn a_low_aircraft_over_land_meets_land_clutter() {
    // A helicopter at 440 m over the 300 m island (140 m above it, under the 150 m clutter
    // ceiling) competes with land clutter. Over the sea at 440 m it is above the ceiling.
    let mut s = scenario("GEN-HELO-UTIL", 440.0);
    s.units[1].position.lon_deg = -170.0;
    let over_land = clutter(&run(&s, Some(island(300))));
    let over_sea = clutter(&run(&s, None));
    assert!(!over_land.is_empty() && !over_sea.is_empty());
    assert_eq!(over_sea[0], 0.0);
    assert!(over_land[0] > 0.0, "land clutter {}", over_land[0]);
}

#[test]
fn an_island_cuts_a_line_of_sight_radio_link() {
    // A command post 17 km west of the island and the frigate 17 km east of it: within radio
    // line of sight over the sea, but the 300 m island stands between them.
    let s = Scenario::from_json(
        &serde_json::json!({
            "format": "nws-scenario", "format_version": 4, "name": "comms", "seed": 7,
            "duration_s": 600,
            "environment": {"sea_state": 1, "start_unix_s": 1794268800},
            "sides": [{"id": 1, "name": "Blue"}, {"id": 2, "name": "Red"}],
            "units": [
                {"id": 1, "name": "Frigate", "side": 1, "platform": "GEN-FFG",
                 "position": {"lat_deg": 0.0, "lon_deg": -169.85}, "heading_deg": 0.0,
                 "speed_mps": 0.0}
            ],
            "facilities": [
                {"id": 1, "name": "HQ", "side": 1, "kind": "command_post",
                 "position": {"lat_deg": 0.0, "lon_deg": -170.15}, "radius_m": 1000,
                 "range_m": 300000, "line_of_sight": true}
            ]
        })
        .to_string(),
    )
    .unwrap();
    let status = |terrain: Option<Arc<Terrain>>| {
        let mut sim = run(&s, terrain);
        let snap = sim.snapshot(sim_core::Viewer::Side {
            side: sim_core::SideId(1),
        });
        snap.comms
            .iter()
            .find(|c| c.unit == UnitId(1))
            .map(|c| c.status)
            .unwrap()
    };
    assert_eq!(status(None), sim_core::LinkStatus::Up);
    assert_ne!(status(Some(island(300))), sim_core::LinkStatus::Up);
}
