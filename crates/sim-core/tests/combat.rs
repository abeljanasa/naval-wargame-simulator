//! Weapons and damage: engagement checks, missile flight and seekers, the duel test against the
//! hand-calculated hit rate, damage, destruction, damage control, and save/resume mid-flight.
//! Model doc: docs/models/weapons-damage.md.

#![allow(clippy::unwrap_used, clippy::cast_precision_loss)] // test harness

use sim_core::*;
use sim_geo::direct;
use std::collections::BTreeMap;

const ORIGIN: GeoPoint = GeoPoint {
    lat_deg: 0.0,
    lon_deg: 0.0,
};

fn radar() -> SensorSpec {
    SensorSpec {
        sonar: None,
        sensor_id: "TEST-RADAR".into(),
        kind: SensorKind::Radar,
        mount_height_m: 25.0,
        min_range_m: 0.0,
        max_range_m: 250_000.0,
        ref_range_m: Some(120_000.0),
        ref_rcs_m2: Some(1.0),
        max_target_altitude_m: 25_000.0,
        clutter_db_per_sea_state: None,
        beamwidth_deg: None,
        active: false,
    }
}

fn asm() -> WeaponSpec {
    WeaponSpec {
        id: "T-ASM".into(),
        name: "test anti-ship missile".into(),
        kind: WeaponKind::Asm,
        guidance: Guidance::ActiveRadar,
        min_range_m: 5_000.0,
        max_range_m: 150_000.0,
        speed_mps: 270.0,
        warhead_kg: 220.0,
        base_ph: 0.8,
        target_domains: vec!["surface".into()],
        max_range_by_target: std::collections::BTreeMap::new(),
    }
}

fn gun() -> WeaponSpec {
    WeaponSpec {
        id: "T-GUN".into(),
        name: "test gun".into(),
        kind: WeaponKind::Gun,
        guidance: Guidance::Command,
        min_range_m: 0.0,
        max_range_m: 15_000.0,
        speed_mps: 900.0,
        warhead_kg: 6.0,
        base_ph: 0.3,
        target_domains: vec!["surface".into(), "air".into()],
        max_range_by_target: std::collections::BTreeMap::new(),
    }
}

fn sam() -> WeaponSpec {
    WeaponSpec {
        id: "T-SAM".into(),
        name: "test SAM".into(),
        kind: WeaponKind::Sam,
        guidance: Guidance::SemiActive,
        min_range_m: 1_000.0,
        max_range_m: 50_000.0,
        speed_mps: 900.0,
        warhead_kg: 40.0,
        base_ph: 0.75,
        target_domains: vec!["air".into()],
        max_range_by_target: std::collections::BTreeMap::new(),
    }
}

fn cmd(tick: u64, issuer: Issuer, side: Option<u16>, payload: CommandPayload) -> Command {
    Command {
        tick: Tick(tick),
        issuer,
        side: side.map(SideId),
        payload,
    }
}

fn unit(
    id: u64,
    side: u16,
    at: GeoPoint,
    sensors: Vec<SensorSpec>,
    stores: &[(&str, u32)],
    damage: Option<DamageProfile>,
) -> Command {
    cmd(
        0,
        Issuer::Scenario,
        None,
        CommandPayload::SpawnUnit(UnitSpawn {
            id: UnitId(id),
            name: format!("U{id}"),
            side: SideId(side),
            domain: Domain::Surface,
            position: at,
            altitude_m: 0.0,
            heading_deg: 0.0,
            speed_mps: 0.0,
            max_speed_mps: 15.0,
            accel_mps2: 0.1,
            route: vec![],
            platform: None,
            sensors,
            rcs_m2: Some(5000.0),
            max_turn_rate_dps: None,
            min_turn_radius_m: None,
            group: None,
            doctrine: Doctrine::default(),
            jammer: None,
            radiated_noise_db: None,
            category: None,
            length_m: None,
            stores: stores
                .iter()
                .map(|(w, n)| ((*w).to_owned(), *n))
                .collect::<BTreeMap<_, _>>(),
            damage_profile: damage,
            mission: None,
            fuel: None,
            draft_m: None,
            displacement_t: None,
            air: None,
        }),
    )
}

fn weapons_free(tick: u64) -> Command {
    cmd(
        tick,
        Issuer::Player { seat: 1 },
        Some(1),
        CommandPayload::SetDoctrine {
            scope: DoctrineScope::Unit { unit: UnitId(1) },
            doctrine: Doctrine {
                wcs: Some(Wcs::Free),
                ..Doctrine::default()
            },
        },
    )
}

fn engage_as(tick: u64, issuer: Issuer, side: Option<u16>, weapon: &str, rounds: u32) -> Command {
    cmd(
        tick,
        issuer,
        side,
        CommandPayload::Engage {
            shooter: UnitId(1),
            contact: 1,
            weapon: weapon.into(),
            rounds,
        },
    )
}

fn engage(tick: u64, weapon: &str, rounds: u32) -> Command {
    engage_as(tick, Issuer::Player { seat: 1 }, Some(1), weapon, rounds)
}

fn umpire() -> Issuer {
    Issuer::WhiteCell { umpire: "u".into() }
}

/// Blue (radar, 2 ASM, 100 gun rounds, 4 SAM) at the origin; a red ship `range_m` east,
/// detected at tick 0 (Pd ≈ 1 inside the 33.6 km horizon).
fn scene(seed: u64, range_m: f64, red_damage: Option<DamageProfile>) -> Simulation {
    let mut sim = Simulation::new(seed);
    sim.submit(cmd(
        0,
        Issuer::Scenario,
        None,
        CommandPayload::SetWeaponCatalog {
            weapons: vec![asm(), gun(), sam()],
        },
    ))
    .unwrap();
    sim.submit(unit(
        1,
        1,
        ORIGIN,
        vec![radar()],
        &[("T-ASM", 2), ("T-GUN", 100), ("T-SAM", 4)],
        None,
    ))
    .unwrap();
    let at = direct(ORIGIN, 90.0, range_m).point;
    sim.submit(unit(2, 2, at, vec![], &[], red_damage)).unwrap();
    sim.run_ticks(1);
    sim
}

fn rejected(sim: &Simulation) -> Vec<String> {
    sim.events()
        .iter()
        .filter_map(|e| match &e.kind {
            SimEventKind::CommandRejected { reason, .. } => Some(reason.clone()),
            _ => None,
        })
        .collect()
}

fn any_hit(sim: &Simulation) -> bool {
    sim.events()
        .iter()
        .any(|e| matches!(e.kind, SimEventKind::HitRoll { hit: true, .. }))
}

#[test]
fn engage_orders_are_checked_before_anything_is_fired() {
    let mut sim = scene(1, 30_000.0, None);
    // Weapons tight (the default) and the track is only classified to domain: refused.
    sim.submit(engage(1, "T-ASM", 1)).unwrap();
    // Wrong domain, out of range, not carried, not enough rounds.
    sim.submit(engage(1, "T-SAM", 1)).unwrap();
    sim.submit(engage(1, "T-GUN", 1)).unwrap();
    sim.submit(engage(1, "T-TORP", 1)).unwrap();
    sim.submit(engage(1, "T-ASM", 3)).unwrap();
    sim.step();
    let reasons = rejected(&sim);
    assert_eq!(reasons.len(), 5, "{reasons:?}");
    assert!(reasons[0].contains("weapons control status"), "{reasons:?}");
    assert!(reasons[1].contains("cannot engage surface"), "{reasons:?}");
    assert!(reasons[2].contains("outside"), "{reasons:?}");
    assert!(reasons[3].contains("unknown weapon"), "{reasons:?}");
    assert!(reasons[4].contains("not 3"), "{reasons:?}");
    // Nothing was spent.
    let blue = sim.snapshot(Viewer::Side { side: SideId(1) });
    assert!(blue.units[0]
        .stores
        .iter()
        .any(|s| s.weapon == "T-ASM" && s.count == 2));

    // Weapons free: the same order is accepted and one missile leaves.
    let mut sim = scene(1, 30_000.0, None);
    sim.submit(weapons_free(1)).unwrap();
    sim.submit(engage(1, "T-ASM", 1)).unwrap();
    sim.step();
    assert!(rejected(&sim).is_empty(), "{:?}", rejected(&sim));
    let snap = sim.snapshot(Viewer::Side { side: SideId(1) });
    assert_eq!(snap.weapons.len(), 1);
    assert!(snap.units[0]
        .stores
        .iter()
        .any(|s| s.weapon == "T-ASM" && s.count == 1));
    // Red cannot see Blue's missile in its own picture (weapons are not detectable yet).
    assert!(sim
        .snapshot(Viewer::Side { side: SideId(2) })
        .weapons
        .is_empty());
}

/// Fires one ASM at 30 km with weapons free and runs to the end of its flight.
fn asm_shot(seed: u64, red_damage: Option<DamageProfile>) -> Simulation {
    let mut sim = scene(seed, 30_000.0, red_damage);
    sim.submit(weapons_free(1)).unwrap();
    sim.submit(engage(1, "T-ASM", 1)).unwrap();
    // 30 km at 270 m/s: about 111 s.
    sim.run_ticks(200);
    sim
}

#[test]
fn a_missile_flies_acquires_and_rolls_its_hand_calculated_ph() {
    let sim = asm_shot(7, None);
    let launched = sim
        .events()
        .iter()
        .find(|e| matches!(e.kind, SimEventKind::WeaponLaunched { .. }))
        .unwrap()
        .tick;
    let acq = sim
        .events()
        .iter()
        .find_map(|e| match e.kind {
            SimEventKind::SeekerAcquisition { range_m, .. } => Some((e.tick, range_m)),
            _ => None,
        })
        .unwrap();
    // The seeker looks when the target is first inside 20 km: 10 km flown, ≈ 37 s.
    assert!(acq.1 <= 20_000.0 && acq.1 > 19_000.0, "{acq:?}");
    assert!((35..=40).contains(&(acq.0 .0 - launched.0)), "{acq:?}");
    if let Some((ph, size, speed, sea)) = sim.events().iter().find_map(|e| match e.kind {
        SimEventKind::HitRoll {
            ph,
            size_modifier,
            speed_modifier,
            sea_modifier,
            ..
        } => Some((ph, size_modifier, speed_modifier, sea_modifier)),
        _ => None,
    }) {
        // Stationary 100 m target, calm sea: no modifiers, Ph = base 0.8.
        assert_eq!((size, speed, sea), (1.0, 1.0, 1.0));
        assert!((ph - 0.8).abs() < 1e-12);
    }
    // The weapon is gone either way.
    assert!(sim.weapons_in_flight().is_empty());
}

#[test]
fn duel_hit_rate_matches_acquisition_times_ph() {
    // 300 independent one-missile engagements. Expected hit rate = 0.9 (seeker) × 0.8 (Ph)
    // = 0.72; σ = √(0.72 × 0.28 / 300) = 0.0259; tolerance 4σ.
    let n: u32 = 300;
    let hits = (0..n)
        .filter(|&seed| any_hit(&asm_shot(1_000 + u64::from(seed), None)))
        .count();
    let rate = hits as f64 / f64::from(n);
    assert!((rate - 0.72).abs() < 4.0 * 0.0259, "hit rate {rate}");
}

#[test]
fn a_hit_mission_kills_a_frigate_and_destroys_a_small_ship() {
    let frigate = DamageProfile {
        damage_points: 450.0,
        mission_kill_fraction: 0.45,
        fire_susceptibility: 0.0,
        flooding_susceptibility: 0.0,
    };
    // First seed whose missile hits.
    let mut sim = (0..50)
        .map(|s| asm_shot(2_000 + s, Some(frigate)))
        .find(any_hit)
        .unwrap();
    let taken = sim
        .events()
        .iter()
        .find_map(|e| match &e.kind {
            SimEventKind::DamageTaken {
                damage_dp,
                total_dp,
                status,
                ..
            } => Some((*damage_dp, *total_dp, *status)),
            _ => None,
        })
        .unwrap();
    // 220 kg = 220 DP = 48.9 % of 450: past the 45 % mission-kill line.
    assert_eq!(taken, (220.0, 220.0, Status::MissionKilled));
    let red = sim.snapshot(Viewer::Side { side: SideId(2) });
    assert_eq!(red.units[0].status, Status::MissionKilled);
    assert!((red.units[0].damage_fraction - 220.0 / 450.0).abs() < 1e-12);

    let small = DamageProfile {
        damage_points: 200.0,
        ..frigate
    };
    let mut sim = (0..50)
        .map(|s| asm_shot(2_000 + s, Some(small)))
        .find(any_hit)
        .unwrap();
    assert!(sim
        .events()
        .iter()
        .any(|e| matches!(e.kind, SimEventKind::UnitDestroyed { unit: UnitId(2) })));
    assert!(sim
        .snapshot(Viewer::Side { side: SideId(2) })
        .units
        .is_empty());
}

