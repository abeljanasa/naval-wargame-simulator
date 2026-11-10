//! Event log: what happened and why. Detection and engagement events (Phase 2) will record their
//! model inputs and random draws here, so every outcome can be explained.

use crate::ids::UnitId;
use crate::time::Tick;
use bevy_ecs::prelude::Resource;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SimEvent {
    pub tick: Tick,
    pub kind: SimEventKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SimEventKind {
    UnitSpawned { unit: UnitId },
    CommandApplied { seq: u64 },
    CommandRejected { seq: u64, reason: String },
    WaypointReached { unit: UnitId, index: u32 },
    RouteCompleted { unit: UnitId },
}

#[derive(Debug, Clone, Default, Resource, Serialize, Deserialize)]
pub struct EventLog {
    pub events: Vec<SimEvent>,
}

impl EventLog {
    pub fn push(&mut self, tick: Tick, kind: SimEventKind) {
        self.events.push(SimEvent { tick, kind });
    }
}
