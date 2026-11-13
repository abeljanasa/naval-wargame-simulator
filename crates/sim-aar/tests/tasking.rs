//! Sense-zone tasking in the AAR (docs/models/sensor-tasking.md): each decision is a timeline
//! line for the zone's side, naming the zone, the unit and the coverage before and after.

#![allow(clippy::unwrap_used)] // test harness

use sim_aar::{AarReport, TimelineKind};
use sim_core::{SideId, Tick};
use sim_scenario::Scenario;

#[test]
fn tasking_and_shortfall_are_on_the_timeline() {
    let zone = |id: u32, name: &str, lon: f64| {
        serde_json::json!({"id": id, "name": name, "side": 1, "kind": "sense",
            "area": {"shape": "circle", "centre": {"lat_deg": 0.0, "lon_deg": lon},
                     "radius_m": 25000},
            "sensors": ["radar"], "coverage_goal": 0.6})
    };
    let s = Scenario::from_json(
        &serde_json::json!({
            "format": "nws-scenario", "format_version": 6, "name": "tasking", "seed": 8,
            "duration_s": 3600,
            "sides": [{"id": 1, "name": "Blue", "doctrine": {"emcon": "active"}}],
            "zones": [zone(1, "Outer screen", -170.0), zone(2, "Far screen", -160.0)],
            "units": [{"id": 1, "name": "Picket", "side": 1, "platform": "GEN-FFG",
                "position": {"lat_deg": 0.0, "lon_deg": -170.5}, "speed_mps": 0.0}]
        })
        .to_string(),
    )
    .unwrap();
    let mut sim = s
        .build(None, &sim_db::embedded::open_platform_db().unwrap())
        .unwrap();
    sim.run_until(Tick(10));
    let sides = [(SideId(1), "Blue".into())];
    let r = AarReport::build(&sim.command_log(), sim.events(), &sides, sim.tick());
    let lines: Vec<&str> = r
        .timeline
        .iter()
        .filter(|t| t.kind == TimelineKind::Tasking && t.side == Some(SideId(1)))
        .map(|t| t.text.as_str())
        .collect();
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("Picket tasked to watch Outer screen (coverage")),
        "{lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("Far screen short of its coverage goal")),
        "the only ship went to the first zone: {lines:?}"
    );
}