#[test]
fn fires_burn_until_damage_control_puts_them_out() {
    let tinder = DamageProfile {
        damage_points: 5_000.0,
        mission_kill_fraction: 0.9,
        fire_susceptibility: 1.0,
        flooding_susceptibility: 0.0,
    };
    // First seed whose hit starts a fire (chance 0.6 per hit).
    let mut sim = (0..80)
        .map(|s| asm_shot(3_000 + s, Some(tinder)))
        .find(|sim| {
            sim.events().iter().any(|e| {
                matches!(
                    e.kind,
                    SimEventKind::DamageTaken {
                        fire_started: true,
                        ..
                    }
                )
            })
        })
        .unwrap();
    sim.run_ticks(600);
    let dc: Vec<(f64, u32)> = sim
        .events()
        .iter()
        .filter_map(|e| match e.kind {
            SimEventKind::DamageControl {
                fire_dp, fires_out, ..
            } => Some((fire_dp, fires_out)),
            _ => None,
        })
        .collect();
    assert!(!dc.is_empty());
    // One fire burns at 2 DP a minute until put out (P = 0.25 a minute).
    assert!(dc.iter().all(|(dp, _)| *dp == 2.0), "{dc:?}");
    let total = 220.0 + 2.0 * dc.len() as f64;
    let red = sim.snapshot(Viewer::Side { side: SideId(2) });
    assert!((red.units[0].damage_fraction * 5_000.0 - total).abs() < 1e-9);
}

#[test]
fn guns_resolve_at_once_and_the_white_cell_may_override_wcs() {
    // Gun at 10 km (Ph 0.3), five rounds, ordered by the white cell under weapons tight.
    let mut sim = scene(9, 10_000.0, None);
    sim.submit(engage_as(1, umpire(), None, "T-GUN", 5))
        .unwrap();
    sim.step();
    let rolls = sim
        .events()
        .iter()
        .filter(|e| {
            matches!(
                e.kind,
                SimEventKind::HitRoll {
                    weapon_id: None,
                    ..
                }
            )
        })
        .count();
    assert_eq!(rolls, 5);
    assert!(sim.weapons_in_flight().is_empty());
}

#[test]
fn save_and_resume_with_missiles_in_flight_is_exact() {
    let mut a = scene(10, 30_000.0, None);
    a.submit(engage_as(1, umpire(), None, "T-ASM", 2)).unwrap();
    a.run_ticks(50);
    assert_eq!(a.weapons_in_flight().len(), 2);
    let bytes = a.save().to_bytes().unwrap();
    let mut b = Simulation::from_save(SaveGame::from_bytes(&bytes).unwrap()).unwrap();
    assert_eq!(a.state_hash(), b.state_hash());
    a.run_ticks(400);
    b.run_ticks(400);
    assert_eq!(a.state_hash(), b.state_hash());
    assert_eq!(a.events(), b.events());
}

// ---- Point defence and soft kill (slice 9) ----

fn chaff() -> WeaponSpec {
    WeaponSpec {
        id: "T-CHAFF".into(),
        name: "test chaff".into(),
        kind: WeaponKind::Decoy,
        guidance: Guidance::Unguided,
        min_range_m: 0.0,
        max_range_m: 2_000.0,
        speed_mps: 0.0,
        warhead_kg: 0.0,
        base_ph: 0.3,
        target_domains: vec!["air".into()],
        max_range_by_target: std::collections::BTreeMap::new(),
    }
}

/// Blue (radar, `blue_stores`) at the origin defends against red (radar, 4 ASM) 30 km east,
/// which the white cell orders to fire `salvo` missiles at blue's track.
fn raid(seed: u64, blue_stores: &[(&str, u32)], jammer: bool, salvo: u32) -> Simulation {
    let mut sim = Simulation::new(seed);
    sim.submit(cmd(
        0,
        Issuer::Scenario,
        None,
        CommandPayload::SetWeaponCatalog {
            weapons: vec![asm(), sam(), chaff()],
        },
    ))
    .unwrap();
    let mut blue = unit(1, 1, ORIGIN, vec![radar()], blue_stores, None);
    if jammer {
        if let CommandPayload::SpawnUnit(u) = &mut blue.payload {
            u.jammer = Some(JammerSpec { jn_db_at_1km: 40.0 });
        }
    }
    sim.submit(blue).unwrap();
    let at = direct(ORIGIN, 90.0, 30_000.0).point;
    sim.submit(unit(2, 2, at, vec![radar()], &[("T-ASM", 4)], None))
        .unwrap();
    sim.run_ticks(1);
    sim.submit(cmd(
        1,
        umpire(),
        None,
        CommandPayload::Engage {
            shooter: UnitId(2),
            contact: 1,
            weapon: "T-ASM".into(),
            rounds: salvo,
        },
    ))
    .unwrap();
    sim.run_ticks(200);
    sim
}

#[test]
fn sams_engage_incoming_missiles_with_the_hand_calculated_ph() {
    let sim = raid(21, &[("T-SAM", 8)], false, 1);
    let shot = sim
        .events()
        .iter()
        .find_map(|e| match &e.kind {
            SimEventKind::PointDefenceShot { range_m, .. } => Some((e.tick, *range_m)),
            _ => None,
        })
        .unwrap();
    // The missile is seen at the radar's R₅₀ against 0.1 m² (120 km × 0.1^¼ = 67.5 km, so at
    // once inside the horizon) and engaged after the 8 s SAM reaction time.
    assert!(shot.0 .0 >= 9, "{shot:?}");
    let roll = sim
        .events()
        .iter()
        .find_map(|e| match e.kind {
            SimEventKind::InterceptRoll { ph, .. } => Some(ph),
            _ => None,
        })
        .unwrap();
    // SAM 0.75 × manoeuvre 0.9 (a 270 m/s threat is over 200 m/s) = 0.675.
    assert!((roll - 0.675).abs() < 1e-12, "{roll}");
}

#[test]
fn two_sam_channels_limit_a_four_missile_raid() {
    let sim = raid(22, &[("T-SAM", 20)], false, 4);
    let shots: Vec<(u64, u64)> = sim
        .events()
        .iter()
        .filter_map(|e| match e.kind {
            SimEventKind::PointDefenceShot { resolve_at, .. } => Some((e.tick.0, resolve_at.0)),
            _ => None,
        })
        .collect();
    assert!(shots.len() >= 2);
    // At no moment are more than two SAMs in the air from one ship.
    for &(t, _) in &shots {
        let in_air = shots.iter().filter(|&&(s, r)| s <= t && t < r).count();
        assert!(in_air <= 2, "{in_air} at {t}: {shots:?}");
    }
}

#[test]
fn decoys_and_jamming_lower_ph_at_impact() {
    // No SAMs: the missile always arrives. Chaff (0.3) and a radiating jammer against an active
    // radar seeker: countermeasures × 0.7 × 0.8 = 0.56; Ph = 0.8 × 0.56 = 0.448.
    let sim = (0..40)
        .map(|s| raid(400 + s, &[("T-CHAFF", 2)], true, 1))
        .find(|sim| {
            sim.events()
                .iter()
                .any(|e| matches!(e.kind, SimEventKind::HitRoll { .. }))
        })
        .unwrap();
    assert!(sim.events().iter().any(|e| matches!(
        e.kind,
        SimEventKind::DecoyLaunched { effect, .. } if (effect - 0.3).abs() < 1e-12
    )));
    let (cm, p) = sim
        .events()
        .iter()
        .find_map(|e| match e.kind {
            SimEventKind::HitRoll {
                countermeasure_modifier,
                ph,
                ..
            } => Some((countermeasure_modifier, ph)),
            _ => None,
        })
        .unwrap();
    assert!((cm - 0.56).abs() < 1e-12, "{cm}");
    assert!((p - 0.448).abs() < 1e-12, "{p}");
}

// ---- Missions and automated tasking (slice 10; docs/models/missions.md) ----

fn assign(
    tick: u64,
    issuer: Issuer,
    side: Option<u16>,
    unit: u64,
    mission: Option<Mission>,
) -> Command {
    cmd(
        tick,
        issuer,
        side,
        CommandPayload::AssignMission {
            unit: UnitId(unit),
            mission,
        },
    )
}

fn mission_events(sim: &Simulation) -> (Vec<GeoPoint>, usize, Vec<(u32, String)>) {
    let mut points = vec![];
    let mut complete = 0;
    let mut engaged = vec![];
    for e in sim.events() {
        match &e.kind {
            SimEventKind::MissionWaypoint { point, .. } => points.push(*point),
            SimEventKind::MissionComplete { .. } => complete += 1,
            SimEventKind::MissionEngage {
                contact, weapon, ..
            } => engaged.push((*contact, weapon.clone())),
            _ => {}
        }
    }
    (points, complete, engaged)
}

#[test]
fn a_patrol_picks_points_inside_its_area_and_keeps_moving() {
    let mut sim = scene(31, 30_000.0, None);
    let centre = direct(ORIGIN, 0.0, 20_000.0).point;
    sim.submit(assign(
        1,
        Issuer::Player { seat: 1 },
        Some(1),
        1,
        Some(Mission::Patrol {
            centre,
            radius_m: 10_000.0,
        }),
    ))
    .unwrap();
    sim.run_ticks(3_600);
    let (points, _, _) = mission_events(&sim);
    assert!(points.len() >= 2, "{}", points.len());
    for p in &points {
        assert!(sim_geo::inverse(centre, *p).distance_m <= 10_000.0 + 1e-6);
    }
    let blue = sim.snapshot(Viewer::Side { side: SideId(1) });
    assert!(blue.units[0].speed_mps > 0.0);
    assert!(matches!(
        blue.units[0].mission,
        Some(Mission::Patrol { .. })
    ));
}

#[test]
fn a_ferry_completes_on_arrival_and_an_escort_holds_station() {
    let mut sim = scene(32, 30_000.0, None);
    let dest = direct(ORIGIN, 180.0, 5_000.0).point;
    sim.submit(assign(
        1,
        umpire(),
        None,
        1,
        Some(Mission::Ferry { destination: dest }),
    ))
    .unwrap();
    sim.run_ticks(1_800);
    let (_, complete, _) = mission_events(&sim);
    assert_eq!(complete, 1);
    let blue = sim.snapshot(Viewer::Side { side: SideId(1) });
    assert!(blue.units[0].mission_complete);

    // Red escorts nothing of its own here, so escort blue as the white cell's test: station
    // 2 km on blue's starboard beam.
    let mut sim = scene(33, 6_000.0, None);
    sim.submit(assign(
        1,
        umpire(),
        None,
        2,
        Some(Mission::Escort {
            escorted: 1,
            bearing_deg: 90.0,
            range_m: 2_000.0,
        }),
    ))
    .unwrap();
    sim.run_ticks(1_800);
    let white = sim.snapshot(Viewer::WhiteCell);
    let (b, r) = (&white.units[0], &white.units[1]);
    let station = sim_ai_station(b.position, b.heading_deg, 90.0, 2_000.0);
    assert!(
        sim_geo::inverse(r.position, station).distance_m < 600.0,
        "{:?}",
        sim_geo::inverse(r.position, station)
    );
}

fn sim_ai_station(p: GeoPoint, heading: f64, bearing: f64, range: f64) -> GeoPoint {
    direct(p, heading + bearing, range).point
}

#[test]
fn automated_engagement_follows_the_release_rule() {
    // Weapons free on a patrol: the AI fires one ASM at the nearest contact, and no second one
    // while the first is in flight.
    let mut sim = scene(34, 30_000.0, None);
    sim.submit(weapons_free(1)).unwrap();
    sim.submit(assign(
        1,
        Issuer::Player { seat: 1 },
        Some(1),
        1,
        Some(Mission::Patrol {
            centre: ORIGIN,
            radius_m: 5_000.0,
        }),
    ))
    .unwrap();
    sim.run_ticks(60);
    let (_, _, engaged) = mission_events(&sim);
    assert_eq!(engaged, vec![(1, "T-ASM".to_owned())]);
    assert_eq!(sim.weapons_in_flight().len(), 1);

    // Weapons tight (default): the unidentified contact may not be engaged.
    let mut sim = scene(34, 30_000.0, None);
    sim.submit(assign(
        1,
        Issuer::Player { seat: 1 },
        Some(1),
        1,
        Some(Mission::Strike { contact: 1 }),
    ))
    .unwrap();
    sim.run_ticks(60);
    assert!(mission_events(&sim).2.is_empty());
    assert!(sim.weapons_in_flight().is_empty());
}

#[test]
fn players_task_only_their_own_units() {
    let mut sim = scene(35, 30_000.0, None);
    sim.submit(assign(1, Issuer::Player { seat: 2 }, Some(2), 1, None))
        .unwrap();
    sim.step();
    assert_eq!(rejected(&sim).len(), 1, "{:?}", rejected(&sim));
    assert!(sim
        .submit(assign(
            2,
            umpire(),
            None,
            1,
            Some(Mission::Patrol {
                centre: ORIGIN,
                radius_m: -5.0,
            }),
        ))
        .is_err());
}

// ---- Basic mines (slice 11) ----

fn mine() -> WeaponSpec {
    WeaponSpec {
        id: "T-MINE".into(),
        name: "test bottom mine".into(),
        kind: WeaponKind::Mine,
        guidance: Guidance::Unguided,
        min_range_m: 0.0,
        max_range_m: 50.0,
        speed_mps: 0.0,
        warhead_kg: 600.0,
        base_ph: 0.5,
        target_domains: vec!["surface".into(), "subsurface".into()],
        max_range_by_target: std::collections::BTreeMap::new(),
    }
}

