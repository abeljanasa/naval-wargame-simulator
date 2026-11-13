//! Runs scenario events (Phase 3 slice 2; behaviour doc `docs/models/events.md`). The rules live
//! in the pure `sim-events` crate; this module gathers each tick's facts, keeps each event's
//! state, and carries out the actions as script commands.

use crate::command::{Command, CommandPayload, DoctrineScope, Issuer, UnitSpawn};
use crate::components::{Position, Unit};
use crate::events::{EventLog, SimEventKind};
use crate::ids::{SideId, UnitId};
use crate::rng::{RngStreams, StreamId};
use crate::sim::apply_command;
use crate::time::{SimClock, Tick, TICK_SECONDS};
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use sim_events::{
    Audience, Condition, DoctrineTarget, EventAction, EventRuntime, EventWorld, Inject,
    ScenarioEvent, TickFacts, UnitFact,
};
use std::collections::BTreeMap;

/// How many recent messages a picture carries.
pub const MESSAGE_VIEW_LIMIT: usize = 100;

/// The loaded events and their state.
#[derive(Debug, Clone, Default, Resource, Serialize, Deserialize)]
pub struct EventEngine {
    /// In id order.
    pub events: Vec<ScenarioEvent>,
    pub runtime: BTreeMap<String, EventRuntime>,
    /// Units held off the board until an event brings them in.
    pub reserves: BTreeMap<UnitId, UnitSpawn>,
    /// The last known side of every unit seen in play, so a destroyed unit's side is known.
    pub sides: BTreeMap<UnitId, SideId>,
    /// The inject library, by id.
    #[serde(default)]
    pub injects: BTreeMap<String, Inject>,
    /// Times each inject has been sent.
    #[serde(default)]
    pub sent: BTreeMap<String, u32>,
}

impl EventEngine {
    /// Replaces the events and reserves; every event starts unfired.
    pub(crate) fn load(
        &mut self,
        mut events: Vec<ScenarioEvent>,
        reserves: Vec<UnitSpawn>,
        injects: Vec<Inject>,
    ) {
        self.injects = injects.into_iter().map(|i| (i.id.clone(), i)).collect();
        self.sent.clear();
        events.sort_by(|a, b| a.id.cmp(&b.id));
        self.runtime = events
            .iter()
            .map(|e| {
                (
                    e.id.clone(),
                    EventRuntime {
                        enabled: e.enabled,
                        ..EventRuntime::default()
                    },
                )
            })
            .collect();
        self.events = events;
        self.reserves = reserves.into_iter().map(|s| (s.id, s)).collect();
    }
}

/// A message delivered to a message log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Message {
    pub tick: Tick,
    /// The inject's sender, the event's name (or id), or `Exercise control` for the white cell.
    pub from: String,
    pub to: Audience,
    pub text: String,
    /// The inject's title, when the message is an inject.
    #[serde(default)]
    pub title: Option<String>,
    /// The inject's id, when the message is an inject.
    #[serde(default)]
    pub inject: Option<String>,
}

/// Sender shown for messages the white cell sends.
pub const EXERCISE_CONTROL: &str = "Exercise control";

/// A white-cell facilitation action: running the exercise, as opposed to overriding outcomes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Facilitation {
    /// Sends an inject from the library now.
    SendInject { inject: String },
    /// Sends a free-text message from exercise control.
    SendMessage {
        #[serde(flatten)]
        to: Audience,
        text: String,
    },
    /// Fires an event now: its actions run regardless of its trigger, conditions and whether it
    /// is enabled or spent. It counts as fired.
    FireEvent { event: String },
    /// Turns an event on or off.
    SetEventEnabled { event: String, enabled: bool },
}

impl Facilitation {
    pub(crate) fn validate(&self) -> Result<(), String> {
        match self {
            Facilitation::SendMessage { text, .. } if text.trim().is_empty() => {
                Err("message text is empty".into())
            }
            Facilitation::SendInject { inject: id }
            | Facilitation::FireEvent { event: id }
            | Facilitation::SetEventEnabled { event: id, .. }
                if id.trim().is_empty() =>
            {
                Err("no id given".into())
            }
            _ => Ok(()),
        }
    }
}

/// An inject as the white cell sees it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct InjectStatus {
    pub inject: Inject,
    pub sent: u32,
}

/// An event as the white cell sees it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct EventStatus {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub repeatable: bool,
    pub fired: u32,
}

