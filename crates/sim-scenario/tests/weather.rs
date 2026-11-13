//! Weather zones in the running engine (docs/models/weather.md): rain attenuates radar, sea
//! state sets clutter and slows small hulls, fog shortens optical range, low cloud hides
//! aircraft from the eye, and events change the weather.

#![allow(clippy::unwrap_used)] // test harness

use sim_core::{SimEventKind, Simulation, Tick, UnitId, Viewer};
use sim_scenario::Scenario;

/// A frigate watching a target 20 km east; `weather` (if any) a zone over both.
fn scenario(target: &str, altitude_m: f64, weather: serde_json::Value) -> Scenario {
    Scenario::from_json(
        &serde_json::json!({
            "format": "nws-scenario", "format_version": 4, "name": "weather", "seed": 11,
            "duration_s": 600,
            "environment": {"sea_state": 2, "start_unix_s": 1794301200},
            "weather_zones": weather,
            "sides": [
                {"id": 1, "name": "Blue", "doctrine": {"emcon": "active"}},
                {"id": 2, "name": "Red", "doctrine": {"emcon": "active"}}
            ],
            "units": [
                {"id": 1, "name": "Watcher", "side": 1, "platform": "GEN-FFG",
                 "position": {"lat_deg": 0.0, "lon_deg": -170.09}, "heading_deg": 90.0,
                 "speed_mps": 0.0},
                {"id": 2, "name": "Target", "side": 2, "platform": target,
                 "position": {"lat_deg": 0.0, "lon_deg": -169.91}, "altitude_m": altitude_m,
                 "heading_deg": 0.0, "speed_mps": 0.0}
            ]
        })
        .to_string(),
    )
    .unwrap()
}

fn zone(sea_state: u8, visibility_km: f64, rain: f64, cloud: Option<f64>) -> serde_json::Value {
    serde_json::json!([{
        "id": 1, "name": "Squall line", "sea_state": sea_state, "wind_kn": 40,
        "wind_from_deg": 220, "visibility_km": visibility_km, "rain_mm_h": rain,
        "cloud_base_m": cloud,
        "area": {"shape": "circle", "centre": {"lat_deg": 0.0, "lon_deg": -170.0},
                 "radius_m": 30000}
    }])
}

fn run(s: &Scenario, ticks: u64) -> Simulation {
    let db = sim_db::embedded::open_platform_db().unwrap();
    let mut sim = s.build(None, &db).unwrap();
    sim.run_until(Tick(ticks));
    sim
}

/// (clutter dB, weather dB) of the watcher's radar attempts on the target.
fn radar(sim: &Simulation) -> Vec<(f64, f64)> {
    sim.events()
        .iter()
        .filter_map(|e| match &e.kind {
            SimEventKind::DetectionAttempt {
                observer,
                target,
                clutter_db,
                weather_db,
                ..
            } if *observer == UnitId(1) && *target == UnitId(2) => Some((*clutter_db, *weather_db)),
            _ => None,
        })
        .collect()
}

/// R₅₀ of the watcher's optical attempts on the target.
fn optical(sim: &Simulation) -> Vec<f64> {
    sim.events()
        .iter()
        .filter_map(|e| match &e.kind {
            SimEventKind::OpticalAttempt {
                observer,
                target,
                r50_m,
                ..
            } if *observer == UnitId(1) && *target == UnitId(2) => Some(*r50_m),
            _ => None,
        })
        .collect()
}

#[test]
fn rain_and_a_rough_sea_degrade_radar_inside_the_zone() {
    let clear = radar(&run(&scenario("GEN-FFG", 0.0, serde_json::json!([])), 120));
    let squall = radar(&run(
        &scenario("GEN-FFG", 0.0, zone(6, 40.0, 16.0, None)),
        120,
    ));
    assert!(!clear.is_empty() && !squall.is_empty());
    assert_eq!(clear[0].1, 0.0);
    // 20 km in 16 mm/h rain, two ways: about 16 dB.
    assert!(
        (12.0..20.0).contains(&squall[0].1),
        "rain loss {}",
        squall[0].1
    );
    // Sea state 6 in the zone against 2 outside: more sea clutter.
    assert!(squall[0].0 > clear[0].0);
}

#[test]
fn fog_shortens_optical_range() {
    let clear = optical(&run(&scenario("GEN-FFG", 0.0, serde_json::json!([])), 120));
    let fog = optical(&run(
        &scenario("GEN-FFG", 0.0, zone(2, 1.0, 0.0, None)),
        120,
    ));
    assert!(!clear.is_empty() && !fog.is_empty());
    assert!(fog[0] < clear[0] / 3.0, "fog {} clear {}", fog[0], clear[0]);
}

#[test]
fn low_cloud_hides_an_aircraft_from_the_eye_but_not_from_radar() {
    let s = scenario("GEN-MPA-TP", 3000.0, zone(2, 40.0, 0.0, Some(600.0)));
    let sim = run(&s, 120);
    assert!(optical(&sim).is_empty(), "seen through the cloud");
    assert!(!radar(&sim).is_empty());
    let clear = run(&scenario("GEN-MPA-TP", 3000.0, serde_json::json!([])), 120);
    assert!(!optical(&clear).is_empty());
}

#[test]
fn heavy_seas_slow_a_small_hull() {
    // A 50 m patrol boat (weather limit 5) ordered to full speed in sea state 6.
    let mut s = scenario("GEN-PGG", 0.0, zone(6, 40.0, 0.0, None));
    s.units[1].speed_mps = Some(20.0);
    s.units[1].route = vec![sim_core::GeoPoint::new(0.0, -169.5)];
    let mut sim = run(&s, 600);
    let u = sim
        .snapshot(Viewer::WhiteCell)
        .units
        .into_iter()
        .find(|u| u.id == UnitId(2))
        .unwrap();
    assert!(
        u.speed_mps <= 0.6 * u.max_speed_mps + 1e-9,
        "{} of {}",
        u.speed_mps,
        u.max_speed_mps
    );
    // Without the zone (sea state 2) it makes its full ordered speed.
    s.weather_zones.clear();
    let mut calm = run(&s, 600);
    let v = calm
        .snapshot(Viewer::WhiteCell)
        .units
        .into_iter()
        .find(|u| u.id == UnitId(2))
        .unwrap();
    assert!(v.speed_mps > 0.6 * v.max_speed_mps);
}

#[test]
fn an_event_brings_the_weather_in() {
    let mut j: serde_json::Value = serde_json::from_str(
        &serde_json::to_string(&scenario("GEN-FFG", 0.0, serde_json::json!([]))).unwrap(),
    )
    .unwrap();
    j["events"] = serde_json::json!([{
        "id": "squall", "name": "Exercise control", "trigger": {"type": "time", "at_s": 60},
        "actions": [{"type": "set_weather", "zones": zone(6, 40.0, 16.0, None)}]
    }]);
    let s = Scenario::from_json(&j.to_string()).unwrap();
    let sim = run(&s, 180);
    let r = radar(&sim);
    assert!(
        r.first().is_some_and(|x| x.1 == 0.0),
        "dry before the event"
    );
    assert!(r.last().is_some_and(|x| x.1 > 10.0), "rain after it");
    let mut sim = sim;
    assert_eq!(sim.snapshot(Viewer::WhiteCell).weather_zones.len(), 1);
}