/// A side-1 ship crosses (west to east, 10 m/s) a side-2 field of 20 mines, radius 2 km,
/// centred on the origin.
fn mined_transit(seed: u64) -> Simulation {
    let mut sim = Simulation::new(seed);
    sim.submit(cmd(
        0,
        Issuer::Scenario,
        None,
        CommandPayload::SetWeaponCatalog {
            weapons: vec![mine()],
        },
    ))
    .unwrap();
    sim.submit(cmd(
        0,
        Issuer::Scenario,
        None,
        CommandPayload::CreateMinefield {
            side: SideId(2),
            centre: ORIGIN,
            radius_m: 2_000.0,
            mines: 20,
            weapon: "T-MINE".into(),
        },
    ))
    .unwrap();
    let start = direct(ORIGIN, 270.0, 3_000.0).point;
    let mut ship = unit(1, 1, start, vec![], &[], None);
    if let CommandPayload::SpawnUnit(u) = &mut ship.payload {
        u.heading_deg = 90.0;
        u.speed_mps = 10.0;
        u.route = vec![direct(ORIGIN, 90.0, 3_000.0).point];
        u.damage_profile = Some(DamageProfile {
            damage_points: 100_000.0,
            mission_kill_fraction: 0.99,
            fire_susceptibility: 0.0,
            flooding_susceptibility: 0.0,
        });
    }
    sim.submit(ship).unwrap();
    sim.run_ticks(650);
    sim
}

#[test]
fn mine_transit_risk_matches_the_simple_initial_threat() {
    // λ = ρ W d = 20 / (π × 2000²) × 100 × 4000 = 0.6366; P(≥ 1 encounter) = 0.4710.
    // 200 transits: σ = √(0.471 × 0.529 / 200) = 0.0353; tolerance 4σ.
    let n: u32 = 200;
    let hit = (0..n)
        .filter(|&s| {
            mined_transit(5_000 + u64::from(s))
                .events()
                .iter()
                .any(|e| {
                    matches!(
                        e.kind,
                        SimEventKind::MineEncounter {
                            encountered: true,
                            ..
                        }
                    )
                })
        })
        .count();
    let rate = hit as f64 / f64::from(n);
    assert!((rate - 0.471).abs() < 4.0 * 0.0353, "rate {rate}");
    // Each 10 s check inside the field logs its probability: 100 m step, P = 0.01579.
    let sim = mined_transit(5_000);
    let p = sim
        .events()
        .iter()
        .find_map(|e| match e.kind {
            SimEventKind::MineEncounter { p, distance_m, .. }
                if (distance_m - 100.0).abs() < 0.5 =>
            {
                Some(p)
            }
            _ => None,
        })
        .unwrap();
    assert!((p - 0.015_79).abs() < 2e-4, "{p}");
}

#[test]
fn a_ship_lays_its_own_field_and_is_not_at_risk_in_it() {
    let mut sim = Simulation::new(61);
    sim.submit(cmd(
        0,
        Issuer::Scenario,
        None,
        CommandPayload::SetWeaponCatalog {
            weapons: vec![mine()],
        },
    ))
    .unwrap();
    let mut layer = unit(1, 1, ORIGIN, vec![], &[("T-MINE", 30)], None);
    if let CommandPayload::SpawnUnit(u) = &mut layer.payload {
        u.speed_mps = 8.0;
        u.heading_deg = 90.0;
        u.route = vec![direct(ORIGIN, 90.0, 5_000.0).point];
    }
    sim.submit(layer).unwrap();
    sim.submit(unit(
        2,
        2,
        direct(ORIGIN, 0.0, 50_000.0).point,
        vec![],
        &[],
        None,
    ))
    .unwrap();
    sim.submit(cmd(
        1,
        Issuer::Player { seat: 1 },
        Some(1),
        CommandPayload::LayMines {
            unit: UnitId(1),
            mines: 20,
            radius_m: 3_000.0,
        },
    ))
    .unwrap();
    sim.run_ticks(600);
    assert!(rejected(&sim).is_empty(), "{:?}", rejected(&sim));
    let blue = sim.snapshot(Viewer::Side { side: SideId(1) });
    assert_eq!(blue.minefields.len(), 1);
    assert_eq!(blue.minefields[0].mines, 20);
    assert!(blue.units[0]
        .stores
        .iter()
        .any(|s| s.weapon == "T-MINE" && s.count == 10));
    // The layer steamed through its own field without a single draw.
    assert!(!sim
        .events()
        .iter()
        .any(|e| matches!(e.kind, SimEventKind::MineEncounter { .. })));
    // Red does not see Blue's field.
    assert!(sim
        .snapshot(Viewer::Side { side: SideId(2) })
        .minefields
        .is_empty());
}

// ---- White-cell adjudication (Phase 3 slice 1; docs/models/adjudication.md) ----

fn adjudicate(tick: u64, issuer: Issuer, reason: &str, action: Adjudication) -> Command {
    cmd(
        tick,
        issuer,
        None,
        CommandPayload::Adjudicate {
            reason: reason.into(),
            action,
        },
    )
}

#[test]
fn only_the_white_cell_adjudicates_and_always_with_a_reason() {
    let mut sim = scene(71, 30_000.0, None);
    assert!(sim
        .submit(adjudicate(
            1,
            umpire(),
            "  ",
            Adjudication::DestroyUnit { unit: UnitId(2) }
        ))
        .is_err());
    sim.submit(adjudicate(
        1,
        Issuer::Player { seat: 1 },
        "I want it gone",
        Adjudication::DestroyUnit { unit: UnitId(2) },
    ))
    .unwrap();
    sim.submit(adjudicate(
        1,
        Issuer::Scenario,
        "scripted",
        Adjudication::DestroyUnit { unit: UnitId(2) },
    ))
    .unwrap();
    sim.step();
    assert_eq!(rejected(&sim).len(), 2, "{:?}", rejected(&sim));
    assert_eq!(sim.snapshot(Viewer::WhiteCell).units.len(), 2);
}

#[test]
fn the_umpire_forces_a_missile_to_hit_or_miss() {
    let frigate = DamageProfile {
        damage_points: 450.0,
        mission_kill_fraction: 0.45,
        fire_susceptibility: 0.0,
        flooding_susceptibility: 0.0,
    };
    for hit in [true, false] {
        let mut sim = scene(72, 30_000.0, Some(frigate));
        sim.submit(engage_as(1, umpire(), None, "T-ASM", 1))
            .unwrap();
        sim.run_ticks(10);
        let id = sim.weapons_in_flight()[0].id;
        sim.submit(adjudicate(
            11,
            umpire(),
            "missile malfunction ruled",
            Adjudication::ForceWeaponOutcome { weapon_id: id, hit },
        ))
        .unwrap();
        sim.step();
        assert!(sim.weapons_in_flight().is_empty());
        let rolls = sim
            .events()
            .iter()
            .filter(|e| matches!(e.kind, SimEventKind::HitRoll { .. }))
            .count();
        assert_eq!(rolls, 0, "no Ph roll on a forced outcome");
        let red = sim.snapshot(Viewer::Side { side: SideId(2) });
        if hit {
            // Full 220 kg warhead: 220 DP, mission-killed.
            assert_eq!(red.units[0].status, Status::MissionKilled);
        } else {
            assert_eq!(red.units[0].damage_fraction, 0.0);
            assert!(sim.events().iter().any(|e| matches!(
                &e.kind,
                SimEventKind::WeaponMissed { reason, .. } if reason == "adjudicated miss"
            )));
        }
    }
}

#[test]
fn damage_repair_destroy_remove_move_and_stores_are_logged_overrides() {
    let mut sim = scene(73, 30_000.0, None);
    let mid = direct(ORIGIN, 0.0, 10_000.0).point;
    let actions = [
        Adjudication::SetDamage {
            unit: UnitId(1),
            damage_dp: 200.0,
            clear_effects: false,
        },
        Adjudication::SetDamage {
            unit: UnitId(1),
            damage_dp: 0.0,
            clear_effects: true,
        },
        Adjudication::MoveUnit {
            unit: UnitId(1),
            position: mid,
            altitude_m: None,
        },
        Adjudication::SetStores {
            unit: UnitId(1),
            weapon: "T-ASM".into(),
            count: 6,
        },
        Adjudication::RemoveUnit { unit: UnitId(2) },
    ];
    for (i, a) in actions.iter().enumerate() {
        sim.submit(adjudicate(
            1 + i as u64,
            umpire(),
            &format!("ruling {i}"),
            a.clone(),
        ))
        .unwrap();
    }
    sim.run_ticks(10);
    assert!(rejected(&sim).is_empty(), "{:?}", rejected(&sim));
    let white = sim.snapshot(Viewer::WhiteCell);
    assert_eq!(white.units.len(), 1, "red removed");
    let blue = &white.units[0];
    assert_eq!(blue.damage_fraction, 0.0);
    assert!(sim_geo::inverse(blue.position, mid).distance_m < 1.0);
    assert!(blue
        .stores
        .iter()
        .any(|s| s.weapon == "T-ASM" && s.count == 6));
    // Removed is not destroyed.
    assert!(!sim
        .events()
        .iter()
        .any(|e| matches!(e.kind, SimEventKind::UnitDestroyed { .. })));
    // Five overrides, each with umpire and reason; only the white cell sees them.
    assert_eq!(white.adjudications.len(), 5);
    assert!(white
        .adjudications
        .iter()
        .all(|a| a.umpire == "u" && a.reason.starts_with("ruling")));
    assert!(sim
        .snapshot(Viewer::Side { side: SideId(1) })
        .adjudications
        .is_empty());
    // And they replay: save, resume, same hash.
    let bytes = sim.save().to_bytes().unwrap();
    let mut b = Simulation::from_save(SaveGame::from_bytes(&bytes).unwrap()).unwrap();
    assert_eq!(sim.state_hash(), b.state_hash());
    assert_eq!(b.snapshot(Viewer::WhiteCell).adjudications.len(), 5);

    // Destroy is logged as a destruction.
    let mut sim = scene(74, 30_000.0, None);
    sim.submit(adjudicate(
        1,
        umpire(),
        "sunk by inject",
        Adjudication::DestroyUnit { unit: UnitId(2) },
    ))
    .unwrap();
    sim.step();
    assert!(sim
        .events()
        .iter()
        .any(|e| matches!(e.kind, SimEventKind::UnitDestroyed { unit: UnitId(2) })));
}

// ---- Planning layer (docs/models/planning-layer.md) ----

fn circle(centre: GeoPoint, radius_m: f64) -> Area {
    Area::Circle { centre, radius_m }
}

fn zone(id: u32, kind: ZoneKind, area: Area, doctrine: Doctrine) -> Zone {
    Zone {
        id,
        name: format!("Z{id}"),
        side: SideId(1),
        kind,
        area,
        doctrine,
        choke_point: None,
        shooters: vec![],
        layer: None,
        sensors: vec![],
        coverage_goal: None,
        assets: vec![],
    }
}

fn facility(
    id: u32,
    kind: FacilityKind,
    at: GeoPoint,
    services: Services,
    berths: u32,
) -> Facility {
    Facility {
        id,
        name: format!("F{id}"),
        side: SideId(1),
        kind,
        position: at,
        radius_m: 2_000.0,
        range_m: None,
        berths,
        services,
        unit: None,
        subordinates: vec![],
        alternate: None,
        handover_s: DEFAULT_HANDOVER_S,
        orders_in_gap: GapPolicy::Lose,
        line_of_sight: false,
        antenna_m: None,
        anti_jam_db: None,
        relays: Vec::new(),
        transfer_t_per_h: None,
        fuel_t: None,
        area: None,
        depth_m: None,
        max_sea_state: None,
        staging: false,
        air: None,
    }
}

fn planning(facilities: Vec<Facility>, zones: Vec<Zone>) -> Command {
    cmd(
        0,
        Issuer::Scenario,
        None,
        CommandPayload::SetPlanning {
            lanes: None,
            facilities,
            zones,
            action_groups: None,
        },
    )
}

/// `scene`, with a planning layer in force from tick 0.
fn planned_scene(range_m: f64, facilities: Vec<Facility>, zones: Vec<Zone>) -> Simulation {
    let mut sim = Simulation::new(1);
    sim.submit(planning(facilities, zones)).unwrap();
    sim.submit(cmd(
        0,
        Issuer::Scenario,
        None,
        CommandPayload::SetWeaponCatalog {
            weapons: vec![asm(), gun(), sam()],
        },
    ))
    .unwrap();
    sim.submit(unit(
        1,
        1,
        ORIGIN,
        vec![radar()],
        &[("T-ASM", 2), ("T-GUN", 100), ("T-SAM", 4)],
        None,
    ))
    .unwrap();
    sim.submit(unit(
        2,
        2,
        direct(ORIGIN, 90.0, range_m).point,
        vec![],
        &[],
        None,
    ))
    .unwrap();
    sim.run_ticks(1);
    sim
}

fn wcs(w: Wcs) -> Doctrine {
    Doctrine {
        wcs: Some(w),
        ..Doctrine::default()
    }
}

