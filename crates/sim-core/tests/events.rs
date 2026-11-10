//! Scenario events: triggers, conditions, actions, logging, reserves, and determinism across save
//! and resume. Behaviour doc: docs/models/events.md.

#![allow(clippy::unwrap_used)] // test harness

use sim_core::*;

fn cmd(tick: u64, issuer: Issuer, payload: CommandPayload) -> Command {
    Command {
        tick: Tick(tick),
        issuer,
        side: None,
        payload,
    }
}

fn spawn(id: u64, side: u16, at: GeoPoint, speed_mps: f64, route: Vec<GeoPoint>) -> UnitSpawn {
    UnitSpawn {
        id: UnitId(id),
        name: format!("U{id}"),
        side: SideId(side),
        domain: Domain::Surface,
        position: at,
        altitude_m: 0.0,
        heading_deg: 90.0,
        speed_mps,
        max_speed_mps: 15.0,
        accel_mps2: 0.1,
        route,
        platform: None,
        sensors: vec![],
        rcs_m2: None,
        max_turn_rate_dps: None,
        min_turn_radius_m: None,
        group: None,
        doctrine: Doctrine::default(),
        jammer: None,
        radiated_noise_db: None,
        category: None,
        length_m: None,
        stores: Default::default(),
        damage_profile: None,
        mission: None,
    }
}

fn event(id: &str, trigger: Trigger, actions: Vec<EventAction>) -> ScenarioEvent {
    ScenarioEvent {
        id: id.into(),
        name: String::new(),
        enabled: true,
        repeatable: false,
        trigger,
        conditions: vec![],
        actions,
    }
}

fn say(side: Option<u16>, text: &str) -> EventAction {
    EventAction::Message {
        to: side.map_or(Audience::All, |side| Audience::Side { side }),
        text: text.into(),
    }
}

fn umpire() -> Issuer {
    Issuer::WhiteCell {
        umpire: "U1".into(),
    }
}

/// Blue unit 1 sails east at 10 m/s from the origin; red unit 2 waits 20 km east.
fn scene(seed: u64, events: Vec<ScenarioEvent>, reserves: Vec<UnitSpawn>) -> Simulation {
    let mut sim = Simulation::new(seed);
    let east = GeoPoint::new(0.0, 1.0);
    for s in [
        spawn(1, 1, GeoPoint::new(0.0, 0.0), 10.0, vec![east]),
        spawn(2, 2, GeoPoint::new(0.0, 0.18), 0.0, vec![]),
    ] {
        sim.submit(cmd(0, Issuer::Scenario, CommandPayload::SpawnUnit(s)))
            .unwrap();
    }
    sim.submit(cmd(
        0,
        Issuer::Scenario,
        CommandPayload::SetEvents { events, reserves },
    ))
    .unwrap();
    sim
}

fn fired(sim: &Simulation) -> Vec<(u64, String)> {
    sim.events()
        .iter()
        .filter_map(|e| match &e.kind {
            SimEventKind::ScenarioEventFired { event } => Some((e.tick.0, event.clone())),
            _ => None,
        })
        .collect()
}

#[test]
fn a_timed_message_reaches_its_side_and_the_white_cell_only() {
    let mut sim = scene(
        1,
        vec![event(
            "brief",
            Trigger::Time { at_s: 30.0 },
            vec![say(Some(1), "Proceed to the strait")],
        )],
        vec![],
    );
    sim.run_until(Tick(100));
    assert_eq!(fired(&sim), vec![(30, "brief".to_owned())]);
    let blue = sim.snapshot(Viewer::Side { side: SideId(1) });
    assert_eq!(blue.messages.len(), 1);
    assert_eq!(blue.messages[0].text, "Proceed to the strait");
    assert_eq!(blue.messages[0].from, "brief");
    assert_eq!(blue.messages[0].tick, Tick(30));
    assert!(sim
        .snapshot(Viewer::Side { side: SideId(2) })
        .messages
        .is_empty());
    assert_eq!(sim.snapshot(Viewer::WhiteCell).messages.len(), 1);
}

