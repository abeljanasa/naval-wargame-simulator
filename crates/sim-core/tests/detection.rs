//! Gate 1 radar detection: duel tests against hand calculation, geometry cut-offs, contacts,
//! fog of war and save/resume. Model doc: docs/models/radar-detection-gate1.md.

#![allow(clippy::unwrap_used, clippy::cast_precision_loss)] // test harness

use sim_core::*;
use sim_geo::direct;

const R50_REF: f64 = 120_000.0;

fn radar() -> SensorSpec {
    SensorSpec {
        sensor_id: "TEST-RADAR".into(),
        kind: SensorKind::Radar,
        mount_height_m: 25.0,
        min_range_m: 0.0,
        max_range_m: 250_000.0,
        ref_range_m: Some(R50_REF),
        ref_rcs_m2: Some(1.0),
        max_target_altitude_m: 25_000.0,
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn(
    id: u64,
    side: u16,
    domain: Domain,
    at: GeoPoint,
    altitude_m: f64,
    heading_deg: f64,
    speed_mps: f64,
    sensors: Vec<SensorSpec>,
    rcs: f64,
) -> Command {
    Command {
        tick: Tick(0),
        issuer: Issuer::Scenario,
        side: None,
        payload: CommandPayload::SpawnUnit(UnitSpawn {
            id: UnitId(id),
            name: format!("U{id}"),
            side: SideId(side),
            domain,
            position: at,
            altitude_m,
            heading_deg,
            speed_mps,
            max_speed_mps: speed_mps,
            accel_mps2: if speed_mps > 0.0 { 1.0 } else { 0.0 },
            route: vec![],
            platform: None,
            sensors,
            rcs_m2: Some(rcs),
            max_turn_rate_dps: None,
            min_turn_radius_m: None,
            group: None,
            doctrine: Doctrine::default(),
        }),
    }
}

const ORIGIN: GeoPoint = GeoPoint {
    lat_deg: 0.0,
    lon_deg: 0.0,
};

/// Observer (side 1, radar) at the origin; stationary target (side 2) at `range_m` due east.
fn duel(seed: u64, target_domain: Domain, altitude: f64, range_m: f64, rcs: f64) -> Simulation {
    let mut sim = Simulation::new(seed);
    sim.submit(spawn(
        1,
        1,
        Domain::Surface,
        ORIGIN,
        0.0,
        0.0,
        0.0,
        vec![radar()],
        5000.0,
    ))
    .unwrap();
    let at = direct(ORIGIN, 90.0, range_m).point;
    sim.submit(spawn(
        2,
        2,
        target_domain,
        at,
        altitude,
        0.0,
        0.0,
        vec![],
        rcs,
    ))
    .unwrap();
    sim
}

fn attempts(sim: &Simulation) -> Vec<(f64, f64, bool)> {
    sim.events()
        .iter()
        .filter_map(|e| match &e.kind {
            SimEventKind::DetectionAttempt {
                pd, draw, detected, ..
            } => Some((*pd, *draw, *detected)),
            _ => None,
        })
        .collect()
}

/// Duel test: detections over N independent scans follow Binomial(N, Pd) with Pd from the
/// hand-calculated curve. Tolerance is 4 standard deviations.
fn check_binomial(range_factor: f64, expected_pd: f64, scans: u64) {
    // Air target at 10 km altitude: horizon 4.12·(√25 + √10000) = 432.6 km, far beyond R₅₀.
    let mut sim = duel(42, Domain::Air, 10_000.0, R50_REF * range_factor, 1.0);
    sim.run_ticks(scans * SENSOR_PERIOD_TICKS);
    let a = attempts(&sim);
    assert_eq!(a.len() as u64, scans, "one draw per scan");
    for (pd, draw, detected) in &a {
        assert!(
            (pd - expected_pd).abs() < 1e-6,
            "pd {pd} vs hand value {expected_pd}"
        );
        assert_eq!(*detected, draw < pd, "detected iff draw < pd");
    }
    let hits = a.iter().filter(|x| x.2).count() as f64;
    let n = scans as f64;
    let mean = n * expected_pd;
    let sd = (n * expected_pd * (1.0 - expected_pd)).sqrt();
    assert!(
        (hits - mean).abs() <= 4.0 * sd,
        "hits {hits}, expected {mean} ± {}",
        4.0 * sd
    );
}

#[test]
fn duel_at_r50_detects_half_the_time() {
    check_binomial(1.0, 0.5, 4000);
}

#[test]
fn duel_at_eight_tenths_r50_matches_hand_value() {
    // Pd = 1 / (1 + 0.8⁸) = 0.856329
    check_binomial(0.8, 1.0 / 1.167_772_16, 2000);
}

#[test]
fn duel_at_one_point_two_r50_matches_hand_value() {
    // Pd = 1 / (1 + 1.2⁸) = 0.188686
    check_binomial(1.2, 1.0 / 5.299_816_96, 2000);
}

#[test]
fn targets_beyond_the_radar_horizon_get_no_draw() {
    // Surface target: horizon 4.12·(√25 + √10) = 33.63 km. Put it at 40 km.
    let mut sim = duel(1, Domain::Surface, 0.0, 40_000.0, 5000.0);
    sim.run_ticks(600);
    assert!(attempts(&sim).is_empty());
    assert!(sim.snapshot(Viewer::WhiteCell).contacts.is_empty());
    // At 30 km it is inside the horizon and, with Pd ≈ 1, detected on the first scan.
    let mut sim = duel(1, Domain::Surface, 0.0, 30_000.0, 5000.0);
    sim.step();
    assert_eq!(attempts(&sim).len(), 1);
    assert_eq!(sim.snapshot(Viewer::WhiteCell).contacts.len(), 1);
}

#[test]
fn submerged_submarines_are_invisible_to_radar() {
    let mut sim = duel(1, Domain::Subsurface, -50.0, 3_000.0, 200.0);
    sim.run_ticks(600);
    assert!(attempts(&sim).is_empty());
}

#[test]
fn contacts_are_gained_shown_to_the_owning_side_only_and_lost_when_stale() {
    let mut sim = Simulation::new(7);
    sim.submit(spawn(
        1,
        1,
        Domain::Surface,
        ORIGIN,
        0.0,
        0.0,
        0.0,
        vec![radar()],
        5000.0,
    ))
    .unwrap();
    // Target 5 km east, steaming east at 50 m/s: leaves the 33.6 km horizon after about 9.5 min.
    let at = direct(ORIGIN, 90.0, 5_000.0).point;
    sim.submit(spawn(
        2,
        2,
        Domain::Surface,
        at,
        0.0,
        90.0,
        50.0,
        vec![],
        5000.0,
    ))
    .unwrap();

    sim.step();
    assert!(sim.events().iter().any(|e| matches!(
        e.kind,
        SimEventKind::ContactGained {
            side: SideId(1),
            number: 1,
            target: UnitId(2)
        }
    )));
    let blue = sim.snapshot(Viewer::Side { side: SideId(1) });
    assert_eq!(blue.units.len(), 1, "blue sees only its own unit");
    assert_eq!(blue.contacts.len(), 1);
    assert_eq!(blue.contacts[0].number, 1);
    assert!(sim
        .snapshot(Viewer::Side { side: SideId(2) })
        .contacts
        .is_empty());

    // Fog of war: the side stream must not carry the target's unit id.
    let json = serde_json::to_string(&blue).unwrap();
    assert!(!json.contains("\"target\""), "{json}");

    sim.run_ticks(1_200);
    assert!(sim.events().iter().any(|e| matches!(
        e.kind,
        SimEventKind::ContactLost {
            side: SideId(1),
            number: 1,
            ..
        }
    )));
    assert!(sim
        .snapshot(Viewer::Side { side: SideId(1) })
        .contacts
        .is_empty());
    // The contact is dropped one timeout after the last successful detection.
    let last_detect = sim
        .events()
        .iter()
        .filter(|e| {
            matches!(
                e.kind,
                SimEventKind::DetectionAttempt { detected: true, .. }
            )
        })
        .map(|e| e.tick.0)
        .max()
        .unwrap();
    let lost_at = sim
        .events()
        .iter()
        .find(|e| matches!(e.kind, SimEventKind::ContactLost { .. }))
        .map(|e| e.tick.0)
        .unwrap();
    assert!(lost_at > last_detect + CONTACT_TIMEOUT_TICKS);
    assert!(lost_at <= last_detect + CONTACT_TIMEOUT_TICKS + SENSOR_PERIOD_TICKS);
}

#[test]
fn save_and_resume_with_live_contacts_matches_uninterrupted_run() {
    let mut a = duel(99, Domain::Air, 10_000.0, R50_REF, 1.0);
    a.run_ticks(503);
    assert!(!a.snapshot(Viewer::WhiteCell).contacts.is_empty());
    let bytes = a.save().to_bytes().unwrap();
    let mut b = Simulation::from_save(SaveGame::from_bytes(&bytes).unwrap()).unwrap();
    assert_eq!(a.state_hash(), b.state_hash());
    a.run_ticks(2_000);
    b.run_ticks(2_000);
    assert_eq!(a.state_hash(), b.state_hash());
    assert_eq!(a.events(), b.events());
}

#[test]
fn detection_runs_are_deterministic_and_seed_dependent() {
    let run = |seed| {
        let mut s = duel(seed, Domain::Air, 10_000.0, R50_REF, 1.0);
        s.run_ticks(1_000);
        (s.state_hash(), attempts(&s))
    };
    assert_eq!(run(5), run(5));
    assert_ne!(run(5).1, run(6).1);
}

// ---- Tracks: measurement error, fusion, growth, classification (docs/models/tracks.md) ----

/// Stationary surface target 20 km east of the observer. Horizon 33.6 km; R50 is capped at the
/// 250 km maximum range, so Pd = 1/(1 + 0.08^8), effectively 1.
fn surface_track_run(seed: u64, ticks: u64) -> (Simulation, GeoPoint) {
    let mut sim = Simulation::new(seed);
    sim.submit(spawn(
        1,
        1,
        Domain::Surface,
        ORIGIN,
        0.0,
        0.0,
        0.0,
        vec![radar()],
        5000.0,
    ))
    .unwrap();
    let at = direct(ORIGIN, 90.0, 20_000.0).point;
    sim.submit(spawn(
        2,
        2,
        Domain::Surface,
        at,
        0.0,
        0.0,
        0.0,
        vec![],
        5000.0,
    ))
    .unwrap();
    sim.run_ticks(ticks);
    (sim, at)
}

#[test]
fn first_detection_reports_the_single_measurement_error() {
    // One scan at tick 0; the snapshot is at tick 1, so 1 s of growth at 18 m/s.
    let (mut sim, truth) = surface_track_run(11, 1);
    let blue = sim.snapshot(Viewer::Side { side: SideId(1) });
    let c = &blue.contacts[0];
    assert_eq!(c.classification, Classification::Domain);
    assert_eq!(c.domain, Some(Domain::Surface));
    // Cross-range σ = 20 km × 0.5° = 174.53 m; along-range σ = 50 m. Growth adds 18² m² to both.
    let cross = 20_000.0 * 0.5_f64.to_radians();
    let major = (cross * cross + 18.0 * 18.0).sqrt();
    let minor = (50.0_f64 * 50.0 + 18.0 * 18.0).sqrt();
    assert!((c.error.semi_major_m - major).abs() < 0.5, "{:?}", c.error);
    assert!((c.error.semi_minor_m - minor).abs() < 0.5, "{:?}", c.error);
    // Major axis across the line of sight (east), so it points north–south.
    assert!(c.error.orientation_deg < 1.0 || c.error.orientation_deg > 179.0);
    // The reported position is not ground truth, but within 4 σ of it.
    let miss = sim_geo::inverse(truth, c.position).distance_m;
    assert!(miss > 0.0 && miss < 4.0 * major, "miss {miss}");
}

#[test]
fn repeated_detections_shrink_the_track_error() {
    // 31 scans (ticks 0..=150); the snapshot is at tick 151.
    let (mut sim, truth) = surface_track_run(12, 151);
    let blue = sim.snapshot(Viewer::Side { side: SideId(1) });
    let c = &blue.contacts[0];
    let single = 20_000.0 * 0.5_f64.to_radians();
    assert!(c.error.semi_major_m < single, "{:?}", c.error);
    let miss = sim_geo::inverse(truth, c.position).distance_m;
    assert!(
        miss < 4.0 * c.error.semi_major_m,
        "miss {miss} vs {:?}",
        c.error
    );
}

#[test]
fn measurement_errors_are_logged_only_for_detections() {
    let mut sim = duel(5, Domain::Air, 10_000.0, R50_REF, 1.0);
    sim.run_ticks(40 * SENSOR_PERIOD_TICKS);
    let mut seen = 0;
    for e in sim.events() {
        if let SimEventKind::DetectionAttempt {
            detected,
            range_error_m,
            bearing_error_deg,
            ..
        } = &e.kind
        {
            assert_eq!(range_error_m.is_some(), *detected);
            assert_eq!(bearing_error_deg.is_some(), *detected);
            seen += 1;
        }
    }
    assert_eq!(seen, 40);
}

#[test]
fn track_error_grows_with_time_since_update() {
    // Hand value: σ 100 m, surface bound 18 m/s, 100 s without update:
    // √(100² + 1800²) = 1802.776 m on both axes.
    let c = Contact {
        number: 1,
        target: UnitId(2),
        first_seen: Tick(0),
        last_seen: Tick(0),
        position: ORIGIN,
        cov: sim_sensors_cov(100.0),
        classification: Classification::Domain,
        domain: Some(Domain::Surface),
        detected_by: UnitId(1),
        sensor_id: "TEST-RADAR".into(),
    };
    let e = c.error_at(Tick(100));
    assert!((e.semi_major_m - 1802.776).abs() < 1e-3, "{e:?}");
    assert!((e.semi_minor_m - 1802.776).abs() < 1e-3, "{e:?}");
    // An unclassified track grows at the 600 m/s bound: 60 km after 100 s.
    let unknown = Contact {
        classification: Classification::Unknown,
        domain: None,
        ..c
    };
    assert!((unknown.error_at(Tick(100)).semi_major_m - 60_000.08).abs() < 0.01);
}

fn sim_sensors_cov(sigma_m: f64) -> Cov2 {
    Cov2::isotropic(sigma_m)
}

// ---- C2: EMCON, doctrine inheritance, postures (docs/models/c2-doctrine.md) ----

fn order(tick: u64, issuer: Issuer, side: Option<u16>, payload: CommandPayload) -> Command {
    Command {
        tick: Tick(tick),
        issuer,
        side: side.map(SideId),
        payload,
    }
}

fn doctrine_emcon(emcon: Emcon) -> Doctrine {
    Doctrine {
        emcon: Some(emcon),
        ..Doctrine::default()
    }
}

fn detection_draws(sim: &Simulation) -> usize {
    attempts(sim).len()
}

#[test]
fn emcon_silent_radar_detects_nothing_until_active_again() {
    let (mut sim, _) = surface_track_run(21, 0);
    let player = Issuer::Player { seat: 1 };
    sim.submit(order(
        0,
        player.clone(),
        Some(1),
        CommandPayload::SetDoctrine {
            scope: DoctrineScope::Unit { unit: UnitId(1) },
            doctrine: doctrine_emcon(Emcon::Silent),
        },
    ))
    .unwrap();
    sim.run_ticks(100);
    assert_eq!(detection_draws(&sim), 0, "silent radar takes no draws");
    assert!(sim
        .snapshot(Viewer::Side { side: SideId(1) })
        .contacts
        .is_empty());

    // Clearing the unit setting falls back to the default (active).
    sim.submit(order(
        100,
        player,
        Some(1),
        CommandPayload::SetDoctrine {
            scope: DoctrineScope::Unit { unit: UnitId(1) },
            doctrine: Doctrine::default(),
        },
    ))
    .unwrap();
    sim.run_ticks(10);
    assert!(detection_draws(&sim) > 0);
    assert_eq!(
        sim.snapshot(Viewer::Side { side: SideId(1) })
            .contacts
            .len(),
        1
    );
}

#[test]
fn doctrine_inherits_side_then_group_then_unit() {
    let mut sim = Simulation::new(1);
    let mut s = spawn(
        1,
        1,
        Domain::Surface,
        ORIGIN,
        0.0,
        0.0,
        0.0,
        vec![radar()],
        5000.0,
    );
    if let CommandPayload::SpawnUnit(u) = &mut s.payload {
        u.group = Some("TG1".into());
    }
    sim.submit(s).unwrap();
    let set = |scope, doctrine| {
        order(
            0,
            Issuer::Scenario,
            None,
            CommandPayload::SetDoctrine { scope, doctrine },
        )
    };
    sim.submit(set(
        DoctrineScope::Side { side: SideId(1) },
        Doctrine {
            wcs: Some(Wcs::Free),
            emcon: Some(Emcon::Silent),
            engage_unfriendly: Some(true),
        },
    ))
    .unwrap();
    sim.submit(set(
        DoctrineScope::Group {
            side: SideId(1),
            group: "TG1".into(),
        },
        Doctrine {
            wcs: Some(Wcs::Hold),
            ..Doctrine::default()
        },
    ))
    .unwrap();
    sim.submit(set(
        DoctrineScope::Unit { unit: UnitId(1) },
        doctrine_emcon(Emcon::Active),
    ))
    .unwrap();
    sim.step();
    let u = &sim.snapshot(Viewer::Side { side: SideId(1) }).units[0];
    assert_eq!(u.group.as_deref(), Some("TG1"));
    assert_eq!(u.doctrine.wcs, Wcs::Hold, "group over side");
    assert_eq!(u.doctrine.emcon, Emcon::Active, "unit over side");
    assert!(u.doctrine.engage_unfriendly, "from side");
    assert_eq!(u.doctrine_set, doctrine_emcon(Emcon::Active));
}

#[test]
fn players_cannot_set_postures_or_another_sides_doctrine() {
    let (mut sim, _) = surface_track_run(3, 0);
    let red = Issuer::Player { seat: 2 };
    sim.submit(order(
        0,
        red.clone(),
        Some(2),
        CommandPayload::SetDoctrine {
            scope: DoctrineScope::Side { side: SideId(1) },
            doctrine: doctrine_emcon(Emcon::Silent),
        },
    ))
    .unwrap();
    sim.submit(order(
        0,
        red.clone(),
        Some(2),
        CommandPayload::SetDoctrine {
            scope: DoctrineScope::Unit { unit: UnitId(1) },
            doctrine: doctrine_emcon(Emcon::Silent),
        },
    ))
    .unwrap();
    sim.submit(order(
        0,
        red,
        Some(2),
        CommandPayload::SetPosture {
            side: SideId(2),
            toward: SideId(1),
            posture: Posture::Hostile,
        },
    ))
    .unwrap();
    sim.step();
    let rejected = sim
        .events()
        .iter()
        .filter(|e| matches!(e.kind, SimEventKind::CommandRejected { .. }))
        .count();
    assert_eq!(rejected, 3);
    assert!(detection_draws(&sim) > 0, "blue radar still active");
    // A side's posture toward itself is refused at submission.
    assert!(sim
        .submit(order(
            1,
            Issuer::Scenario,
            None,
            CommandPayload::SetPosture {
                side: SideId(1),
                toward: SideId(1),
                posture: Posture::Hostile,
            },
        ))
        .is_err());
}

#[test]
fn postures_are_shown_per_side_and_radar_tracks_have_no_affiliation() {
    let (mut sim, _) = surface_track_run(4, 0);
    for (side, toward, posture) in [(1, 2, Posture::Hostile), (2, 1, Posture::Unfriendly)] {
        sim.submit(order(
            0,
            Issuer::Scenario,
            None,
            CommandPayload::SetPosture {
                side: SideId(side),
                toward: SideId(toward),
                posture,
            },
        ))
        .unwrap();
    }
    sim.run_ticks(6);
    let blue = sim.snapshot(Viewer::Side { side: SideId(1) });
    assert_eq!(
        blue.postures,
        vec![PostureView {
            side: SideId(1),
            toward: SideId(2),
            posture: Posture::Hostile
        }]
    );
    // Radar classifies to domain only, so the side does not know whose the track is.
    assert_eq!(blue.contacts[0].affiliation, None);
    assert_eq!(sim.snapshot(Viewer::WhiteCell).postures.len(), 2);
}

#[test]
fn save_and_resume_keeps_c2_state() {
    let (mut a, _) = surface_track_run(8, 0);
    a.submit(order(
        0,
        Issuer::Scenario,
        None,
        CommandPayload::SetPosture {
            side: SideId(1),
            toward: SideId(2),
            posture: Posture::Hostile,
        },
    ))
    .unwrap();
    a.submit(order(
        20,
        Issuer::Scenario,
        None,
        CommandPayload::SetDoctrine {
            scope: DoctrineScope::Side { side: SideId(1) },
            doctrine: doctrine_emcon(Emcon::Silent),
        },
    ))
    .unwrap();
    a.run_ticks(50);
    let bytes = a.save().to_bytes().unwrap();
    let mut b = Simulation::from_save(SaveGame::from_bytes(&bytes).unwrap()).unwrap();
    assert_eq!(a.state_hash(), b.state_hash());
    a.run_ticks(100);
    b.run_ticks(100);
    assert_eq!(a.state_hash(), b.state_hash());
    assert_eq!(a.events(), b.events());
}