#[test]
fn an_engage_zone_sets_the_rules_for_targets_inside_it() {
    let red = direct(ORIGIN, 90.0, 30_000.0).point;
    // Weapons tight (the default): refused. An engage zone round the target, weapons free:
    // accepted.
    let mut sim = planned_scene(30_000.0, vec![], vec![]);
    sim.submit(engage(1, "T-ASM", 1)).unwrap();
    sim.step();
    assert!(rejected(&sim)[0].contains("weapons control status"));
    let free_box = zone(1, ZoneKind::Engage, circle(red, 5_000.0), wcs(Wcs::Free));
    let mut sim = planned_scene(30_000.0, vec![], vec![free_box.clone()]);
    sim.submit(engage(1, "T-ASM", 1)).unwrap();
    sim.step();
    assert!(rejected(&sim).is_empty(), "{:?}", rejected(&sim));
    // A zone elsewhere does nothing.
    let elsewhere = zone(1, ZoneKind::Engage, circle(ORIGIN, 5_000.0), wcs(Wcs::Free));
    let mut sim = planned_scene(30_000.0, vec![], vec![elsewhere]);
    sim.submit(engage(1, "T-ASM", 1)).unwrap();
    sim.step();
    assert_eq!(rejected(&sim).len(), 1);
    // A weapons-safe zone holds fire even when the unit itself is weapons free.
    let safe_box = zone(1, ZoneKind::Engage, circle(red, 5_000.0), wcs(Wcs::Safe));
    let mut sim = planned_scene(30_000.0, vec![], vec![safe_box]);
    sim.submit(weapons_free(1)).unwrap();
    sim.submit(engage(1, "T-ASM", 1)).unwrap();
    sim.step();
    assert!(rejected(&sim)[0].contains("weapons control status"));
}

#[test]
fn a_sense_zone_sets_emcon_for_own_units_and_alerts_on_new_contacts() {
    let red = direct(ORIGIN, 90.0, 30_000.0).point;
    // A sense zone round the red ship: Blue's first contact there raises an alert.
    let watch = zone(
        1,
        ZoneKind::Sense,
        circle(red, 5_000.0),
        Doctrine::default(),
    );
    let mut sim = planned_scene(30_000.0, vec![], vec![watch]);
    sim.run_ticks(10);
    let alerts: Vec<_> = sim
        .events()
        .iter()
        .filter(|e| {
            matches!(
                e.kind,
                SimEventKind::ZoneAlert {
                    zone: 1,
                    contact: 1,
                    ..
                }
            )
        })
        .collect();
    assert_eq!(alerts.len(), 1);
    let blue = sim.snapshot(Viewer::Side { side: SideId(1) });
    assert!(blue.messages.iter().any(|m| m.text == "C1 detected in Z1"));
    assert!(sim
        .snapshot(Viewer::Side { side: SideId(2) })
        .messages
        .iter()
        .all(|m| !m.text.contains("Z1")));
    // A silent sense zone round Blue: its radar is off, so nothing is detected.
    let quiet = zone(
        1,
        ZoneKind::Sense,
        circle(ORIGIN, 5_000.0),
        Doctrine {
            emcon: Some(Emcon::Silent),
            ..Doctrine::default()
        },
    );
    let mut sim = planned_scene(30_000.0, vec![], vec![quiet]);
    sim.run_ticks(10);
    let blue = sim.snapshot(Viewer::Side { side: SideId(1) });
    assert_eq!(blue.units[0].doctrine.emcon, Emcon::Silent);
    assert!(blue.contacts.is_empty());
}

#[test]
fn command_posts_limit_player_orders_to_units_in_range() {
    let speed = |issuer: Issuer, side: Option<u16>| {
        cmd(
            1,
            issuer,
            side,
            CommandPayload::SetSpeed {
                unit: UnitId(1),
                speed_mps: 3.0,
            },
        )
    };
    let mut near = facility(1, FacilityKind::CommandPost, ORIGIN, Services::default(), 0);
    near.range_m = Some(10_000.0);
    let mut sim = planned_scene(30_000.0, vec![near], vec![]);
    sim.submit(speed(Issuer::Player { seat: 1 }, Some(1)))
        .unwrap();
    sim.step();
    assert!(rejected(&sim).is_empty());

    let far_at = direct(ORIGIN, 0.0, 200_000.0).point;
    let mut far = facility(1, FacilityKind::CommandPost, far_at, Services::default(), 0);
    far.range_m = Some(50_000.0);
    let mut sim = planned_scene(30_000.0, vec![far], vec![]);
    sim.submit(speed(Issuer::Player { seat: 1 }, Some(1)))
        .unwrap();
    sim.submit(speed(umpire(), None)).unwrap();
    sim.step();
    let r = rejected(&sim);
    assert_eq!(r.len(), 1, "the white cell is not limited: {r:?}");
    assert!(
        r[0].contains("cannot be reached from F1: out of range"),
        "{r:?}"
    );
}

#[test]
fn ports_repair_and_rearm_units_alongside_up_to_their_berths() {
    let port = facility(
        1,
        FacilityKind::ShelteringPort,
        ORIGIN,
        Services {
            repair: true,
            rearm: true,
            refuel: true,
        },
        1,
    );
    let mut sim = planned_scene(30_000.0, vec![port], vec![]);
    // A second Blue ship alongside too: only one berth.
    let mut second = unit(
        3,
        1,
        direct(ORIGIN, 0.0, 500.0).point,
        vec![],
        &[("T-GUN", 10)],
        None,
    );
    second.tick = Tick(1);
    sim.submit(second).unwrap();
    for (u, damage) in [(1, 120.0), (3, 60.0)] {
        sim.submit(cmd(
            1,
            umpire(),
            None,
            CommandPayload::Adjudicate {
                reason: "test damage".into(),
                action: Adjudication::SetDamage {
                    unit: UnitId(u),
                    damage_dp: damage,
                    clear_effects: false,
                },
            },
        ))
        .unwrap();
    }
    sim.submit(cmd(
        1,
        umpire(),
        None,
        CommandPayload::Adjudicate {
            reason: "test stores".into(),
            action: Adjudication::SetStores {
                unit: UnitId(1),
                weapon: "T-ASM".into(),
                count: 0,
            },
        },
    ))
    .unwrap();
    // A sensors critical knocked out the radar (as a hit would).
    sim.run_ticks(2);
    let mut save = sim.save();
    let rec = save
        .units
        .iter_mut()
        .find(|r| r.unit.id == UnitId(1))
        .unwrap();
    rec.health.as_mut().unwrap().criticals = vec![System::Sensors];
    rec.sensors.0.clear();
    let mut sim = Simulation::from_save(save).unwrap();

    let damage = |sim: &mut Simulation, id: u64| {
        sim.snapshot(Viewer::WhiteCell)
            .units
            .iter()
            .find(|u| u.id == UnitId(id))
            .map(|u| u.damage_fraction)
            .unwrap()
    };
    sim.run_until(Tick(20 * 60));
    // 2 % of 300 DP a minute: 120 DP takes 20 minutes.
    assert!(damage(&mut sim, 1) < 0.1, "{}", damage(&mut sim, 1));
    assert!(
        (damage(&mut sim, 3) - 0.2).abs() < 1e-9,
        "unit 3 waits for a berth"
    );
    sim.run_until(Tick(31 * 60));
    let blue = sim.snapshot(Viewer::WhiteCell);
    let u1 = blue.units.iter().find(|u| u.id == UnitId(1)).unwrap();
    assert_eq!(u1.damage_fraction, 0.0);
    assert!(u1.criticals.is_empty());
    assert!(u1
        .stores
        .iter()
        .any(|s| s.weapon == "T-ASM" && s.count == 2));
    let ev = sim.events();
    assert!(ev.iter().any(|e| matches!(
        e.kind,
        SimEventKind::UnitRepaired {
            unit: UnitId(1),
            facility: 1
        }
    )));
    assert!(ev.iter().any(|e| matches!(
        e.kind,
        SimEventKind::UnitRearmed {
            unit: UnitId(1),
            facility: 1
        }
    )));
    // The radar is back.
    let save = sim.save();
    assert!(!save.units[0].sensors.0.is_empty());
    // Service state survives save and resume exactly.
    let mut resumed = Simulation::from_save(save).unwrap();
    sim.run_ticks(600);
    resumed.run_ticks(600);
    assert_eq!(sim.state_hash(), resumed.state_hash());
}

#[test]
fn only_privileged_issuers_set_the_planning_layer_and_bad_layers_are_refused() {
    let mut sim = Simulation::new(1);
    sim.submit(cmd(
        0,
        Issuer::Player { seat: 1 },
        Some(1),
        CommandPayload::SetPlanning {
            lanes: None,
            facilities: vec![],
            zones: vec![zone(
                1,
                ZoneKind::Sense,
                circle(ORIGIN, 1_000.0),
                Doctrine::default(),
            )],
            action_groups: None,
        },
    ))
    .unwrap();
    sim.step();
    assert!(rejected(&sim)[0].contains("planning layer"));
    for bad in [
        planning(
            vec![],
            vec![zone(
                1,
                ZoneKind::Sense,
                circle(ORIGIN, 0.0),
                Doctrine::default(),
            )],
        ),
        planning(
            vec![],
            vec![
                zone(
                    1,
                    ZoneKind::Sense,
                    circle(ORIGIN, 10.0),
                    Doctrine::default(),
                ),
                zone(
                    1,
                    ZoneKind::Engage,
                    circle(ORIGIN, 10.0),
                    Doctrine::default(),
                ),
            ],
        ),
        planning(
            vec![facility(
                1,
                FacilityKind::Anchorage,
                GeoPoint::new(95.0, 0.0),
                Services::default(),
                0,
            )],
            vec![],
        ),
    ] {
        assert!(Simulation::new(1).submit(bad).is_err());
    }
}

fn speed_order(tick: u64, unit: u64) -> Command {
    cmd(
        tick,
        Issuer::Player { seat: 1 },
        Some(1),
        CommandPayload::SetSpeed {
            unit: UnitId(unit),
            speed_mps: 3.0,
        },
    )
}

fn destroy(tick: u64, unit: u64) -> Command {
    cmd(
        tick,
        umpire(),
        None,
        CommandPayload::Adjudicate {
            reason: "test".into(),
            action: Adjudication::DestroyUnit { unit: UnitId(unit) },
        },
    )
}

fn ordered_speed(sim: &mut Simulation, unit: u64) -> f64 {
    sim.snapshot(Viewer::WhiteCell)
        .units
        .iter()
        .find(|u| u.id == UnitId(unit))
        .map(|u| u.ordered_speed_mps)
        .unwrap()
}

/// Blue unit 1 at the origin, a flagship (unit 3) carrying Blue's command post 20 km north,
/// and an alternate post ashore 50 km west, taking over after 120 s.
fn command_scene(gap: GapPolicy) -> Simulation {
    let mut afloat = facility(1, FacilityKind::CommandPost, ORIGIN, Services::default(), 0);
    afloat.unit = Some(UnitId(3));
    afloat.range_m = Some(100_000.0);
    afloat.alternate = Some(2);
    afloat.handover_s = 120.0;
    afloat.orders_in_gap = gap;
    let mut ashore = facility(
        2,
        FacilityKind::CommandPost,
        direct(ORIGIN, 270.0, 50_000.0).point,
        Services::default(),
        0,
    );
    ashore.range_m = Some(100_000.0);
    // The alternate commands only unit 1 until it takes over the flagship's subordinates.
    ashore.subordinates = vec![UnitId(99)];
    let mut sim = planned_scene(30_000.0, vec![afloat, ashore], vec![]);
    let mut flagship = unit(3, 1, direct(ORIGIN, 0.0, 20_000.0).point, vec![], &[], None);
    flagship.tick = Tick(1);
    sim.submit(flagship).unwrap();
    sim.step();
    sim
}

#[test]
fn a_lost_command_post_hands_over_to_its_alternate_after_the_delay() {
    let mut sim = command_scene(GapPolicy::Lose);
    sim.submit(speed_order(2, 1)).unwrap();
    sim.step();
    assert!(rejected(&sim).is_empty(), "the flagship's post is up");
    sim.submit(destroy(3, 3)).unwrap();
    sim.run_ticks(2);
    assert!(sim
        .events()
        .iter()
        .any(|e| matches!(e.kind, SimEventKind::CommandPostDown { facility: 1 })));
    sim.submit(speed_order(5, 1)).unwrap();
    sim.step();
    let r = rejected(&sim);
    assert!(
        r.last()
            .unwrap()
            .contains("is down; command passes to F2 at 00:02:03"),
        "{r:?}"
    );
    // After the handover the alternate commands the flagship's subordinates.
    sim.run_until(Tick(124));
    sim.submit(speed_order(124, 1)).unwrap();
    sim.step();
    assert_eq!(rejected(&sim).len(), r.len(), "accepted after the handover");
    assert!((ordered_speed(&mut sim, 1) - 3.0).abs() < 1e-9);
}

#[test]
fn orders_in_the_gap_can_be_queued_for_the_alternate() {
    let mut sim = command_scene(GapPolicy::Queue);
    sim.submit(destroy(2, 3)).unwrap();
    sim.run_ticks(2);
    sim.submit(speed_order(4, 1)).unwrap();
    sim.step();
    assert!(rejected(&sim)
        .last()
        .unwrap()
        .starts_with("queued: F1 is down"));
    assert!(ordered_speed(&mut sim, 1) < 1e-9);
    sim.run_until(Tick(125));
    assert!(sim
        .events()
        .iter()
        .any(|e| matches!(e.kind, SimEventKind::QueuedOrderApplied { .. })));
    assert!((ordered_speed(&mut sim, 1) - 3.0).abs() < 1e-9);
    // The run replays exactly from its command log.
    let mut replay = Simulation::new(1);
    for c in sim.command_log() {
        replay.submit(c.command).unwrap();
    }
    replay.run_until(sim.tick());
    assert_eq!(replay.state_hash(), sim.state_hash());
}

