//! Track course estimates in the running engine (docs/models/tracks.md): a side estimates a
//! contact's course from the movement of its own track, never from the target's true heading.

#![allow(clippy::unwrap_used)] // test harness

use sim_core::{SideId, Simulation, Tick, Viewer};
use sim_scenario::Scenario;

/// A frigate (side 1) with radar; a corvette (side 2) 20 km east steaming `course` at 8 m/s.
fn scenario(course: f64) -> Scenario {
    Scenario::from_json(
        &serde_json::json!({
            "format": "nws-scenario", "format_version": 4, "name": "tracks", "seed": 3,
            "duration_s": 1800,
            "environment": {"sea_state": 2, "start_unix_s": 1794301200},
            "sides": [
                {"id": 1, "name": "Blue", "doctrine": {"emcon": "active", "wcs": "hold"}},
                {"id": 2, "name": "Red", "doctrine": {"emcon": "silent", "wcs": "hold"}}
            ],
            "units": [
                {"id": 1, "name": "Frigate", "side": 1, "platform": "GEN-FFG",
                 "position": {"lat_deg": 0.0, "lon_deg": -170.0}, "heading_deg": 0.0,
                 "speed_mps": 0.0},
                {"id": 2, "name": "Corvette", "side": 2, "platform": "GEN-FFL",
                 "position": {"lat_deg": 0.0, "lon_deg": -169.82}, "heading_deg": course,
                 "speed_mps": 8.0}
            ]
        })
        .to_string(),
    )
    .unwrap()
}

fn build(s: &Scenario) -> Simulation {
    let db = sim_db::embedded::open_platform_db().unwrap();
    s.build(None, &db).unwrap()
}

/// The smallest angle between two bearings, degrees.
fn off(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(360.0);
    d.min(360.0 - d)
}

#[test]
fn a_new_contact_has_no_course_then_gets_the_targets_course() {
    for course in [0.0, 135.0, 270.0] {
        let mut sim = build(&scenario(course));
        // Just acquired: no course yet, so the map shows it facing the screen.
        sim.run_until(Tick(10));
        let c = sim.snapshot(Viewer::Side { side: SideId(1) }).contacts;
        assert_eq!(c.len(), 1, "course {course}");
        assert_eq!(c[0].course_deg, None, "course {course}");
        // After ten minutes of radar updates the track's own movement gives the course.
        sim.run_until(Tick(600));
        let c = &sim.snapshot(Viewer::Side { side: SideId(1) }).contacts[0];
        let est = c.course_deg.expect("a course once the track has moved");
        assert!(
            off(est, course) < 15.0,
            "true {course}°, estimated {est:.1}°"
        );
        let speed = c.speed_mps.unwrap();
        assert!(
            (speed - 8.0).abs() < 2.5,
            "true 8 m/s, estimated {speed:.2}"
        );
    }
}

#[test]
fn a_stopped_target_has_no_course() {
    let mut s = scenario(90.0);
    s.units[1].speed_mps = Some(0.0);
    let mut sim = build(&s);
    sim.run_until(Tick(900));
    let c = &sim.snapshot(Viewer::Side { side: SideId(1) }).contacts[0];
    assert_eq!(c.course_deg, None);
}
