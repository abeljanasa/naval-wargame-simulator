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
        fuel: None,
        draft_m: None,
        displacement_t: None,
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
        CommandPayload::SetEvents {
            events,
            reserves,
            injects: vec![],
        },
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
        sonar: None,
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
            injects: vec![],
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
            injects: vec![],
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
                injects: vec![],
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

// --- Injects and white-cell facilitation (Phase 3 slice 3) ---

fn library() -> Vec<Inject> {
    vec![
        Inject {
            id: "i-distress".into(),
            title: "Distress call".into(),
            from: "MV Ember".into(),
            to: Audience::Side { side: 1 },
            text: "Engine room fire, request assistance".into(),
            note: "Tests whether Blue diverts from the patrol".into(),
        },
        Inject {
            id: "i-media".into(),
            title: "Media report".into(),
            from: String::new(),
            to: Audience::All,
            text: "Footage of the encounter is circulating online".into(),
            note: String::new(),
        },
    ]
}

/// The scene plus an inject library, with an event that sends the media report at 60 s and a
/// disabled event the white cell can fire.
fn facilitated(seed: u64) -> Simulation {
    let mut sim = scene(seed, vec![], vec![]);
    let mut spare = event(
        "spare",
        Trigger::Time { at_s: 1.0 },
        vec![say(Some(2), "Red: return to port")],
    );
    spare.enabled = false;
    let mut media = event(
        "media",
        Trigger::Time { at_s: 60.0 },
        vec![EventAction::SendInject {
            inject: "i-media".into(),
        }],
    );
    media.name = "Press desk".into();
    sim.submit(cmd(
        0,
        Issuer::Scenario,
        CommandPayload::SetEvents {
            events: vec![media, spare],
            reserves: vec![],
            injects: library(),
        },
    ))
    .unwrap();
    sim
}

fn facilitate(tick: u64, issuer: Issuer, action: Facilitation) -> Command {
    cmd(tick, issuer, CommandPayload::Facilitate { action })
}

#[test]
fn injects_go_out_by_event_and_by_the_white_cell_and_are_counted() {
    let mut sim = facilitated(7);
    sim.submit(facilitate(
        30,
        umpire(),
        Facilitation::SendInject {
            inject: "i-distress".into(),
        },
    ))
    .unwrap();
    sim.run_until(Tick(100));
    let blue = sim.snapshot(Viewer::Side { side: SideId(1) });
    let got: Vec<(u64, &str, Option<&str>, &str)> = blue
        .messages
        .iter()
        .map(|m| {
            (
                m.tick.0,
                m.from.as_str(),
                m.title.as_deref(),
                m.text.as_str(),
            )
        })
        .collect();
    assert_eq!(
        got,
        vec![
            (
                30,
                "MV Ember",
                Some("Distress call"),
                "Engine room fire, request assistance"
            ),
            (
                60,
                "Press desk",
                Some("Media report"),
                "Footage of the encounter is circulating online"
            ),
        ]
    );
    // Red gets only the media report; sides never see the library or the white-cell notes.
    let red = sim.snapshot(Viewer::Side { side: SideId(2) });
    assert_eq!(red.messages.len(), 1);
    assert!(red.injects.is_empty() && red.scenario_events.is_empty());
    let white = sim.snapshot(Viewer::WhiteCell);
    let sent: Vec<(String, u32)> = white
        .injects
        .iter()
        .map(|s| (s.inject.id.clone(), s.sent))
        .collect();
    assert_eq!(
        sent,
        vec![("i-distress".to_owned(), 1), ("i-media".to_owned(), 1)]
    );
    assert_eq!(
        white.injects[0].inject.note,
        "Tests whether Blue diverts from the patrol"
    );
    // The umpire who sent it is in the event log.
    assert!(sim.events().iter().any(|e| matches!(
        &e.kind,
        SimEventKind::Facilitated { umpire, .. } if umpire == "U1"
    )));
}