#[test]
fn a_command_post_commands_only_its_subordinates() {
    let mut post = facility(1, FacilityKind::CommandPost, ORIGIN, Services::default(), 0);
    post.range_m = Some(100_000.0);
    post.subordinates = vec![UnitId(7)];
    let mut sim = planned_scene(30_000.0, vec![post], vec![]);
    sim.submit(speed_order(1, 1)).unwrap();
    sim.step();
    assert!(rejected(&sim)[0].contains("not assigned to a command post"));
}

#[test]
fn engage_zone_shooters_engage_contacts_inside_on_their_own() {
    let red = direct(ORIGIN, 90.0, 30_000.0).point;
    let mut kill_box = zone(
        1,
        ZoneKind::Engage,
        circle(red, 5_000.0),
        Doctrine {
            wcs: Some(Wcs::Free),
            ..Doctrine::default()
        },
    );
    kill_box.shooters = vec![UnitId(1)];
    kill_box.choke_point = Some("Narrows".into());
    let mut sim = planned_scene(30_000.0, vec![], vec![kill_box]);
    sim.run_until(Tick(30));
    let ev = sim.events();
    assert!(ev.iter().any(|e| matches!(
        &e.kind,
        SimEventKind::ZoneEngage { zone: 1, unit: UnitId(1), contact: 1, weapon } if weapon == "T-ASM"
    )));
    assert!(ev
        .iter()
        .any(|e| matches!(e.kind, SimEventKind::WeaponLaunched { .. })));
    // The red ship inside the choke point is logged once.
    let transits: Vec<_> = ev
        .iter()
        .filter(|e| matches!(&e.kind, SimEventKind::ChokePointTransit { unit: UnitId(2), choke_point } if choke_point == "Narrows"))
        .collect();
    assert_eq!(transits.len(), 1);
}

#[test]
fn a_mission_killed_unit_diverts_to_port_is_repaired_and_resumes_its_mission() {
    let port_at = direct(ORIGIN, 0.0, 8_000.0).point;
    let port = facility(
        1,
        FacilityKind::ShelteringPort,
        port_at,
        Services {
            repair: true,
            rearm: false,
            refuel: false,
        },
        0,
    );
    let mut sim = planned_scene(80_000.0, vec![port], vec![]);
    let patrol = Mission::Patrol {
        centre: ORIGIN,
        radius_m: 5_000.0,
    };
    for c in [
        cmd(
            1,
            Issuer::Scenario,
            None,
            CommandPayload::AssignMission {
                unit: UnitId(1),
                mission: Some(patrol.clone()),
            },
        ),
        cmd(
            1,
            umpire(),
            None,
            CommandPayload::Adjudicate {
                reason: "test".into(),
                action: Adjudication::SetDamage {
                    unit: UnitId(1),
                    damage_dp: 150.0,
                    clear_effects: false,
                },
            },
        ),
    ] {
        sim.submit(c).unwrap();
    }
    sim.run_until(Tick(61));
    assert!(sim.events().iter().any(|e| matches!(
        e.kind,
        SimEventKind::DivertedToPort {
            unit: UnitId(1),
            facility: 1
        }
    )));
    sim.run_until(Tick(4 * 3600));
    let ev = sim.events();
    assert!(ev.iter().any(|e| matches!(
        e.kind,
        SimEventKind::UnitRepaired {
            unit: UnitId(1),
            facility: 1
        }
    )));
    let save = sim.save();
    let u1 = save.units.iter().find(|r| r.unit.id == UnitId(1)).unwrap();
    assert_eq!(u1.mission.mission, Some(patrol));
    assert!(save.planning.diverted.is_empty());
}

// ---- Comms links (Phase 3 slice 10; docs/models/comms.md) ----

fn post(id: u32, at: GeoPoint, range_m: f64) -> Facility {
    let mut f = facility(id, FacilityKind::CommandPost, at, Services::default(), 0);
    f.range_m = Some(range_m);
    f
}

fn comms_of(sim: &mut Simulation, unit: u64) -> CommsView {
    sim.snapshot(Viewer::Side { side: SideId(1) })
        .comms
        .into_iter()
        .find(|c| c.unit == UnitId(unit))
        .unwrap()
}

fn with_jammer(mut c: Command, jn_db_at_1km: f64) -> Command {
    if let CommandPayload::SpawnUnit(s) = &mut c.payload {
        s.jammer = Some(JammerSpec { jn_db_at_1km });
    }
    c
}

#[test]
fn a_line_of_sight_post_reaches_only_inside_the_radio_horizon_or_through_a_relay() {
    // Post ashore (30 m antenna) 60 km north of unit 1 (25 m mast): horizon
    // 4.12 km × (√30 + √25) = 43.2 km, so the direct link fails.
    let at = direct(ORIGIN, 0.0, 60_000.0).point;
    let mut los = post(1, at, 200_000.0);
    los.line_of_sight = true;
    let mut sim = planned_scene(30_000.0, vec![los.clone()], vec![]);
    let c = comms_of(&mut sim, 1);
    assert_eq!(c.status, LinkStatus::BeyondHorizon);
    sim.submit(speed_order(1, 1)).unwrap();
    sim.step();
    let r = rejected(&sim);
    assert!(
        r.last().unwrap().contains("beyond the radio horizon"),
        "{r:?}"
    );

    // The default link (HF or satellite) is not horizon-limited.
    let mut sim = planned_scene(30_000.0, vec![post(1, at, 200_000.0)], vec![]);
    assert_eq!(comms_of(&mut sim, 1).status, LinkStatus::Up);

    // A relay 30 km north is within 43 km of both: the order goes through it.
    los.relays = vec![UnitId(3)];
    let mut sim = planned_scene(30_000.0, vec![los], vec![]);
    let mut relay = unit(3, 1, direct(ORIGIN, 0.0, 30_000.0).point, vec![], &[], None);
    relay.tick = Tick(1);
    sim.submit(relay).unwrap();
    sim.step();
    let c = comms_of(&mut sim, 1);
    assert_eq!((c.status, c.via), (LinkStatus::Up, Some(UnitId(3))));
    sim.submit(speed_order(2, 1)).unwrap();
    sim.step();
    assert!(rejected(&sim).is_empty(), "{:?}", rejected(&sim));
}

#[test]
fn jamming_cuts_links_and_a_jammed_post_hands_over_then_comes_back() {
    // Blue post 100 km north of unit 1; a red jammer (60 dB at 1 km) 30 km east of unit 1.
    // J/S at unit 1 = 60 − 60 − 10 + 20·log10(100 / 30) = +0.46 dB: jammed.
    let at = direct(ORIGIN, 0.0, 100_000.0).point;
    let mut sim = Simulation::new(1);
    sim.submit(planning(vec![post(1, at, 150_000.0)], vec![]))
        .unwrap();
    sim.submit(unit(1, 1, ORIGIN, vec![], &[], None)).unwrap();
    sim.submit(with_jammer(
        unit(
            2,
            2,
            direct(ORIGIN, 90.0, 30_000.0).point,
            vec![],
            &[],
            None,
        ),
        60.0,
    ))
    .unwrap();
    sim.step();
    assert_eq!(comms_of(&mut sim, 1).status, LinkStatus::Jammed);
    sim.submit(speed_order(1, 1)).unwrap();
    sim.step();
    assert!(rejected(&sim).last().unwrap().contains("link jammed"));
    // At 40 km: −10 + 20·log10(100 / 40) = −2.04 dB, so the link holds.
    sim.submit(cmd(
        2,
        umpire(),
        None,
        CommandPayload::Adjudicate {
            reason: "test".into(),
            action: Adjudication::MoveUnit {
                unit: UnitId(2),
                position: direct(ORIGIN, 90.0, 40_000.0).point,
                altitude_m: None,
            },
        },
    ))
    .unwrap();
    sim.step();
    assert_eq!(comms_of(&mut sim, 1).status, LinkStatus::Up);

    // Now jam the post itself: 20 km from it, J/S at the post for a unit at the edge of its
    // 150 km range = −10 + 20·log10(150 / 20) = +7.5 dB. It goes down.
    let mut alt = post(2, direct(ORIGIN, 180.0, 50_000.0).point, 150_000.0);
    alt.subordinates = vec![UnitId(99)];
    let mut main = post(1, at, 150_000.0);
    main.alternate = Some(2);
    main.handover_s = 60.0;
    let mut sim = Simulation::new(1);
    sim.submit(planning(vec![main, alt], vec![])).unwrap();
    sim.submit(unit(1, 1, ORIGIN, vec![], &[], None)).unwrap();
    sim.submit(with_jammer(
        unit(2, 2, direct(at, 90.0, 20_000.0).point, vec![], &[], None),
        60.0,
    ))
    .unwrap();
    sim.run_ticks(2);
    assert!(sim.events().iter().any(|e| matches!(
        e.kind,
        SimEventKind::CommandPostDisrupted {
            facility: 1,
            status: LinkStatus::Jammed
        }
    )));
    let posts = sim.snapshot(Viewer::Side { side: SideId(1) }).posts;
    assert!(!posts[0].up && posts[0].status == LinkStatus::Jammed);
    sim.submit(speed_order(2, 1)).unwrap();
    sim.step();
    assert!(rejected(&sim)
        .last()
        .unwrap()
        .contains("command passes to F2"));
    // After the handover the alternate commands unit 1.
    sim.run_until(Tick(61));
    sim.submit(speed_order(61, 1)).unwrap();
    sim.step();
    assert_eq!(rejected(&sim).len(), 1);
    // The jammer falls silent: the post comes back.
    sim.submit(cmd(
        62,
        umpire(),
        None,
        CommandPayload::SetDoctrine {
            scope: DoctrineScope::Unit { unit: UnitId(2) },
            doctrine: Doctrine {
                emcon: Some(Emcon::Silent),
                ..Doctrine::default()
            },
        },
    ))
    .unwrap();
    sim.run_ticks(2);
    assert!(sim
        .events()
        .iter()
        .any(|e| matches!(e.kind, SimEventKind::CommandPostRestored { facility: 1 })));
    assert!(sim.snapshot(Viewer::WhiteCell).posts[0].up);
}

#[test]
fn deep_submarines_and_knocked_out_comms_cannot_receive() {
    let mut sim = planned_scene(30_000.0, vec![post(1, ORIGIN, 100_000.0)], vec![]);
    let mut sub = unit(3, 1, direct(ORIGIN, 0.0, 10_000.0).point, vec![], &[], None);
    if let CommandPayload::SpawnUnit(s) = &mut sub.payload {
        s.domain = Domain::Subsurface;
        s.altitude_m = -100.0;
    }
    sub.tick = Tick(1);
    sim.submit(sub).unwrap();
    sim.step();
    assert_eq!(comms_of(&mut sim, 3).status, LinkStatus::Submerged);
    assert_eq!(comms_of(&mut sim, 1).status, LinkStatus::Up);
}

// ---- Fuel and refuelling (Phase 3 slice 11; docs/models/fuel.md) ----

fn fuelled(mut c: Command, capacity_t: f64, endurance_m: f64, level: f64) -> Command {
    if let CommandPayload::SpawnUnit(s) = &mut c.payload {
        s.fuel = Some(FuelSpec {
            capacity_t,
            endurance_m,
            cruise_mps: 10.0,
            level,
            bingo: None,
            kind: Default::default(),
            idle_fraction: None,
        });
    }
    c
}

fn with_mission(mut c: Command, mission: Mission) -> Command {
    if let CommandPayload::SpawnUnit(s) = &mut c.payload {
        s.mission = Some(mission);
    }
    c
}

fn unit_view(sim: &mut Simulation, unit: u64) -> UnitView {
    sim.snapshot(Viewer::WhiteCell)
        .units
        .into_iter()
        .find(|u| u.id == UnitId(unit))
        .unwrap()
}

fn fired(sim: &Simulation, f: impl Fn(&SimEventKind) -> bool) -> Vec<u64> {
    sim.events()
        .iter()
        .filter(|e| f(&e.kind))
        .map(|e| e.tick.0)
        .collect()
}

#[test]
fn fuel_burns_by_speed_and_a_dry_ship_coasts_to_a_stop() {
    // 1 t for 10 km at a 10 m/s cruise: 0.001 t/s at cruise, so empty after 1 000 s.
    let mut sim = Simulation::new(1);
    let mut c = fuelled(unit(1, 1, ORIGIN, vec![], &[], None), 1.0, 10_000.0, 1.0);
    if let CommandPayload::SpawnUnit(s) = &mut c.payload {
        s.speed_mps = 10.0;
    }
    sim.submit(c).unwrap();
    sim.run_ticks(500);
    let f = unit_view(&mut sim, 1).fuel_fraction.unwrap();
    assert!((f - 0.5).abs() < 1e-9, "{f}");
    sim.run_until(Tick(1_001));
    let dry = fired(&sim, |k| matches!(k, SimEventKind::FuelExhausted { .. }));
    assert_eq!(dry.len(), 1);
    assert!((998..=1_000).contains(&dry[0]), "{dry:?}");
    // It decelerates at 0.1 m/s² from 10 m/s: stopped 100 s later, its order kept.
    sim.run_until(Tick(1_150));
    let u = unit_view(&mut sim, 1);
    assert_eq!(u.speed_mps, 0.0);
    assert_eq!(u.ordered_speed_mps, 10.0);
    assert_eq!(u.fuel_fraction, Some(0.0));
}