impl EventEngine {
    /// The inject library with send counts, in id order.
    #[must_use]
    pub fn inject_status(&self) -> Vec<InjectStatus> {
        self.injects
            .values()
            .map(|i| InjectStatus {
                inject: i.clone(),
                sent: self.sent.get(&i.id).copied().unwrap_or(0),
            })
            .collect()
    }

    /// Every event's state, in id order.
    #[must_use]
    pub fn event_status(&self) -> Vec<EventStatus> {
        self.events
            .iter()
            .map(|e| {
                let rt = self.runtime.get(&e.id);
                EventStatus {
                    id: e.id.clone(),
                    name: e.name.clone(),
                    enabled: rt.is_some_and(|r| r.enabled),
                    repeatable: e.repeatable,
                    fired: rt.map_or(0, |r| r.fired),
                }
            })
            .collect()
    }
}

/// Sends inject `id` (`event_from` is the sending event's name, if an event sends it).
fn deliver_inject(
    world: &mut World,
    engine: &mut EventEngine,
    id: &str,
    event_from: Option<&str>,
    tick: Tick,
) -> Result<(), String> {
    let inject = engine
        .injects
        .get(id)
        .ok_or_else(|| format!("unknown inject {id:?}"))?;
    let from = if !inject.from.is_empty() {
        inject.from.clone()
    } else {
        event_from.unwrap_or(EXERCISE_CONTROL).to_owned()
    };
    world.resource_mut::<MessageLog>().0.push(Message {
        tick,
        from,
        to: inject.to,
        text: inject.text.clone(),
        title: Some(inject.title.clone()),
        inject: Some(inject.id.clone()),
    });
    *engine.sent.entry(id.to_owned()).or_default() += 1;
    Ok(())
}

/// Carries out a white-cell facilitation action from `umpire`. The `Facilitated` event is logged
/// first, so the log explains what follows.
pub(crate) fn facilitate(
    world: &mut World,
    umpire: &str,
    action: &Facilitation,
) -> Result<(), String> {
    let tick = world.resource::<SimClock>().tick;
    let mut engine = world.remove_resource::<EventEngine>().unwrap_or_default();
    let known = match action {
        Facilitation::SendInject { inject } => engine
            .injects
            .contains_key(inject)
            .then_some(())
            .ok_or_else(|| format!("unknown inject {inject:?}")),
        Facilitation::FireEvent { event } | Facilitation::SetEventEnabled { event, .. } => engine
            .runtime
            .contains_key(event)
            .then_some(())
            .ok_or_else(|| format!("unknown event {event:?}")),
        Facilitation::SendMessage { .. } => Ok(()),
    };
    let result = known.and_then(|()| {
        world.resource_mut::<EventLog>().push(
            tick,
            SimEventKind::Facilitated {
                umpire: umpire.to_owned(),
                action: action.clone(),
            },
        );
        match action {
            Facilitation::SendInject { inject } => {
                deliver_inject(world, &mut engine, inject, None, tick)
            }
            Facilitation::SendMessage { to, text } => {
                world.resource_mut::<MessageLog>().0.push(Message {
                    tick,
                    from: EXERCISE_CONTROL.to_owned(),
                    to: *to,
                    text: text.clone(),
                    title: None,
                    inject: None,
                });
                Ok(())
            }
            Facilitation::FireEvent { event } => {
                if let Some(i) = engine.events.iter().position(|e| &e.id == event) {
                    fire(world, &mut engine, i, tick);
                }
                Ok(())
            }
            Facilitation::SetEventEnabled { event, enabled } => {
                if let Some(rt) = engine.runtime.get_mut(event) {
                    rt.enabled = *enabled;
                }
                Ok(())
            }
        }
    });
    world.insert_resource(engine);
    result
}

/// Every message sent, in order.
#[derive(Debug, Clone, Default, Resource, Serialize, Deserialize)]
pub struct MessageLog(pub Vec<Message>);

impl MessageLog {
    /// The latest messages a viewer of `side` (`None` = white cell) receives, oldest first.
    #[must_use]
    pub fn visible(&self, side: Option<SideId>) -> Vec<Message> {
        let mut v: Vec<Message> = self
            .0
            .iter()
            .rev()
            .filter(|m| m.to.reaches(side.map(|s| s.0)))
            .take(MESSAGE_VIEW_LIMIT)
            .cloned()
            .collect();
        v.reverse();
        v
    }
}