#[test]
fn area_entry_fires_once_and_a_repeatable_event_on_each_entry() {
    // Unit 1 reaches 0.05° E (5 566 m) after about 9 minutes at 10 m/s.
    let area = Area::Circle {
        centre: GeoPoint::new(0.0, 0.1),
        radius_m: 5_566.0,
    };
    let mut once = event(
        "enter",
        Trigger::AreaEntered {
            area,
            side: Some(1),
            unit: None,
        },
        vec![say(None, "Blue entered the box")],
    );
    once.name = "Box entry".into();
    let mut sim = scene(2, vec![once], vec![]);
    sim.run_until(Tick(2_000));
    let f = fired(&sim);
    assert_eq!(f.len(), 1);
    // 5 566 m at 10 m/s after accelerating from rest: well inside 9–11 minutes.
    assert!((540..660).contains(&f[0].0), "{f:?}");
    assert_eq!(
        sim.snapshot(Viewer::Side { side: SideId(2) }).messages[0].from,
        "Box entry"
    );
    // A repeatable event round red's unit 2, which sits there from the start.
    let mut red = event(
        "red",
        Trigger::AreaEntered {
            area: Area::Circle {
                centre: GeoPoint::new(0.0, 0.18),
                radius_m: 1_000.0,
            },
            side: Some(2),
            unit: None,
        },
        vec![say(None, "Red in the box")],
    );
    red.repeatable = true;
    let mut sim = scene(2, vec![red], vec![]);
    sim.run_until(Tick(50));
    // Inside at the first check counts as entering; staying inside does not fire again.
    assert_eq!(fired(&sim), vec![(0, "red".to_owned())]);
}

#[test]
fn conditions_hold_an_event_and_the_failing_condition_is_logged() {
    let mut e = event(
        "late",
        Trigger::Every {
            period_s: 10.0,
            start_s: 0.0,
        },
        vec![say(None, "tick")],
    );
    e.repeatable = true;
    e.conditions = vec![
        Condition::UnitsAlive {
            side: 2,
            cmp: Cmp::Eq,
            count: 1,
        },
        Condition::TimeBetween {
            from_s: Some(25.0),
            to_s: Some(45.0),
        },
    ];
    let mut sim = scene(3, vec![e], vec![]);
    sim.run_until(Tick(61));
    assert_eq!(
        fired(&sim).iter().map(|f| f.0).collect::<Vec<_>>(),
        vec![30, 40]
    );
    let held: Vec<(u64, u32)> = sim
        .events()
        .iter()
        .filter_map(|e| match &e.kind {
            SimEventKind::ScenarioEventHeld { condition, .. } => Some((e.tick.0, *condition)),
            _ => None,
        })
        .collect();
    assert_eq!(held, vec![(0, 1), (10, 1), (20, 1), (50, 1), (60, 1)]);
}

#[test]
fn a_loss_brings_in_a_reserve_changes_posture_and_enables_the_next_event() {
    let reserve = spawn(9, 1, GeoPoint::new(0.5, 0.0), 0.0, vec![]);
    let mut follow_up = event(
        "relief",
        Trigger::Time { at_s: 200.0 },
        vec![say(Some(1), "Relief on station")],
    );
    follow_up.enabled = false;
    let loss = event(
        "loss",
        Trigger::UnitDestroyed {
            side: Some(2),
            unit: None,
        },
        vec![
            EventAction::SpawnReserve { unit: 9 },
            EventAction::SetPosture {
                side: 2,
                toward: 1,
                posture: Posture::Hostile,
            },
            EventAction::EnableEvent {
                event: "relief".into(),
                enabled: true,
            },
            // Fails (no such reserve); logged, and the rest still runs.
            EventAction::SpawnReserve { unit: 99 },
        ],
    );
    let mut sim = scene(4, vec![loss, follow_up], vec![reserve]);
    sim.submit(cmd(
        100,
        umpire(),
        CommandPayload::Adjudicate {
            reason: "test loss".into(),
            action: Adjudication::DestroyUnit { unit: UnitId(2) },
        },
    ))
    .unwrap();
    sim.run_until(Tick(300));
    assert_eq!(
        fired(&sim),
        vec![(100, "loss".to_owned()), (200, "relief".to_owned())]
    );
    let white = sim.snapshot(Viewer::WhiteCell);
    assert!(white.units.iter().any(|u| u.id == UnitId(9)));
    assert!(white
        .postures
        .iter()
        .any(|p| p.side == SideId(2) && p.toward == SideId(1) && p.posture == Posture::Hostile));
    let failed: Vec<(u32, String)> = sim
        .events()
        .iter()
        .filter_map(|e| match &e.kind {
            SimEventKind::ScenarioActionFailed { action, reason, .. } => {
                Some((*action, reason.clone()))
            }
            _ => None,
        })
        .collect();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].0, 3);
    assert!(failed[0].1.contains("not a reserve"));
}