#[test]
fn below_bingo_a_unit_refuels_at_the_nearest_depot_and_resumes_its_mission() {
    let mut depot = facility(
        1,
        FacilityKind::RefuellingPoint,
        direct(ORIGIN, 0.0, 5_000.0).point,
        Services {
            refuel: true,
            ..Services::default()
        },
        0,
    );
    depot.transfer_t_per_h = Some(3_600.0); // 60 t a service period
    depot.fuel_t = Some(1_000.0);
    let patrol = Mission::Patrol {
        centre: ORIGIN,
        radius_m: 1_000.0,
    };
    let mut sim = Simulation::new(1);
    sim.submit(planning(vec![depot], vec![])).unwrap();
    // 100 t at 20 %: below the 30 % bingo.
    sim.submit(with_mission(
        fuelled(unit(1, 1, ORIGIN, vec![], &[], None), 100.0, 1.0e9, 0.2),
        patrol.clone(),
    ))
    .unwrap();
    sim.run_until(Tick(1_800));
    let diverted = fired(
        &sim,
        |k| matches!(k, SimEventKind::DivertedForFuel { unit, facility: 1 } if *unit == UnitId(1)),
    );
    assert_eq!(diverted, vec![0], "checked every 60 s, from tick 0");
    let full = fired(&sim, |k| matches!(k, SimEventKind::UnitRefuelled { .. }));
    assert_eq!(full.len(), 1, "refuelled once");
    let u = unit_view(&mut sim, 1);
    assert_eq!(u.mission, Some(patrol), "mission resumed");
    assert!(u.fuel_fraction.unwrap() > 0.999);
    // About 80 t issued (20 % → 100 %, plus what it burned on the way).
    let save = sim.save();
    let issued = save.planning.issued[&1];
    assert!((80.0..81.0).contains(&issued), "{issued}");
}

#[test]
fn an_oiler_under_way_refuels_one_ship_at_a_time_and_the_other_waits() {
    // Oiler (unit 5) steaming north at 6 m/s, one berth; two ships at 20 % 3 km astern.
    let mut ras = facility(
        1,
        FacilityKind::RefuellingPoint,
        ORIGIN,
        Services {
            refuel: true,
            ..Services::default()
        },
        1,
    );
    ras.unit = Some(UnitId(5));
    ras.radius_m = 1_500.0;
    ras.transfer_t_per_h = Some(3_600.0);
    let mut sim = Simulation::new(1);
    sim.submit(planning(vec![ras], vec![])).unwrap();
    let mut oiler = unit(5, 1, ORIGIN, vec![], &[], None);
    if let CommandPayload::SpawnUnit(s) = &mut oiler.payload {
        s.speed_mps = 6.0;
        s.route = vec![direct(ORIGIN, 0.0, 200_000.0).point];
    }
    sim.submit(oiler).unwrap();
    let astern = direct(ORIGIN, 180.0, 3_000.0).point;
    for id in [3, 4] {
        let at = direct(astern, 90.0, f64::from(id as u32 - 3) * 300.0).point;
        sim.submit(with_mission(
            fuelled(unit(id, 1, at, vec![], &[], None), 100.0, 1.0e9, 0.2),
            Mission::Ferry {
                destination: direct(ORIGIN, 0.0, 300_000.0).point,
            },
        ))
        .unwrap();
    }
    sim.run_until(Tick(3_600));
    let refuelled: Vec<(u64, UnitId)> = sim
        .events()
        .iter()
        .filter_map(|e| match e.kind {
            SimEventKind::UnitRefuelled { unit, facility: 1 } => Some((e.tick.0, unit)),
            _ => None,
        })
        .collect();
    assert_eq!(refuelled.len(), 2, "{refuelled:?}");
    assert_eq!(refuelled[0].1, UnitId(3), "lowest id first");
    assert!(refuelled[0].0 < refuelled[1].0);
    let waited = fired(
        &sim,
        |k| matches!(k, SimEventKind::WaitingForBerth { unit, .. } if *unit == UnitId(4)),
    );
    assert!(!waited.is_empty());
    // Both under way the whole time, at the oiler's speed.
    let u3 = unit_view(&mut sim, 3);
    assert!(u3.speed_mps > 5.0, "{}", u3.speed_mps);
}

#[test]
fn fuel_survives_save_and_resume() {
    let build = || {
        let mut sim = Simulation::new(7);
        let mut c = fuelled(unit(1, 1, ORIGIN, vec![], &[], None), 5.0, 50_000.0, 0.9);
        if let CommandPayload::SpawnUnit(s) = &mut c.payload {
            s.speed_mps = 12.0;
        }
        sim.submit(c).unwrap();
        sim
    };
    let mut a = build();
    a.run_ticks(300);
    let mut b = Simulation::from_save(a.save()).unwrap();
    a.run_ticks(300);
    b.run_ticks(300);
    assert_eq!(a.state_hash(), b.state_hash());
    assert_eq!(
        unit_view(&mut a, 1).fuel_fraction,
        unit_view(&mut b, 1).fuel_fraction
    );
}

// ---- More missions (Phase 3 slice 12; docs/models/missions.md) ----

#[test]
fn a_support_unit_holds_its_station_and_engages_nothing() {
    let mut sim = scene(34, 30_000.0, None);
    sim.submit(weapons_free(1)).unwrap();
    let station = direct(ORIGIN, 0.0, 8_000.0).point;
    sim.submit(assign(
        1,
        Issuer::Player { seat: 1 },
        Some(1),
        1,
        Some(Mission::Support {
            station,
            radius_m: 2_000.0,
        }),
    ))
    .unwrap();
    sim.run_ticks(1_500);
    let u = unit_view(&mut sim, 1);
    assert!(sim_geo::inverse(u.position, station).distance_m <= 2_000.0);
    assert_eq!(u.speed_mps, 0.0, "holding");
    assert!(
        mission_events(&sim).2.is_empty(),
        "support units do not engage"
    );
}

#[test]
fn a_mining_mission_lays_its_field_at_the_centre_and_completes() {
    let mut sim = Simulation::new(3);
    sim.submit(cmd(
        0,
        Issuer::Scenario,
        None,
        CommandPayload::SetWeaponCatalog {
            weapons: vec![mine()],
        },
    ))
    .unwrap();
    let centre = direct(ORIGIN, 90.0, 4_000.0).point;
    sim.submit(with_mission(
        unit(1, 1, ORIGIN, vec![], &[("T-MINE", 12)], None),
        Mission::Mining {
            centre,
            radius_m: 1_500.0,
            mines: 20,
        },
    ))
    .unwrap();
    sim.run_ticks(1_200);
    let laid: Vec<(u32, u32)> = sim
        .events()
        .iter()
        .filter_map(|e| match e.kind {
            SimEventKind::MissionMinesLaid { field, mines, .. } => Some((field, mines)),
            _ => None,
        })
        .collect();
    assert_eq!(laid, vec![(1, 12)], "all it carried, fewer than ordered");
    let fields = sim.snapshot(Viewer::Side { side: SideId(1) }).minefields;
    assert_eq!(fields.len(), 1);
    assert_eq!((fields[0].mines, fields[0].radius_m), (12, 1_500.0));
    assert!(sim_geo::inverse(fields[0].centre, centre).distance_m <= 1_000.0);
    let u = unit_view(&mut sim, 1);
    assert!(u.mission_complete && u.stores.is_empty());
}

#[test]
fn mine_clearing_sweeps_mines_at_a_tenth_of_the_risk() {
    let mut sim = Simulation::new(11);
    sim.submit(cmd(
        0,
        Issuer::Scenario,
        None,
        CommandPayload::SetWeaponCatalog {
            weapons: vec![mine()],
        },
    ))
    .unwrap();
    sim.submit(cmd(
        0,
        Issuer::Scenario,
        None,
        CommandPayload::CreateMinefield {
            side: SideId(2),
            centre: ORIGIN,
            radius_m: 2_000.0,
            mines: 40,
            weapon: "T-MINE".into(),
        },
    ))
    .unwrap();
    let mut mcm = with_mission(
        unit(1, 1, ORIGIN, vec![], &[], None),
        Mission::MineClearing {
            centre: ORIGIN,
            radius_m: 2_000.0,
        },
    );
    if let CommandPayload::SpawnUnit(s) = &mut mcm.payload {
        s.damage_profile = Some(DamageProfile {
            damage_points: 100_000.0,
            mission_kill_fraction: 0.99,
            fire_susceptibility: 0.0,
            flooding_susceptibility: 0.0,
        });
    }
    sim.submit(mcm).unwrap();
    sim.run_ticks(7_200);
    let mut sweeps = Vec::new();
    let mut risks = Vec::new();
    for e in sim.events() {
        match e.kind {
            SimEventKind::MineSwept { p, swept, .. } => sweeps.push((e.tick, p, swept)),
            SimEventKind::MineEncounter { p, .. } => risks.push((e.tick, p)),
            _ => {}
        }
    }
    let swept = sweeps.iter().filter(|s| s.2).count();
    assert!(swept >= 5, "{swept} swept in two hours");
    let left = sim.snapshot(Viewer::WhiteCell).minefields[0].mines;
    assert_eq!(left as usize, 40 - swept);
    // Each check: the sweep chance (W = 200 m) is far above the risk to the sweeper.
    for (t, p) in &risks {
        let sweep = sweeps.iter().find(|s| s.0 == *t).unwrap();
        assert!(*p < sweep.1, "risk {p} vs sweep {}", sweep.1);
    }
    let u = unit_view(&mut sim, 1);
    assert!(!u.mission_complete, "never completes on its own");
    assert!(u.speed_mps <= 4.0 + 1e-9, "sweeps at 8 kn");
}

#[test]
fn cargo_is_loaded_carried_and_delivered() {
    let origin = direct(ORIGIN, 0.0, 3_000.0).point;
    let destination = direct(ORIGIN, 0.0, 12_000.0).point;
    let mut sim = Simulation::new(5);
    sim.submit(with_mission(
        unit(1, 1, ORIGIN, vec![], &[], None),
        Mission::Cargo {
            origin: Some(origin),
            destination,
            tonnes: 800.0,
        },
    ))
    .unwrap();
    sim.run_ticks(6_000);
    let loaded = fired(&sim, |k| matches!(k, SimEventKind::CargoLoaded { .. }));
    let delivered: Vec<(u64, f64)> = sim
        .events()
        .iter()
        .filter_map(|e| match e.kind {
            SimEventKind::CargoDelivered { tonnes, .. } => Some((e.tick.0, tonnes)),
            _ => None,
        })
        .collect();
    assert_eq!(loaded.len(), 1);
    assert_eq!(delivered.len(), 1);
    assert_eq!(delivered[0].1, 800.0);
    // At least 30 minutes alongside at each end.
    assert!(loaded[0] >= 1_800);
    assert!(delivered[0].0 >= loaded[0] + 1_800);
    let u = unit_view(&mut sim, 1);
    assert!(u.mission_complete);
    assert!(sim_geo::inverse(u.position, destination).distance_m <= 1_000.0);
}

#[test]
fn a_barrier_patrol_runs_back_and_forth_between_its_ends() {
    let start = direct(ORIGIN, 0.0, 4_000.0).point;
    let end = direct(ORIGIN, 180.0, 4_000.0).point;
    let mut sim = Simulation::new(9);
    sim.submit(with_mission(
        unit(1, 1, ORIGIN, vec![], &[], None),
        Mission::BarrierPatrol { start, end },
    ))
    .unwrap();
    let mut legs = Vec::new();
    for _ in 0..360 {
        sim.run_ticks(10);
        let at = unit_view(&mut sim, 1).position;
        let near = if sim_geo::inverse(at, start).distance_m <= 1_000.0 {
            Some('S')
        } else if sim_geo::inverse(at, end).distance_m <= 1_000.0 {
            Some('E')
        } else {
            None
        };
        if let Some(c) = near {
            if legs.last() != Some(&c) {
                legs.push(c);
            }
        }
    }
    assert!(legs.len() >= 4, "{legs:?}");
    assert!(legs.windows(2).all(|w| w[0] != w[1]));
}

