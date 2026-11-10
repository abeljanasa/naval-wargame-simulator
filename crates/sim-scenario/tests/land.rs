//! Land avoidance in the running engine (docs/models/land-avoidance.md): routes ordered across land
//! are planned round it, and a ship steaming at a coast stops instead of running aground.

#![allow(clippy::unwrap_used)] // test harness

use sim_core::{
    Command, CommandPayload, Doctrine, Domain, GeoPoint, Issuer, SideId, SimEventKind, Simulation,
    Tick, UnitId, UnitSpawn, Viewer,
};

fn ship(at: GeoPoint, heading: f64, speed: f64) -> Command {
    Command {
        tick: Tick(0),
        issuer: Issuer::Scenario,
        side: None,
        payload: CommandPayload::SpawnUnit(UnitSpawn {
            id: UnitId(1),
            name: "ship".into(),
            side: SideId(1),
            domain: Domain::Surface,
            position: at,
            altitude_m: 0.0,
            heading_deg: heading,
            speed_mps: speed,
            max_speed_mps: 15.0,
            accel_mps2: 0.05,
            route: vec![],
            platform: None,
            sensors: vec![],
            rcs_m2: None,
            max_turn_rate_dps: Some(3.0),
            min_turn_radius_m: Some(300.0),
            group: None,
            doctrine: Doctrine::default(),
            jammer: None,
        }),
    }
}

fn sim_with_land() -> Simulation {
    let mut sim = Simulation::new(1);
    sim.set_land_mask(sim_geo::land::embedded());
    sim
}

#[test]
fn route_across_tasmania_is_replanned_and_sailed_without_grounding() {
    let mask = sim_geo::land::embedded();
    let mut sim = sim_with_land();
    sim.submit(ship(GeoPoint::new(-42.0, 144.0), 90.0, 12.0))
        .unwrap();
    sim.submit(Command {
        tick: Tick(0),
        issuer: Issuer::Player { seat: 1 },
        side: Some(SideId(1)),
        payload: CommandPayload::SetRoute {
            unit: UnitId(1),
            waypoints: vec![GeoPoint::new(-42.0, 149.5)],
        },
    })
    .unwrap();
    sim.step();
    let route = sim.snapshot(Viewer::WhiteCell).units[0]
        .route
        .clone()
        .unwrap();
    assert!(route.waypoints.len() > 1, "not replanned: {route:?}");
    assert!(sim
        .events()
        .iter()
        .any(|e| matches!(e.kind, SimEventKind::RouteReplanned { legs: 1, .. })));

    // Sail it: never on land, never stopped by the grounding guard, and it arrives.
    let mut arrived = false;
    for _ in 0..(3 * 86_400) {
        sim.step();
        let u = &sim.snapshot(Viewer::WhiteCell).units[0];
        assert!(
            !mask.is_land(u.position),
            "ran onto land at {:?}",
            u.position
        );
        if sim
            .events()
            .iter()
            .any(|e| matches!(e.kind, SimEventKind::RouteCompleted { .. }))
        {
            arrived = true;
            break;
        }
    }
    assert!(arrived, "never completed the route");
    assert!(!sim
        .events()
        .iter()
        .any(|e| matches!(e.kind, SimEventKind::LandAhead { .. })));
}

#[test]
fn ship_dead_reckoning_at_a_coast_stops_before_land() {
    let mask = sim_geo::land::embedded();
    let mut sim = sim_with_land();
    // In Bass Strait heading due south at 12 m/s: Tasmania's north coast is about 150 km away.
    sim.submit(ship(GeoPoint::new(-39.6, 146.5), 180.0, 12.0))
        .unwrap();
    sim.run_ticks(6 * 3600);
    let u = &sim.snapshot(Viewer::WhiteCell).units[0];
    assert!(!mask.is_land(u.position), "on land at {:?}", u.position);
    assert_eq!(u.speed_mps, 0.0);
    assert!(sim
        .events()
        .iter()
        .any(|e| matches!(e.kind, SimEventKind::LandAhead { .. })));
}

#[test]
fn route_to_a_point_on_land_is_refused() {
    let mut sim = sim_with_land();
    sim.submit(ship(GeoPoint::new(-39.3, 146.0), 90.0, 8.0))
        .unwrap();
    let seq = sim
        .submit(Command {
            tick: Tick(0),
            issuer: Issuer::Player { seat: 1 },
            side: Some(SideId(1)),
            payload: CommandPayload::SetRoute {
                unit: UnitId(1),
                waypoints: vec![GeoPoint::new(-33.0, 147.0)],
            },
        })
        .unwrap();
    sim.step();
    assert!(sim.events().iter().any(|e| matches!(
        &e.kind,
        SimEventKind::CommandRejected { seq: s, reason } if *s == seq && reason.contains("on land")
    )));
}
