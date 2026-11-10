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
    Audience, Condition, DoctrineTarget, EventAction, EventRuntime, EventWorld, ScenarioEvent,
    TickFacts, UnitFact,
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
}

impl EventEngine {
    /// Replaces the events and reserves; every event starts unfired.
    pub(crate) fn load(&mut self, mut events: Vec<ScenarioEvent>, reserves: Vec<UnitSpawn>) {
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
    /// The event's name (or id), or the umpire.
    pub from: String,
    pub to: Audience,
    pub text: String,
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
        if let Some(c) = failing {
            #[allow(clippy::cast_possible_truncation)]
            world.resource_mut::<EventLog>().push(
                tick,
                SimEventKind::ScenarioEventHeld {
                    event: id,
                    condition: c as u32,
                },
            );
            continue;
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
            });
            return Ok(());
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