#[test]
fn denial_patrols_inside_the_choke_point_and_engages_only_inside_it() {
    let around = |c: GeoPoint| Area::Polygon {
        points: vec![
            direct(c, 315.0, 5_000.0).point,
            direct(c, 45.0, 5_000.0).point,
            direct(c, 135.0, 5_000.0).point,
            direct(c, 225.0, 5_000.0).point,
        ],
    };
    let run = |zone_at: GeoPoint| {
        let mut sim = scene(34, 30_000.0, None);
        sim.submit(weapons_free(1)).unwrap();
        let mut z = zone(1, ZoneKind::Sense, around(zone_at), Doctrine::default());
        z.choke_point = Some("Narrows".into());
        sim.submit(cmd(
            1,
            Issuer::Scenario,
            None,
            CommandPayload::SetPlanning {
                lanes: None,
                facilities: vec![],
                zones: vec![z],
                action_groups: None,
            },
        ))
        .unwrap();
        sim.submit(assign(
            1,
            Issuer::Player { seat: 1 },
            Some(1),
            1,
            Some(Mission::Denial {
                choke_point: "Narrows".into(),
            }),
        ))
        .unwrap();
        sim.run_ticks(120);
        sim
    };
    // The zone holds the red ship (30 km east): engaged.
    let red = direct(ORIGIN, 90.0, 30_000.0).point;
    let sim = run(red);
    let (points, _, engaged) = mission_events(&sim);
    assert_eq!(engaged, vec![(1, "T-ASM".to_owned())]);
    assert!(!points.is_empty());
    for p in &points {
        assert!(around(red).contains(*p), "waypoint {p:?} inside the zone");
    }
    // The zone is elsewhere: the red ship is not engaged.
    let sim = run(direct(ORIGIN, 0.0, 30_000.0).point);
    assert!(mission_events(&sim).2.is_empty());
}

// ---- Anchorages and weather (Phase 3 slice 13; docs/models/planning-layer.md) ----

fn square(centre: GeoPoint, half_m: f64) -> Area {
    Area::Polygon {
        points: [315.0, 45.0, 135.0, 225.0]
            .iter()
            .map(|b| direct(centre, *b, half_m * std::f64::consts::SQRT_2).point)
            .collect(),
    }
}

fn anchorage(id: u32, at: GeoPoint, depth_m: Option<f64>) -> Facility {
    let mut a = facility(id, FacilityKind::Anchorage, at, Services::default(), 0);
    a.area = Some(square(at, 1_500.0));
    a.depth_m = depth_m;
    a.staging = true;
    a
}

fn sea_state(tick: u64, sea_state: u8) -> Command {
    cmd(
        tick,
        umpire(),
        None,
        CommandPayload::SetEnvironment {
            environment: Environment {
                sea_state,
                ..Environment::default()
            },
        },
    )
}

fn ship(id: u64, at: GeoPoint, mission: Mission, draft_m: f64, length_m: f64) -> Command {
    let mut c = with_mission(unit(id, 1, at, vec![], &[], None), mission);
    if let CommandPayload::SpawnUnit(s) = &mut c.payload {
        s.draft_m = Some(draft_m);
        s.length_m = Some(length_m);
    }
    c
}

#[test]
fn units_hold_at_the_nearest_staging_anchorage_they_fit_between_missions() {
    // Two staging anchorages: 6 m of water 10 km north, 20 m 15 km south. A ship of 7 m draft
    // ferries 5 km east, then goes to the deeper one although it is further.
    let shallow = anchorage(1, direct(ORIGIN, 0.0, 10_000.0).point, Some(6.0));
    let deep_at = direct(ORIGIN, 180.0, 15_000.0).point;
    let deep = anchorage(2, deep_at, Some(20.0));
    let mut sim = Simulation::new(1);
    sim.submit(planning(vec![shallow, deep.clone()], vec![]))
        .unwrap();
    sim.submit(ship(
        1,
        ORIGIN,
        Mission::Ferry {
            destination: direct(ORIGIN, 90.0, 5_000.0).point,
        },
        7.0,
        135.0,
    ))
    .unwrap();
    sim.run_ticks(4_000);
    let held = fired(&sim, |k| {
        matches!(k, SimEventKind::HoldingAtAnchorage { facility: 2, .. })
    });
    assert_eq!(held.len(), 1, "to the deeper anchorage, once");
    let u = unit_view(&mut sim, 1);
    assert!(
        deep.area.as_ref().unwrap().contains(u.position),
        "inside the polygon"
    );
    assert_eq!(u.speed_mps, 0.0);
    // A new mission takes it away from the anchorage.
    sim.submit(assign(
        4_000,
        umpire(),
        None,
        1,
        Some(Mission::Ferry {
            destination: ORIGIN,
        }),
    ))
    .unwrap();
    sim.run_ticks(300);
    assert!(unit_view(&mut sim, 1).speed_mps > 1.0);
}

#[test]
fn heavy_weather_sends_ships_to_shelter_and_they_resume_when_it_eases() {
    let mut port = facility(
        1,
        FacilityKind::ShelteringPort,
        direct(ORIGIN, 270.0, 8_000.0).point,
        Services::default(),
        0,
    );
    port.depth_m = Some(12.0);
    let patrol = Mission::Patrol {
        centre: ORIGIN,
        radius_m: 3_000.0,
    };
    let mut sim = Simulation::new(2);
    sim.submit(planning(vec![port], vec![])).unwrap();
    // A 135 m frigate (limit sea state 8) and a 40 m patrol boat (limit 5).
    sim.submit(ship(1, ORIGIN, patrol.clone(), 5.0, 135.0))
        .unwrap();
    sim.submit(ship(2, ORIGIN, patrol.clone(), 2.0, 40.0))
        .unwrap();
    sim.submit(sea_state(100, 6)).unwrap();
    sim.run_until(Tick(1_500));
    let sheltering: Vec<UnitId> = sim
        .events()
        .iter()
        .filter_map(|e| match e.kind {
            SimEventKind::ShelteringFromWeather { unit, facility: 1 } => Some(unit),
            _ => None,
        })
        .collect();
    assert_eq!(
        sheltering,
        vec![UnitId(2)],
        "only the patrol boat runs at sea state 6"
    );
    assert_eq!(unit_view(&mut sim, 2).mission, None, "mission set aside");
    assert_eq!(unit_view(&mut sim, 1).mission, Some(patrol.clone()));
    sim.submit(sea_state(1_500, 3)).unwrap();
    sim.run_until(Tick(1_600));
    assert!(sim
        .events()
        .iter()
        .any(|e| matches!(e.kind, SimEventKind::WeatherEased { unit } if unit == UnitId(2))));
    assert_eq!(unit_view(&mut sim, 2).mission, Some(patrol));
}

#[test]
fn facility_areas_and_limits_are_checked() {
    let mut carried = anchorage(1, ORIGIN, None);
    carried.kind = FacilityKind::RefuellingPoint;
    carried.staging = false;
    carried.unit = Some(UnitId(1));
    let mut staging_port = facility(
        2,
        FacilityKind::ShelteringPort,
        ORIGIN,
        Services::default(),
        0,
    );
    staging_port.staging = true;
    let mut rough = anchorage(3, ORIGIN, Some(10.0));
    rough.max_sea_state = Some(12);
    let mut dry = anchorage(4, ORIGIN, Some(0.0));
    dry.staging = false;
    for bad in [carried, staging_port, rough, dry] {
        assert!(
            Simulation::new(1)
                .submit(planning(vec![bad], vec![]))
                .is_err(),
            "refused"
        );
    }
    assert_eq!(weather_limit(40.0), 5);
    assert_eq!(weather_limit(90.0), 7);
    assert_eq!(weather_limit(135.0), 8);
}

// ---- Sense-zone layers and coverage (Phase 3 slice 14) ----

#[test]
fn sense_zone_coverage_follows_the_radar_horizon_and_emcon() {
    // A 20 km sense zone centred 30 km east of unit 1 (25 m radar). Against the 10 m reference
    // ship the horizon is 4.12 km × (√25 + √10) = 33.6 km; the radar's R50 against 1 000 m² is
    // far beyond it, so the horizon is the edge of coverage.
    let centre = direct(ORIGIN, 90.0, 30_000.0).point;
    let mut z = zone(
        1,
        ZoneKind::Sense,
        circle(centre, 20_000.0),
        Doctrine::default(),
    );
    z.layer = Some(ZoneLayer::Outer);
    z.sensors = vec![SensorKind::Radar];
    let mut sim = planned_scene(80_000.0, vec![], vec![z.clone()]);
    let cov = sim.zone_coverage(1).unwrap();
    let horizon = sim_geo::radar_horizon_m(25.0, 10.0);
    assert!(cov.cells.len() > 300, "{} cells", cov.cells.len());
    for c in &cov.cells {
        let d = sim_geo::inverse(ORIGIN, c.centre).distance_m;
        assert_eq!(c.covered, d <= horizon, "cell {d:.0} m out");
        assert!(z.area.contains(c.centre));
    }
    assert!(cov.fraction > 0.1 && cov.fraction < 0.6, "{}", cov.fraction);
    let hash = sim.state_hash();

    // A query changes nothing.
    assert_eq!(sim.state_hash(), hash);
    // The side's radars silent: no coverage.
    let mut sim2 = planned_scene(80_000.0, vec![], vec![z.clone()]);
    sim2.submit(cmd(
        1,
        Issuer::Scenario,
        None,
        CommandPayload::SetDoctrine {
            scope: DoctrineScope::Side { side: SideId(1) },
            doctrine: Doctrine {
                emcon: Some(Emcon::Silent),
                ..Doctrine::default()
            },
        },
    ))
    .unwrap();
    sim2.step();
    assert_eq!(sim2.zone_coverage(1).unwrap().fraction, 0.0);

    // A sonar layer: the frigate has no sonar.
    let mut sonar = z;
    sonar.sensors = vec![SensorKind::SonarHull, SensorKind::SonarTowed];
    let mut sim3 = planned_scene(80_000.0, vec![], vec![sonar]);
    assert_eq!(sim3.zone_coverage(1).unwrap().fraction, 0.0);

    // Engage zones have no coverage, and only sense zones list sensors.
    let engage = zone(
        2,
        ZoneKind::Engage,
        circle(centre, 5_000.0),
        Doctrine::default(),
    );
    let mut sim4 = planned_scene(80_000.0, vec![], vec![engage.clone()]);
    assert!(sim4.zone_coverage(2).is_none());
    let mut bad = engage;
    bad.sensors = vec![SensorKind::Radar];
    assert!(Simulation::new(1)
        .submit(planning(vec![], vec![bad]))
        .is_err());
}

// ---- Weapons in flight as sensor targets (Phase 3 slice 15) ----

fn sensor(kind: SensorKind, max_range_m: f64) -> SensorSpec {
    SensorSpec {
        sonar: None,
        sensor_id: format!("TEST-{kind:?}"),
        kind,
        mount_height_m: 20.0,
        min_range_m: 0.0,
        max_range_m,
        ref_range_m: None,
        ref_rcs_m2: None,
        max_target_altitude_m: 25_000.0,
        clutter_db_per_sea_state: None,
        beamwidth_deg: None,
        active: false,
    }
}

fn torpedo() -> WeaponSpec {
    WeaponSpec {
        id: "T-TORP".into(),
        name: "test torpedo".into(),
        kind: WeaponKind::Torpedo,
        guidance: Guidance::Acoustic,
        min_range_m: 500.0,
        max_range_m: 20_000.0,
        speed_mps: 25.0,
        warhead_kg: 250.0,
        base_ph: 0.7,
        target_domains: vec!["surface".into()],
        max_range_by_target: std::collections::BTreeMap::new(),
    }
}

/// Blue (unit 1) at the origin with `sensors` and `stores`, radiating or silent; red (unit 2,
/// radar) `range_m` east fires one `weapon` at it at tick 1.
fn incoming(
    sensors: Vec<SensorSpec>,
    stores: &[(&str, u32)],
    silent: bool,
    weapon: &str,
    range_m: f64,
) -> Simulation {
    let mut sim = Simulation::new(4);
    sim.submit(cmd(
        0,
        Issuer::Scenario,
        None,
        CommandPayload::SetWeaponCatalog {
            weapons: vec![asm(), sam(), torpedo()],
        },
    ))
    .unwrap();
    let mut blue = unit(1, 1, ORIGIN, sensors, stores, None);
    if let CommandPayload::SpawnUnit(u) = &mut blue.payload {
        if silent {
            u.doctrine.emcon = Some(Emcon::Silent);
        }
    }
    sim.submit(blue).unwrap();
    let at = direct(ORIGIN, 90.0, range_m).point;
    sim.submit(unit(2, 2, at, vec![radar()], &[(weapon, 2)], None))
        .unwrap();
    sim.run_ticks(1);
    sim.submit(cmd(
        1,
        umpire(),
        None,
        CommandPayload::Engage {
            shooter: UnitId(2),
            contact: 1,
            weapon: weapon.into(),
            rounds: 1,
        },
    ))
    .unwrap();
    sim
}