#[test]
fn a_contact_trigger_fires_when_a_side_gains_a_track() {
    let mut sim = Simulation::new(5);
    let mut blue = spawn(1, 1, GeoPoint::new(0.0, 0.0), 0.0, vec![]);
    blue.sensors = vec![SensorSpec {
        sensor_id: "R".into(),
        kind: SensorKind::Radar,
        mount_height_m: 25.0,
        min_range_m: 0.0,
        max_range_m: 100_000.0,
        ref_range_m: Some(80_000.0),
        ref_rcs_m2: Some(1.0),
        max_target_altitude_m: 25_000.0,
        clutter_db_per_sea_state: None,
        beamwidth_deg: None,
        active: false,
    }];
    for s in [blue, spawn(2, 2, GeoPoint::new(0.0, 0.1), 0.0, vec![])] {
        sim.submit(cmd(0, Issuer::Scenario, CommandPayload::SpawnUnit(s)))
            .unwrap();
    }
    sim.submit(cmd(
        0,
        Issuer::Scenario,
        CommandPayload::SetEvents {
            events: vec![event(
                "first-contact",
                Trigger::ContactGained {
                    side: 1,
                    target_side: Some(2),
                },
                vec![say(Some(1), "New surface contact")],
            )],
            reserves: vec![],
        },
    ))
    .unwrap();
    sim.run_until(Tick(60));
    let gained = sim
        .events()
        .iter()
        .find(|e| matches!(e.kind, SimEventKind::ContactGained { .. }))
        .unwrap()
        .tick;
    assert_eq!(fired(&sim), vec![(gained.0, "first-contact".to_owned())]);
}

#[test]
fn only_privileged_issuers_set_events_and_bad_events_are_refused() {
    let mut sim = Simulation::new(6);
    let e = event("a", Trigger::Time { at_s: 1.0 }, vec![say(None, "x")]);
    sim.submit(Command {
        tick: Tick(0),
        issuer: Issuer::Player { seat: 1 },
        side: Some(SideId(1)),
        payload: CommandPayload::SetEvents {
            events: vec![e.clone()],
            reserves: vec![],
        },
    })
    .unwrap();
    sim.run_until(Tick(5));
    assert!(sim.events().iter().any(|ev| matches!(
        &ev.kind,
        SimEventKind::CommandRejected { reason, .. } if reason.contains("set events")
    )));
    assert!(fired(&sim).is_empty());
    let err = sim
        .submit(cmd(
            5,
            Issuer::Scenario,
            CommandPayload::SetEvents {
                events: vec![e.clone(), e],
                reserves: vec![],
            },
        ))
        .unwrap_err();
    assert!(err.to_string().contains("duplicate event id"));
}

fn probabilistic(seed: u64) -> Simulation {
    let mut e = event(
        "maybe",
        Trigger::Every {
            period_s: 5.0,
            start_s: 0.0,
        },
        vec![say(None, "heads")],
    );
    e.repeatable = true;
    e.conditions = vec![Condition::Probability { p: 0.5 }];
    scene(seed, vec![e], vec![])
}

#[test]
fn probability_draws_are_seeded_and_survive_save_and_resume() {
    let mut a = probabilistic(42);
    a.run_until(Tick(500));
    let n = fired(&a).len();
    // 100 checks at p = 0.5: comfortably inside 5 standard deviations.
    assert!((25..=75).contains(&n), "{n}");
    let mut b = probabilistic(42);
    b.run_until(Tick(500));
    assert_eq!(fired(&a), fired(&b));
    assert_eq!(a.state_hash(), b.state_hash());
    // Save midway and resume: identical to the uninterrupted run.
    let mut c = probabilistic(42);
    c.run_until(Tick(250));
    let bytes = c.save().to_bytes().unwrap();
    let mut d = Simulation::from_save(SaveGame::from_bytes(&bytes).unwrap()).unwrap();
    d.run_until(Tick(500));
    assert_eq!(d.state_hash(), a.state_hash());
    assert_eq!(
        d.snapshot(Viewer::WhiteCell).messages,
        a.snapshot(Viewer::WhiteCell).messages
    );
}