#[test]
fn the_white_cell_fires_and_switches_events_and_messages_but_players_cannot() {
    let mut sim = facilitated(8);
    let player = Issuer::Player { seat: 1 };
    sim.submit(facilitate(
        5,
        player,
        Facilitation::FireEvent {
            event: "spare".into(),
        },
    ))
    .unwrap();
    // A disabled event fires on the white cell's order, and counts as fired.
    sim.submit(facilitate(
        10,
        umpire(),
        Facilitation::FireEvent {
            event: "spare".into(),
        },
    ))
    .unwrap();
    // Switching the timed media event off stops it.
    sim.submit(facilitate(
        20,
        umpire(),
        Facilitation::SetEventEnabled {
            event: "media".into(),
            enabled: false,
        },
    ))
    .unwrap();
    sim.submit(facilitate(
        25,
        umpire(),
        Facilitation::SendMessage {
            to: Audience::WhiteCell,
            text: "Note for the umpires".into(),
        },
    ))
    .unwrap();
    sim.submit(facilitate(
        26,
        umpire(),
        Facilitation::SendInject {
            inject: "nope".into(),
        },
    ))
    .unwrap();
    sim.run_until(Tick(100));
    let rejected: Vec<String> = sim
        .events()
        .iter()
        .filter_map(|e| match &e.kind {
            SimEventKind::CommandRejected { reason, .. } => Some(reason.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(rejected.len(), 2, "{rejected:?}");
    assert!(rejected[0].contains("only the white cell"));
    assert!(rejected[1].contains("unknown inject"));
    assert_eq!(fired(&sim), vec![(10, "spare".to_owned())]);
    let red = sim.snapshot(Viewer::Side { side: SideId(2) });
    assert_eq!(red.messages.len(), 1);
    assert_eq!(red.messages[0].text, "Red: return to port");
    let white = sim.snapshot(Viewer::WhiteCell);
    let status: Vec<(&str, bool, u32)> = white
        .scenario_events
        .iter()
        .map(|e| (e.id.as_str(), e.enabled, e.fired))
        .collect();
    assert_eq!(status, vec![("media", false, 0), ("spare", false, 1)]);
    let note = white.messages.last().unwrap();
    assert_eq!(
        (note.from.as_str(), note.to, note.text.as_str()),
        (
            EXERCISE_CONTROL,
            Audience::WhiteCell,
            "Note for the umpires"
        )
    );
    assert!(sim
        .snapshot(Viewer::Side { side: SideId(1) })
        .messages
        .is_empty());
}

// --- Lua scripts (Phase 3 slice 4) ---

#[test]
fn lua_conditions_and_actions_drive_the_simulation() {
    // Every 10 s, once blue unit 1 is east of 0.01° E, a script orders it to slow down and
    // reports its position. It needs a minute or so to get there from rest.
    let mut e = event(
        "slow",
        Trigger::Every {
            period_s: 10.0,
            start_s: 0.0,
        },
        vec![EventAction::Lua {
            script: "local u = sim.unit(1)\n\
                     sim.set_speed(1, 2)\n\
                     sim.message(1, string.format('Anvil at %.4f E, slowing', u.lon))"
                .into(),
        }],
    );
    e.conditions = vec![Condition::Lua {
        script: "local u = sim.unit(1) return u ~= nil and u.lon > 0.01".into(),
    }];
    let mut sim = scene(9, vec![e], vec![]);
    sim.run_until(Tick(300));
    let f = fired(&sim);
    assert_eq!(f.len(), 1, "{f:?}");
    let blue = sim.snapshot(Viewer::Side { side: SideId(1) });
    let u = blue.units.iter().find(|u| u.id == UnitId(1)).unwrap();
    assert!((u.ordered_speed_mps - 2.0).abs() < 1e-12);
    assert!(
        blue.messages[0].text.starts_with("Anvil at 0.01")
            && blue.messages[0].text.ends_with("slowing"),
        "{}",
        blue.messages[0].text
    );
    // Before the unit got there, the condition held the event every 10 s.
    let held = sim
        .events()
        .iter()
        .filter(|e| matches!(e.kind, SimEventKind::ScenarioEventHeld { .. }))
        .count();
    assert!(held >= 5, "{held}");
}

#[test]
fn script_errors_are_logged_and_never_stop_the_run() {
    let cond_err = {
        let mut e = event(
            "bad-cond",
            Trigger::Time { at_s: 5.0 },
            vec![say(None, "x")],
        );
        e.conditions = vec![Condition::Lua {
            script: "return sim.unit(1).nope.deeper".into(),
        }];
        e
    };
    let act_err = event(
        "bad-act",
        Trigger::Time { at_s: 6.0 },
        vec![
            EventAction::Lua {
                script: "sim.set_speed(1, -5) sim.spawn_reserve(42)".into(),
            },
            say(None, "after the script"),
        ],
    );
    let loop_forever = event(
        "spin",
        Trigger::Time { at_s: 7.0 },
        vec![EventAction::Lua {
            script: "while true do end".into(),
        }],
    );
    let mut sim = scene(10, vec![cond_err, act_err, loop_forever], vec![]);
    sim.run_until(Tick(20));
    let script_failed: Vec<String> = sim
        .events()
        .iter()
        .filter_map(|e| match &e.kind {
            SimEventKind::ScenarioScriptFailed { event, reason, .. } => {
                Some(format!("{event}: {reason}"))
            }
            _ => None,
        })
        .collect();
    assert_eq!(script_failed.len(), 1, "{script_failed:?}");
    assert!(script_failed[0].starts_with("bad-cond: "));
    let action_failed: Vec<String> = sim
        .events()
        .iter()
        .filter_map(|e| match &e.kind {
            SimEventKind::ScenarioActionFailed { event, reason, .. } => {
                Some(format!("{event}: {reason}"))
            }
            _ => None,
        })
        .collect();
    assert_eq!(action_failed.len(), 2, "{action_failed:?}");
    // Both requested actions were tried; both failures are reported together.
    assert!(action_failed[0].contains("speed must be finite and >= 0"));
    assert!(action_failed[0].contains("not a reserve"));
    assert!(action_failed[1].contains("instruction limit"));
    // The plain message after the failing script still went out, and the run carried on.
    assert_eq!(sim.snapshot(Viewer::WhiteCell).messages.len(), 1);
    assert_eq!(sim.tick(), Tick(20));
}

#[test]
fn a_script_with_a_syntax_error_is_refused_when_the_events_are_loaded() {
    let mut sim = Simulation::new(11);
    let bad = event(
        "a",
        Trigger::Time { at_s: 1.0 },
        vec![EventAction::Lua {
            script: "sim.message('all', ".into(),
        }],
    );
    let err = sim
        .submit(cmd(
            0,
            Issuer::Scenario,
            CommandPayload::SetEvents {
                events: vec![bad],
                reserves: vec![],
                injects: vec![],
            },
        ))
        .unwrap_err();
    assert!(err.to_string().contains("Lua action"), "{err}");
}

// --- Seats (Phase 3 slice 5) ---

fn seat(id: u32, name: &str, side: Option<u16>) -> Seat {
    Seat {
        id,
        name: name.into(),
        role: side.map_or(SeatRole::WhiteCell, |s| SeatRole::Side { side: SideId(s) }),
    }
}

fn seated(seed: u64) -> Simulation {
    let mut sim = scene(seed, vec![], vec![]);
    sim.submit(cmd(
        0,
        Issuer::Scenario,
        CommandPayload::SetSeats {
            seats: vec![
                seat(0, "Umpire A", None),
                seat(1, "Blue CO", Some(1)),
                seat(2, "Red CO", Some(2)),
            ],
        },
    ))
    .unwrap();
    sim
}

fn speed_order(tick: u64, issuer: Issuer, side: Option<u16>, unit: u64) -> Command {
    Command {
        tick: Tick(tick),
        issuer,
        side: side.map(SideId),
        payload: CommandPayload::SetSpeed {
            unit: UnitId(unit),
            speed_mps: 3.0,
        },
    }
}

#[test]
fn seats_decide_who_may_command_which_side() {
    let mut sim = seated(12);
    let player = |seat| Issuer::Player { seat };
    let orders = [
        speed_order(1, player(1), Some(1), 1), // Blue seat, Blue unit: applied
        speed_order(1, player(1), Some(2), 2), // Blue seat claiming Red: refused
        speed_order(1, player(9), Some(1), 1), // no such seat: refused
        speed_order(1, player(0), Some(1), 1), // white-cell seat as a player: refused
        speed_order(1, umpire(), None, 2),     // "U1" is not a white-cell seat: refused
        speed_order(
            1,
            Issuer::WhiteCell {
                umpire: "Umpire A".into(),
            },
            None,
            2,
        ), // applied
    ];
    let seqs: Vec<u64> = orders.into_iter().map(|o| sim.submit(o).unwrap()).collect();
    sim.run_until(Tick(3));
    let outcome = |seq: u64| -> Result<(), String> {
        sim.events()
            .iter()
            .find_map(|e| match &e.kind {
                SimEventKind::CommandApplied { seq: s } if *s == seq => Some(Ok(())),
                SimEventKind::CommandRejected { seq: s, reason } if *s == seq => {
                    Some(Err(reason.clone()))
                }
                _ => None,
            })
            .unwrap()
    };
    assert_eq!(outcome(seqs[0]), Ok(()));
    assert!(outcome(seqs[1]).unwrap_err().contains("plays side 1"));
    assert!(outcome(seqs[2]).unwrap_err().contains("no seat 9"));
    assert!(outcome(seqs[3]).unwrap_err().contains("white-cell seat"));
    assert!(outcome(seqs[4])
        .unwrap_err()
        .contains("not a white-cell seat"));
    assert_eq!(outcome(seqs[5]), Ok(()));
}

#[test]
fn seat_tables_are_validated_visible_by_side_and_saved() {
    let mut sim = Simulation::new(13);
    let err = sim
        .submit(cmd(
            0,
            Issuer::Scenario,
            CommandPayload::SetSeats {
                seats: vec![seat(1, "Blue", Some(1))],
            },
        ))
        .unwrap_err();
    assert!(err.to_string().contains("white-cell seat"));
    let err = sim
        .submit(cmd(
            0,
            Issuer::Scenario,
            CommandPayload::SetSeats {
                seats: vec![seat(0, "A", None), seat(1, "A", Some(1))],
            },
        ))
        .unwrap_err();
    assert!(err.to_string().contains("duplicate seat name"));

    let mut sim = seated(13);
    sim.run_until(Tick(2));
    let names = |v: Viewer, sim: &mut Simulation| -> Vec<String> {
        sim.snapshot(v).seats.into_iter().map(|s| s.name).collect()
    };
    assert_eq!(
        names(Viewer::WhiteCell, &mut sim),
        vec!["Umpire A", "Blue CO", "Red CO"]
    );
    assert_eq!(
        names(Viewer::Side { side: SideId(2) }, &mut sim),
        vec!["Red CO"]
    );
    // A resumed run keeps the table and still enforces it.
    let bytes = sim.save().to_bytes().unwrap();
    let mut resumed = Simulation::from_save(SaveGame::from_bytes(&bytes).unwrap()).unwrap();
    assert_eq!(resumed.state_hash(), sim.state_hash());
    let seq = resumed
        .submit(speed_order(2, Issuer::Player { seat: 2 }, Some(1), 1))
        .unwrap();
    resumed.run_until(Tick(3));
    assert!(resumed.events().iter().any(|e| matches!(
        &e.kind,
        SimEventKind::CommandRejected { seq: s, .. } if *s == seq
    )));
}
