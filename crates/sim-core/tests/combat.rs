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
