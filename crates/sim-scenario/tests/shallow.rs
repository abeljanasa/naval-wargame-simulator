//! Shallow water in the running engine (docs/models/shallow-water.md): with terrain packs, ships
//! route round water too shallow for their draft, stop rather than steam into it, and ports and
//! anchorages take their depth from the chart.

#![allow(clippy::unwrap_used)] // test harness

use sim_core::{
    chart_depth_m, Command, CommandPayload, Doctrine, Domain, Facility, GeoPoint, Issuer, SideId,
    SimEventKind, Simulation, Tick, UnitId, UnitSpawn, Viewer,
};
use sim_geo::terrain::{encode, PackHeader, Terrain, TerrainPack};
use std::sync::Arc;

/// Open central Pacific, 20°S–20°N, 175°W–165°W at 0.1°: 2,000 m deep, with a 4 m bank along
/// 170°W (169.7–170.3°W) between `bank_lat` south and north.
fn terrain(bank_lat: f64) -> Arc<Terrain> {
    let (rows, cols) = (401u32, 101u32);
    let heights: Vec<i16> = (0..rows * cols)
        .map(|i| {
            let lat = -20.0 + 0.1 * f64::from(i / cols);
            let lon = -175.0 + 0.1 * f64::from(i % cols);
            if (lon + 170.0).abs() <= 0.3 && lat.abs() <= bank_lat {
                -4
            } else {
                -2000
            }
        })
        .collect();
    let header = PackHeader {
        name: "bank".into(),
        source: "test".into(),
        credit: "test".into(),
        lat0: -20 * 36_000,
        lon0: -175 * 36_000,
        cell: 3_600,
        rows,
        cols,
        min_m: -2000,
        max_m: -4,
    };
    let pack = TerrainPack::decode(&encode(&header, &heights).unwrap()).unwrap();
    Arc::new(Terrain::new(vec![Arc::new(pack)]).unwrap())
}

fn ship(at: GeoPoint, heading: f64, speed: f64, draft_m: f64, route: Vec<GeoPoint>) -> Command {
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
            route,
            platform: None,
            sensors: vec![],
            rcs_m2: None,
            max_turn_rate_dps: Some(3.0),
            min_turn_radius_m: Some(300.0),
            group: None,
            doctrine: Doctrine::default(),
            jammer: None,
            radiated_noise_db: None,
            category: None,
            length_m: None,
            stores: Default::default(),
            damage_profile: None,
            mission: None,
            fuel: None,
            draft_m: Some(draft_m),
        }),
    }
}

fn sim(terrain: &Arc<Terrain>) -> Simulation {
    let mut sim = Simulation::new(1);
    sim.set_land_mask(sim_geo::land::embedded());
    sim.set_terrain(Some(terrain.clone()));
    sim
}

fn has(sim: &Simulation, f: impl Fn(&SimEventKind) -> bool) -> bool {
    sim.events().iter().any(|e| f(&e.kind))
}

/// Runs until the route is complete (or `max_s`), checking the ship's depth every minute.
fn sail(sim: &mut Simulation, terrain: &Terrain, draft: f64, max_s: u64) -> f64 {
    let mut least = f64::MAX;
    for _ in 0..max_s / 60 {
        sim.run_ticks(60);
        let u = &sim.snapshot(Viewer::WhiteCell).units[0];
        least = least.min(terrain.depth_m(u.position).unwrap());
        if has(sim, |k| matches!(k, SimEventKind::RouteCompleted { .. })) {
            return least;
        }
    }
    let u = &sim.snapshot(Viewer::WhiteCell).units[0];
    let ev: Vec<String> = sim
        .events()
        .iter()
        .filter(|e| !matches!(e.kind, SimEventKind::UnitSpawned { .. }))
        .map(|e| format!("{:?} {:?}", e.tick, e.kind))
        .collect();
    panic!(
        "never completed the route (least depth {least} m, draft {draft} m) at {:?} speed {} events {ev:?}",
        u.position, u.speed_mps
    );
}

#[test]
fn a_deep_draft_ship_routes_round_the_bank_and_sails_it() {
    let t = terrain(3.0);
    let mut sim = sim(&t);
    let route = vec![GeoPoint::new(0.0, -168.0)];
    sim.submit(ship(GeoPoint::new(0.0, -172.0), 90.0, 12.0, 6.0, route))
        .unwrap();
    sim.run_ticks(1);
    assert!(has(&sim, |k| matches!(
        k,
        SimEventKind::RouteReplanned { .. }
    )));
    assert!(!has(&sim, |k| matches!(
        k,
        SimEventKind::RouteShallow { .. }
    )));
    let least = sail(&mut sim, &t, 6.0, 24 * 3600);
    assert!(least >= 7.0, "passed over {least} m with a 6 m draft");
    assert!(!has(&sim, |k| matches!(k, SimEventKind::ShoalAhead { .. })));
}