fn detections(sim: &Simulation) -> Vec<(u64, SideId, SensorKind)> {
    sim.events()
        .iter()
        .filter_map(|e| match e.kind {
            SimEventKind::WeaponDetected { side, sensor, .. } => Some((e.tick.0, side, sensor)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_radar_tracks_an_incoming_missile_and_the_side_sees_it() {
    let mut sim = incoming(vec![radar()], &[], false, "T-ASM", 30_000.0);
    sim.run_ticks(5);
    let d = detections(&sim);
    assert_eq!(d.len(), 1, "{d:?}");
    assert_eq!((d[0].1, d[0].2), (SideId(1), SensorKind::Radar));
    // In blue's picture, not in red's (red sees its own weapon as its own).
    let blue = sim.snapshot(Viewer::Side { side: SideId(1) }).weapon_tracks;
    assert_eq!(blue.len(), 1);
    assert_eq!(blue[0].domain, Domain::Air);
    assert!(sim
        .snapshot(Viewer::Side { side: SideId(2) })
        .weapon_tracks
        .is_empty());
    // Gone with the weapon.
    sim.run_ticks(200);
    assert!(sim.weapons_in_flight().is_empty());
    assert!(sim.snapshot(Viewer::WhiteCell).weapon_tracks.is_empty());
}

#[test]
fn a_silent_ship_sees_a_missile_with_ir_and_defends_itself() {
    // IR, 20 km: R50 against the missile's plume = 0.7 × 20 km = 14 km (night, full size).
    let mut sim = incoming(
        vec![sensor(SensorKind::Ir, 20_000.0)],
        &[("T-SAM", 4)],
        true,
        "T-ASM",
        30_000.0,
    );
    sim.run_ticks(120);
    let d = detections(&sim);
    assert_eq!(d.len(), 1, "{d:?}");
    assert_eq!(d[0].2, SensorKind::Ir);
    let shots: Vec<f64> = sim
        .events()
        .iter()
        .filter_map(|e| match e.kind {
            SimEventKind::PointDefenceShot { range_m, .. } => Some(range_m),
            _ => None,
        })
        .collect();
    assert!(!shots.is_empty(), "radar silent, but IR lets it shoot");
    // Seen at 14 km; the SAM's 8 s reaction time lets it close 270 m/s × 8 s = 2.16 km.
    assert!((11_500.0..=12_000.0).contains(&shots[0]), "{shots:?}");
    // The same ship without IR, radar silent: blind to the missile.
    let mut blind = incoming(vec![radar()], &[("T-SAM", 4)], true, "T-ASM", 30_000.0);
    blind.run_ticks(120);
    assert!(detections(&blind).is_empty());
    assert!(!blind
        .events()
        .iter()
        .any(|e| matches!(e.kind, SimEventKind::PointDefenceShot { .. })));
}

#[test]
fn a_hull_sonar_hears_an_incoming_torpedo() {
    let mut sim = incoming(
        vec![sensor(SensorKind::SonarHull, 30_000.0)],
        &[],
        false,
        "T-TORP",
        8_000.0,
    );
    sim.run_ticks(3);
    let d = detections(&sim);
    assert_eq!(d.len(), 1, "{d:?}");
    assert_eq!((d[0].1, d[0].2), (SideId(1), SensorKind::SonarHull));
    let v = sim.snapshot(Viewer::Side { side: SideId(1) }).weapon_tracks;
    assert_eq!(v[0].domain, Domain::Subsurface);
    // A radar does not see a torpedo.
    let mut r = incoming(vec![radar()], &[], false, "T-TORP", 8_000.0);
    r.run_ticks(3);
    assert!(detections(&r).is_empty());
}

// ---- Surface action groups in the core (Phase 3 slice 16) ----

fn sag(leader: u64, members: &[(u64, f64)]) -> ActionGroup {
    ActionGroup {
        name: "SAG 1".into(),
        side: SideId(1),
        leader: UnitId(leader),
        members: members
            .iter()
            .map(|&(u, bearing_deg)| GroupStation {
                unit: UnitId(u),
                bearing_deg,
                range_m: 2_000.0,
            })
            .collect(),
        doctrine: Doctrine::default(),
        mission: None,
    }
}

fn set_groups(tick: u64, groups: Vec<ActionGroup>) -> Command {
    cmd(
        tick,
        Issuer::Scenario,
        None,
        CommandPayload::SetPlanning {
            lanes: None,
            facilities: vec![],
            zones: vec![],
            action_groups: Some(groups),
        },
    )
}

fn mission_of(sim: &mut Simulation, unit: u64) -> Option<Mission> {
    unit_view(sim, unit).mission
}

#[test]
fn a_group_runs_one_mission_and_a_member_takes_over_when_the_leader_is_lost() {
    let mut sim = Simulation::new(1);
    for (id, b) in [(1, 0.0), (2, 180.0), (3, 90.0)] {
        sim.submit(unit(
            id,
            1,
            direct(ORIGIN, b, 2_000.0).point,
            vec![],
            &[],
            None,
        ))
        .unwrap();
    }
    sim.submit(set_groups(0, vec![sag(1, &[(2, 180.0), (3, 90.0)])]))
        .unwrap();
    let patrol = Mission::Patrol {
        centre: ORIGIN,
        radius_m: 10_000.0,
    };
    sim.submit(cmd(
        1,
        Issuer::Player { seat: 1 },
        Some(1),
        CommandPayload::AssignGroupMission {
            group: "SAG 1".into(),
            mission: Some(patrol.clone()),
        },
    ))
    .unwrap();
    sim.run_ticks(20);
    assert_eq!(mission_of(&mut sim, 1), Some(patrol.clone()));
    assert!(matches!(
        mission_of(&mut sim, 2),
        Some(Mission::Escort { escorted: 1, .. })
    ));
    // The leader is sunk: unit 2 leads, with the group's mission; unit 3 keeps station on it.
    sim.submit(destroy(20, 1)).unwrap();
    sim.run_ticks(20);
    assert!(sim.events().iter().any(|e| matches!(
        &e.kind,
        SimEventKind::GroupLeaderChanged { leader, .. } if *leader == UnitId(2)
    )));
    assert_eq!(mission_of(&mut sim, 2), Some(patrol));
    assert!(matches!(
        mission_of(&mut sim, 3),
        Some(Mission::Escort { escorted: 2, .. })
    ));
    let groups = sim.snapshot(Viewer::Side { side: SideId(1) }).action_groups;
    assert_eq!(groups[0].leader, UnitId(2));
    assert_eq!(groups[0].members.len(), 1);
}

#[test]
fn units_detach_from_and_join_a_group_in_play() {
    let mut sim = Simulation::new(1);
    for id in 1..=4 {
        sim.submit(unit(
            id,
            1,
            direct(ORIGIN, 0.0, f64::from(id as u32) * 1_000.0).point,
            vec![],
            &[],
            None,
        ))
        .unwrap();
    }
    sim.submit(unit(
        9,
        2,
        direct(ORIGIN, 90.0, 50_000.0).point,
        vec![],
        &[],
        None,
    ))
    .unwrap();
    sim.submit(set_groups(0, vec![sag(1, &[(2, 180.0), (3, 90.0)])]))
        .unwrap();
    sim.run_ticks(11);
    let player = |tick, payload| cmd(tick, Issuer::Player { seat: 1 }, Some(1), payload);
    sim.submit(player(
        11,
        CommandPayload::DetachFromGroup { unit: UnitId(3) },
    ))
    .unwrap();
    sim.submit(player(
        11,
        CommandPayload::JoinGroup {
            unit: UnitId(4),
            group: "SAG 1".into(),
            bearing_deg: 270.0,
            range_m: 3_000.0,
        },
    ))
    .unwrap();
    // Red cannot join a blue group; a unit cannot be in two groups.
    sim.submit(cmd(
        11,
        umpire(),
        None,
        CommandPayload::JoinGroup {
            unit: UnitId(9),
            group: "SAG 1".into(),
            bearing_deg: 0.0,
            range_m: 1_000.0,
        },
    ))
    .unwrap();
    sim.submit(player(
        12,
        CommandPayload::JoinGroup {
            unit: UnitId(2),
            group: "SAG 1".into(),
            bearing_deg: 0.0,
            range_m: 1_000.0,
        },
    ))
    .unwrap();
    sim.run_ticks(20);
    assert_eq!(rejected(&sim).len(), 2, "{:?}", rejected(&sim));
    let u3 = unit_view(&mut sim, 3);
    assert_eq!((u3.mission, u3.group), (None, None));
    let u4 = unit_view(&mut sim, 4);
    assert_eq!(u4.group.as_deref(), Some("SAG 1"));
    assert!(matches!(
        u4.mission,
        Some(Mission::Escort { escorted: 1, bearing_deg, .. }) if bearing_deg == 270.0
    ));
}

/// Two blue ships 1 km apart with SAMs; red fires one ASM at unit 1 from 30 km.
fn two_ship_raid(grouped: bool, silent_1: bool) -> Simulation {
    let mut sim = Simulation::new(8);
    sim.submit(cmd(
        0,
        Issuer::Scenario,
        None,
        CommandPayload::SetWeaponCatalog {
            weapons: vec![asm(), sam()],
        },
    ))
    .unwrap();
    for (id, at) in [(1, ORIGIN), (2, direct(ORIGIN, 0.0, 1_000.0).point)] {
        let mut c = unit(id, 1, at, vec![radar()], &[("T-SAM", 8)], None);
        if let CommandPayload::SpawnUnit(u) = &mut c.payload {
            if silent_1 && id == 1 {
                u.doctrine.emcon = Some(Emcon::Silent);
            }
        }
        sim.submit(c).unwrap();
    }
    if grouped {
        sim.submit(set_groups(0, vec![sag(1, &[(2, 0.0)])]))
            .unwrap();
    }
    let at = direct(ORIGIN, 90.0, 30_000.0).point;
    sim.submit(unit(5, 2, at, vec![radar()], &[("T-ASM", 4)], None))
        .unwrap();
    sim.run_ticks(1);
    sim.submit(cmd(
        1,
        umpire(),
        None,
        CommandPayload::Engage {
            shooter: UnitId(5),
            contact: 1,
            weapon: "T-ASM".into(),
            rounds: 1,
        },
    ))
    .unwrap();
    sim.run_ticks(150);
    sim
}

fn shots_by(sim: &Simulation) -> Vec<(u64, UnitId)> {
    sim.events()
        .iter()
        .filter_map(|e| match e.kind {
            SimEventKind::PointDefenceShot { defender, .. } => Some((e.tick.0, defender)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_group_shares_its_air_picture_and_fires_one_sam_per_threat_at_a_time() {
    // Ungrouped, both ships shoot at the first chance: two SAMs in the air at once.
    let alone = shots_by(&two_ship_raid(false, false));
    assert_eq!(alone[0].0, alone[1].0, "{alone:?}");
    // Grouped: one SAM per threat at a time across the group.
    let sim = two_ship_raid(true, false);
    let together = shots_by(&sim);
    assert!(!together.is_empty());
    let first = together[0].0;
    assert_eq!(
        together.iter().filter(|s| s.0 == first).count(),
        1,
        "{together:?}"
    );
    // Unit 1 silent: alone it cannot see the missile; in the group its mate's radar track lets
    // it fire.
    let silent_alone = shots_by(&two_ship_raid(false, true));
    assert!(
        silent_alone.iter().all(|s| s.1 != UnitId(1)),
        "{silent_alone:?}"
    );
    let shared = two_ship_raid(true, true);
    let tracks: Vec<UnitId> = shared
        .events()
        .iter()
        .filter_map(|e| match e.kind {
            SimEventKind::WeaponDetected { by, .. } => Some(by),
            _ => None,
        })
        .collect();
    assert_eq!(tracks, vec![UnitId(2)], "only the radiating mate tracks it");
    let fired = shots_by(&shared);
    assert_eq!(
        fired[0].1,
        UnitId(1),
        "the silent ship fires on its mate's track: {fired:?}"
    );
}

/// The test gun, with a 5 km range against aircraft (its surface range stays 15 km). Given a
/// seeker's track tolerance so the order is judged on range alone: an aircraft track grows at
/// the air speed bound between scans, past a gun's 300 m limit.
fn gun_with_air_range() -> WeaponSpec {
    let mut g = gun();
    g.guidance = Guidance::ActiveRadar;
    g.max_range_by_target.insert("air".into(), 5_000.0);
    g
}

/// Blue with that gun at the origin; a red aircraft at 1,000 m, `range_m` east.
fn air_scene(range_m: f64) -> Simulation {
    let mut sim = Simulation::new(3);
    sim.submit(cmd(
        0,
        Issuer::Scenario,
        None,
        CommandPayload::SetWeaponCatalog {
            weapons: vec![gun_with_air_range()],
        },
    ))
    .unwrap();
    sim.submit(unit(1, 1, ORIGIN, vec![radar()], &[("T-GUN", 100)], None))
        .unwrap();
    let mut red = unit(2, 2, direct(ORIGIN, 90.0, range_m).point, vec![], &[], None);
    if let CommandPayload::SpawnUnit(s) = &mut red.payload {
        s.domain = Domain::Air;
        s.altitude_m = 1000.0;
    }
    sim.submit(red).unwrap();
    sim.run_ticks(1);
    sim
}

#[test]
fn a_gun_reaches_aircraft_only_within_its_anti_air_range() {
    // 10 km: inside the gun's 15 km surface range, beyond its 5 km range against aircraft.
    let mut sim = air_scene(10_000.0);
    sim.submit(engage_as(1, umpire(), None, "T-GUN", 1))
        .unwrap();
    sim.step();
    let r = rejected(&sim);
    assert!(
        r.iter()
            .any(|x| x.contains("0.0–5.0 km against air targets")),
        "{r:?}"
    );
    assert!(!any_hit(&sim));
    // 4 km: inside it, so the round is fired and rolled.
    let mut sim = air_scene(4_000.0);
    sim.submit(engage_as(1, umpire(), None, "T-GUN", 1))
        .unwrap();
    sim.step();
    assert!(rejected(&sim).is_empty(), "{:?}", rejected(&sim));
    assert!(sim
        .events()
        .iter()
        .any(|e| matches!(e.kind, SimEventKind::HitRoll { .. })));
}