/// The world as conditions see it.
struct View<'a> {
    now_s: f64,
    runtime: &'a BTreeMap<String, EventRuntime>,
    units: &'a [UnitFact],
    rng: &'a mut RngStreams,
}

impl EventWorld for View<'_> {
    fn now_s(&self) -> f64 {
        self.now_s
    }
    fn fired(&self, event: &str) -> u32 {
        self.runtime.get(event).map_or(0, |r| r.fired)
    }
    fn unit_alive(&self, unit: u64) -> bool {
        self.units.binary_search_by_key(&unit, |u| u.id).is_ok()
    }
    fn units_alive(&self, side: u16) -> u32 {
        u32::try_from(self.units.iter().filter(|u| u.side == side).count()).unwrap_or(u32::MAX)
    }
    fn draw(&mut self) -> f64 {
        self.rng.uniform(StreamId::Events)
    }
    fn unit(&self, unit: u64) -> Option<UnitFact> {
        self.units
            .binary_search_by_key(&unit, |u| u.id)
            .ok()
            .map(|i| self.units[i])
    }
}

/// Every unit in play, in id order.
fn units_in_play(world: &mut World) -> Vec<UnitFact> {
    let mut units: Vec<UnitFact> = world
        .query::<(&Unit, &Position)>()
        .iter(world)
        .map(|(u, p)| UnitFact {
            id: u.id.0,
            side: u.side.0,
            position: p.point,
        })
        .collect();
    units.sort_by_key(|u| u.id);
    units
}

/// Checks every event once per tick, after the models have run, and fires those whose trigger
/// occurred and whose conditions hold.
pub(crate) fn scenario_events_system(world: &mut World) {
    if world.resource::<EventEngine>().events.is_empty() {
        return;
    }
    let Some(mut engine) = world.remove_resource::<EventEngine>() else {
        return;
    };
    let tick = world.resource::<SimClock>().tick;
    let now_s = tick.seconds();

    let units = units_in_play(world);
    for u in &units {
        engine.sides.insert(UnitId(u.id), SideId(u.side));
    }
    let mut destroyed = Vec::new();
    let mut gained = Vec::new();
    for e in world
        .resource::<EventLog>()
        .events
        .iter()
        .rev()
        .take_while(|e| e.tick == tick)
    {
        match &e.kind {
            SimEventKind::UnitDestroyed { unit } => {
                if let Some(s) = engine.sides.get(unit) {
                    destroyed.push((unit.0, s.0));
                }
            }
            SimEventKind::ContactGained { side, target, .. } => {
                if let Some(t) = engine.sides.get(target) {
                    gained.push((side.0, t.0));
                }
            }
            _ => {}
        }
    }
    let facts = TickFacts {
        now_s,
        prev_s: now_s - TICK_SECONDS,
        units: &units,
        destroyed: &destroyed,
        contacts_gained: &gained,
    };

    for i in 0..engine.events.len() {
        let id = engine.events[i].id.clone();
        let occurred = {
            let rt = engine.runtime.entry(id.clone()).or_default();
            engine.events[i].trigger.occurred(&facts, &mut rt.inside)
        };
        let rt = &engine.runtime[&id];
        if !occurred || !rt.enabled || (rt.fired > 0 && !engine.events[i].repeatable) {
            continue;
        }
        let failing = {
            let mut rng = world.resource_mut::<RngStreams>();
            let mut view = View {
                now_s,
                runtime: &engine.runtime,
                units: &units,
                rng: &mut rng,
            };
            Condition::first_failing(&engine.events[i].conditions, &mut view)
        };
        #[allow(clippy::cast_possible_truncation)]
        match failing {
            Ok(None) => {}
            Ok(Some(c)) => {
                world.resource_mut::<EventLog>().push(
                    tick,
                    SimEventKind::ScenarioEventHeld {
                        event: id,
                        condition: c as u32,
                    },
                );
                continue;
            }
            Err((c, reason)) => {
                world.resource_mut::<EventLog>().push(
                    tick,
                    SimEventKind::ScenarioScriptFailed {
                        event: id,
                        condition: c as u32,
                        reason,
                    },
                );
                continue;
            }
        }
        fire(world, &mut engine, i, tick);
    }
    world.insert_resource(engine);
}

