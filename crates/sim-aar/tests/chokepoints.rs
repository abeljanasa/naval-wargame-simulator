//! Choke points with lanes and depths (docs/models/choke-points.md): a transit through a choke
//! point with a busy lane is assessed and reported; a choke point takes its depths from the chart.

#![allow(clippy::unwrap_used)] // test harness

use sim_aar::AarReport;
use sim_core::{SideId, SimEventKind, Tick, TransitRisk, UnitId};
use sim_geo::terrain::{encode, PackHeader, Terrain, TerrainPack};
use sim_scenario::Scenario;
use std::sync::Arc;

/// A strait along the equator at 170°W: land north of 0.1°N and south of 0.1°S, a channel
/// 60 m deep at its centre shoaling to 10 m at its edges (0.01° cells, 1°S–1°N, 171–169°W).
fn strait() -> Arc<Terrain> {
    let (rows, cols) = (201u32, 201u32);
    let heights: Vec<i16> = (0..rows * cols)
        .map(|i| {
            let lat = -1.0 + 0.01 * f64::from(i / cols);
            if lat.abs() > 0.1 + 1e-9 {
                30
            } else {
                -(10.0 + 50.0 * (1.0 - lat.abs() / 0.1)).round() as i16
            }
        })
        .collect();
    let header = PackHeader {
        name: "strait".into(),
        source: "test".into(),
        credit: "test".into(),
        lat0: -36_000,
        lon0: -171 * 36_000,
        cell: 360,
        rows,
        cols,
        min_m: -60,
        max_m: 30,
    };
    let pack = TerrainPack::decode(&encode(&header, &heights).unwrap()).unwrap();
    Arc::new(Terrain::new(vec![Arc::new(pack)]).unwrap())
}

fn scenario(traffic: serde_json::Value, source: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "format": "nws-scenario", "format_version": 4, "name": "strait", "seed": 3,
        "duration_s": 3600,
        "environment": {"sea_state": 1, "start_unix_s": 1794268800},
        "sides": [{"id": 1, "name": "Blue"}, {"id": 2, "name": "Red"}],
        "units": [
            {"id": 1, "name": "Runner", "side": 2, "platform": "GEN-FFL",
             "position": {"lat_deg": 0.0, "lon_deg": -170.2}, "heading_deg": 90.0,
             "speed_mps": 10.0, "route": [{"lat_deg": 0.0, "lon_deg": -169.7}]}
        ],
        "zones": [
            {"id": 1, "name": "Strait sense", "side": 1, "kind": "sense",
             "choke_point": "Test Strait",
             "area": {"shape": "polygon", "points": [
                {"lat_deg": -0.1, "lon_deg": -170.05}, {"lat_deg": -0.1, "lon_deg": -169.95},
                {"lat_deg": 0.1, "lon_deg": -169.95}, {"lat_deg": 0.1, "lon_deg": -170.05}]}}
        ],
        "lanes": [
            {"id": 1, "name": "Main channel", "width_m": 4000,
             "ships_per_day": traffic, "source": source,
             "points": [{"lat_deg": 0.0, "lon_deg": -170.5}, {"lat_deg": 0.0, "lon_deg": -169.5}]}
        ]
    })
}

#[test]
fn a_transit_through_a_busy_lane_is_assessed_and_reported() {
    let s = Scenario::from_json(&scenario(150.0.into(), "test traffic set".into()).to_string())
        .unwrap();
    let db = sim_db::embedded::open_platform_db().unwrap();
    let mut sim = s.build(None, &db).unwrap();
    sim.set_terrain(Some(strait()));
    sim.run_until(Tick(3600));
    let assessed: Vec<_> = sim
        .events()
        .iter()
        .filter_map(|e| match &e.kind {
            SimEventKind::TransitAssessed {
                choke_point,
                unit,
                lane,
                traffic_per_day,
                depth_m,
                risk,
                ..
            } => Some((
                choke_point.clone(),
                *unit,
                lane.clone(),
                *traffic_per_day,
                *depth_m,
                *risk,
            )),
            _ => None,
        })
        .collect();
    assert_eq!(assessed.len(), 1, "{assessed:?}");
    let (choke, unit, lane, traffic, depth, risk) = assessed[0].clone();
    assert_eq!((choke.as_str(), unit), ("Test Strait", UnitId(1)));
    assert_eq!(lane.as_deref(), Some("Main channel"));
    assert_eq!(traffic, 150.0);
    assert!(depth.is_some_and(|d| d > 50.0), "{depth:?}");
    assert_eq!(risk, TransitRisk::High, "150 ships a day");

    // The AAR lists it in its transit table and its timeline.
    let sides = [(SideId(1), "Blue".into()), (SideId(2), "Red".into())];
    let r = AarReport::build(&sim.command_log(), sim.events(), &sides, sim.tick());
    assert_eq!(r.transits.len(), 1);
    assert_eq!(r.transits[0].lane.as_deref(), Some("Main channel"));
    let csv = r.csv(sim_aar::AarTable::Transits);
    assert!(
        csv.contains("Test Strait") && csv.contains(",high"),
        "{csv}"
    );
    assert!(r.timeline.iter().any(|t| t.text.contains("high risk")));

    // The choke point's water: 10–60 m, deepest mid-channel, the section north–south.
    let c = &sim.choke_depths()[0];
    assert_eq!(c.choke_point, "Test Strait");
    assert!((c.least_m.unwrap() - 10.0).abs() < 6.0, "{:?}", c.least_m);
    assert!(
        (c.deepest_m.unwrap() - 60.0).abs() < 6.0,
        "{:?}",
        c.deepest_m
    );
    assert_eq!(c.lanes, vec![1]);
    assert_eq!(c.traffic_per_day, 150.0);
    assert!((c.section_from.lon_deg - c.section_to.lon_deg).abs() < 1e-6);
}

#[test]
fn without_lanes_or_terrain_nothing_is_assessed() {
    let mut j = scenario(serde_json::Value::Null, serde_json::Value::Null);
    j["lanes"] = serde_json::json!([]);
    let s = Scenario::from_json(&j.to_string()).unwrap();
    let db = sim_db::embedded::open_platform_db().unwrap();
    let mut sim = s.build(None, &db).unwrap();
    sim.run_until(Tick(3600));
    assert!(sim
        .events()
        .iter()
        .any(|e| matches!(e.kind, SimEventKind::ChokePointTransit { .. })));
    assert!(!sim
        .events()
        .iter()
        .any(|e| matches!(e.kind, SimEventKind::TransitAssessed { .. })));
}

#[test]
fn traffic_without_a_source_is_refused() {
    let j = scenario(150.0.into(), serde_json::Value::Null);
    let s = Scenario::from_json(&j.to_string()).unwrap();
    let db = sim_db::embedded::open_platform_db().unwrap();
    let err = match s.build(None, &db) {
        Ok(mut sim) => {
            sim.run_until(Tick(1));
            sim.events()
                .iter()
                .find_map(|e| match &e.kind {
                    SimEventKind::CommandRejected { reason, .. } => Some(reason.clone()),
                    _ => None,
                })
                .unwrap_or_default()
        }
        Err(e) => e.to_string(),
    };
    assert!(err.contains("without a source"), "{err}");
}