#[test]
fn a_shallow_draft_ship_crosses_the_bank() {
    let t = terrain(3.0);
    let mut sim = sim(&t);
    let route = vec![GeoPoint::new(0.0, -168.0)];
    sim.submit(ship(GeoPoint::new(0.0, -172.0), 90.0, 12.0, 2.0, route))
        .unwrap();
    sim.run_ticks(1);
    assert!(!has(&sim, |k| matches!(
        k,
        SimEventKind::RouteReplanned { .. }
    )));
    let least = sail(&mut sim, &t, 2.0, 12 * 3600);
    assert!(least < 5.0, "it went the long way round ({least} m)");
}

#[test]
fn with_no_deep_route_the_ship_is_warned_and_stops_at_the_bank() {
    // The bank runs the whole height of the pack: no way round within the search.
    let t = terrain(30.0);
    let mut sim = sim(&t);
    let route = vec![GeoPoint::new(0.0, -168.0)];
    sim.submit(ship(GeoPoint::new(0.0, -172.0), 90.0, 12.0, 6.0, route))
        .unwrap();
    sim.run_ticks(1);
    assert!(has(&sim, |k| matches!(
        k,
        SimEventKind::RouteShallow { legs: 1, .. }
    )));
    sim.run_ticks(6 * 3600);
    let u = &sim.snapshot(Viewer::WhiteCell).units[0];
    assert_eq!(u.speed_mps, 0.0);
    assert!(t.depth_m(u.position).unwrap() >= 6.0);
    assert!(has(
        &sim,
        |k| matches!(k, SimEventKind::ShoalAhead { depth_m, .. } if *depth_m < 6.0)
    ));
}

#[test]
fn without_terrain_nothing_changes() {
    let mut sim = Simulation::new(1);
    sim.set_land_mask(sim_geo::land::embedded());
    let route = vec![GeoPoint::new(0.0, -168.0)];
    sim.submit(ship(GeoPoint::new(0.0, -172.0), 90.0, 12.0, 6.0, route))
        .unwrap();
    sim.run_ticks(6 * 3600);
    assert!(!has(&sim, |k| matches!(
        k,
        SimEventKind::RouteReplanned { .. }
            | SimEventKind::RouteShallow { .. }
            | SimEventKind::ShoalAhead { .. }
    )));
}

fn anchorage(at: GeoPoint, radius_m: f64, depth_m: Option<f64>) -> Facility {
    serde_json::from_value(serde_json::json!({
        "id": 1, "name": "Bank anchorage", "side": 1, "kind": "anchorage",
        "position": at, "radius_m": radius_m, "depth_m": depth_m,
    }))
    .unwrap()
}

#[test]
fn an_anchorage_takes_its_deepest_charted_water() {
    let t = terrain(3.0);
    // On the bank, but its 60 km circle reaches deep water either side.
    let wide = anchorage(GeoPoint::new(0.0, -170.0), 60_000.0, None);
    assert_eq!(chart_depth_m(&t, &wide), Some(2000.0));
    // A small one on the bank has only the bank's 4 m.
    let small = anchorage(GeoPoint::new(0.0, -170.0), 2_000.0, None);
    assert_eq!(chart_depth_m(&t, &small), Some(4.0));
    // Off every pack: unknown.
    let far = anchorage(GeoPoint::new(40.0, 10.0), 2_000.0, None);
    assert_eq!(chart_depth_m(&t, &far), None);
}

#[test]
fn depth_along_the_route_shows_the_least_water_ahead() {
    let t = terrain(3.0);
    let route = vec![GeoPoint::new(0.0, -168.0)];
    // Shallow draft: straight over the bank, about 2° (220 km) ahead.
    let mut s1 = sim(&t);
    s1.submit(ship(
        GeoPoint::new(0.0, -172.0),
        90.0,
        12.0,
        2.0,
        route.clone(),
    ))
    .unwrap();
    s1.run_ticks(1);
    let d = s1.route_depths(UnitId(1), Viewer::WhiteCell).unwrap();
    assert_eq!(d.draft_m, Some(2.0));
    assert_eq!(d.here_m, Some(2000.0));
    assert!(d.charted);
    assert_eq!(d.least_m, Some(4.0));
    let at = d.least_at_m.unwrap();
    assert!(
        (180_000.0..230_000.0).contains(&at),
        "least water {at} m ahead"
    );
    assert!(d.profile.len() <= 200 && d.profile.len() > 100);
    // Another side does not see it; without terrain there is nothing to show.
    assert!(s1
        .route_depths(UnitId(1), Viewer::Side { side: SideId(2) })
        .is_none());
    assert!(Simulation::new(1)
        .route_depths(UnitId(1), Viewer::WhiteCell)
        .is_none());
    // Deep draft: the route round the bank keeps clear water all the way.
    let mut s2 = sim(&t);
    s2.submit(ship(GeoPoint::new(0.0, -172.0), 90.0, 12.0, 6.0, route))
        .unwrap();
    s2.run_ticks(1);
    let d = s2
        .route_depths(UnitId(1), Viewer::Side { side: SideId(1) })
        .unwrap();
    assert!(d.charted);
    assert!(d.least_m.unwrap() >= 7.0, "{:?}", d.least_m);
}