/// Fires event `i`: counts it, logs it, and carries out its actions in order. A failing action
/// is logged and the rest still run.
pub(crate) fn fire(world: &mut World, engine: &mut EventEngine, i: usize, tick: Tick) {
    let event = engine.events[i].clone();
    if let Some(rt) = engine.runtime.get_mut(&event.id) {
        rt.fired += 1;
    }
    world.resource_mut::<EventLog>().push(
        tick,
        SimEventKind::ScenarioEventFired {
            event: event.id.clone(),
        },
    );
    let from = if event.name.is_empty() {
        event.id.clone()
    } else {
        event.name.clone()
    };
    for (n, action) in event.actions.iter().enumerate() {
        if let Err(reason) = act(world, engine, action, &from, tick) {
            #[allow(clippy::cast_possible_truncation)]
            world.resource_mut::<EventLog>().push(
                tick,
                SimEventKind::ScenarioActionFailed {
                    event: event.id.clone(),
                    action: n as u32,
                    reason,
                },
            );
        }
    }
}

fn act(
    world: &mut World,
    engine: &mut EventEngine,
    action: &EventAction,
    from: &str,
    tick: Tick,
) -> Result<(), String> {
    let payload = match action {
        EventAction::Message { to, text } => {
            world.resource_mut::<MessageLog>().0.push(Message {
                tick,
                from: from.to_owned(),
                to: *to,
                text: text.clone(),
                title: None,
                inject: None,
            });
            return Ok(());
        }
        EventAction::SendInject { inject } => {
            return deliver_inject(world, engine, inject, Some(from), tick);
        }
        EventAction::Lua { script } => {
            let units = units_in_play(world);
            let asked = {
                let mut rng = world.resource_mut::<RngStreams>();
                let mut view = View {
                    now_s: tick.seconds(),
                    runtime: &engine.runtime,
                    units: &units,
                    rng: &mut rng,
                };
                sim_events::lua::action(script, &mut view)?
            };
            // Every requested action runs; their failures are reported together.
            let failures: Vec<String> = asked
                .iter()
                .filter_map(|a| match a {
                    EventAction::Lua { .. } => Some("a script cannot run a script".to_owned()),
                    _ => act(world, engine, a, from, tick).err(),
                })
                .collect();
            return if failures.is_empty() {
                Ok(())
            } else {
                Err(failures.join("; "))
            };
        }
        EventAction::EnableEvent { event, enabled } => {
            return engine
                .runtime
                .get_mut(event)
                .map(|rt| rt.enabled = *enabled)
                .ok_or_else(|| format!("unknown event {event:?}"));
        }
        EventAction::SpawnReserve { unit } => {
            let spawn = engine
                .reserves
                .remove(&UnitId(*unit))
                .ok_or_else(|| format!("U{unit} is not a reserve still held"))?;
            CommandPayload::SpawnUnit(spawn)
        }
        EventAction::SetPosture {
            side,
            toward,
            posture,
        } => CommandPayload::SetPosture {
            side: SideId(*side),
            toward: SideId(*toward),
            posture: *posture,
        },
        EventAction::SetDoctrine { scope, doctrine } => CommandPayload::SetDoctrine {
            scope: match scope {
                DoctrineTarget::Side { side } => DoctrineScope::Side {
                    side: SideId(*side),
                },
                DoctrineTarget::Group { side, group } => DoctrineScope::Group {
                    side: SideId(*side),
                    group: group.clone(),
                },
                DoctrineTarget::Unit { unit } => DoctrineScope::Unit {
                    unit: UnitId(*unit),
                },
            },
            doctrine: *doctrine,
        },
        EventAction::SetEnvironment { environment } => CommandPayload::SetEnvironment {
            environment: *environment,
        },
        EventAction::SetWeather { zones } => CommandPayload::SetWeather {
            zones: zones.clone(),
        },
        EventAction::SetSpeed { unit, speed_mps } => CommandPayload::SetSpeed {
            unit: UnitId(*unit),
            speed_mps: *speed_mps,
        },
        EventAction::SetRoute { unit, waypoints } => CommandPayload::SetRoute {
            unit: UnitId(*unit),
            waypoints: waypoints.clone(),
        },
        EventAction::AssignMission { unit, mission } => CommandPayload::AssignMission {
            unit: UnitId(*unit),
            mission: mission.clone(),
        },
    };
    payload.validate()?;
    apply_command(
        world,
        &Command {
            tick,
            issuer: Issuer::Script,
            side: None,
            payload,
        },
    )
}
