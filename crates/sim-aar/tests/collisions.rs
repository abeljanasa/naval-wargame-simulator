//! Collisions in the AAR (docs/models/collisions.md): the timeline says who collided and how
//! hard, and a hull sunk by a collision is lost to it, not to the last weapon that hit it.

#![allow(clippy::unwrap_used)] // test harness

use sim_aar::AarReport;
use sim_core::{SideId, Tick, UnitId};
use sim_scenario::Scenario;

#[test]
fn a_boat_run_down_by_a_carrier_is_lost_to_the_collision() {
    let s = Scenario::from_json(
        &serde_json::json!({
            "format": "nws-scenario", "format_version": 4, "name": "ramming", "seed": 9,
            "duration_s": 3600,
            "environment": {"sea_state": 2, "start_unix_s": 1794301200},
            "sides": [
                {"id": 1, "name": "Blue", "doctrine": {"wcs": "hold", "emcon": "silent"}},
                {"id": 2, "name": "Red", "doctrine": {"wcs": "hold", "emcon": "silent"}}
            ],
            "units": [
                {"id": 1, "name": "Carrier", "side": 1, "platform": "GEN-CV",
                 "position": {"lat_deg": 0.0, "lon_deg": -170.05}, "heading_deg": 90.0,
                 "speed_mps": 15.0, "route": [{"lat_deg": 0.0, "lon_deg": -169.9}]},
                {"id": 2, "name": "Boat", "side": 2, "platform": "GEN-SSK",
                 "position": {"lat_deg": 0.0, "lon_deg": -170.0}, "altitude_m": -12.0,
                 "heading_deg": 0.0, "speed_mps": 0.0}
            ]
        })
        .to_string(),
    )
    .unwrap();
    let mut sim = s
        .build(None, &sim_db::embedded::open_platform_db().unwrap())
        .unwrap();
    sim.run_until(Tick(900));
    let sides = [(SideId(1), "Blue".into()), (SideId(2), "Red".into())];
    let r = AarReport::build(&sim.command_log(), sim.events(), &sides, sim.tick());
    let line = r
        .timeline
        .iter()
        .find(|t| t.text.contains("collided"))
        .expect("the collision is on the timeline");
    assert!(
        line.text.starts_with("Carrier and Boat collided at 29 kn"),
        "{}",
        line.text
    );
    let loss = r
        .losses
        .iter()
        .find(|l| l.unit == UnitId(2))
        .expect("the boat is lost");
    assert_eq!(loss.cause, "collision with Carrier");
    assert_eq!(loss.by_side, None);
    assert!(
        r.losses.iter().all(|l| l.unit != UnitId(1)),
        "the carrier survives"
    );
}
