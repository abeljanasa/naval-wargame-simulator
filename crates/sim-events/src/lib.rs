//! Scenario event engine (master prompt §7 "events: triggers → conditions → actions").
//! Phase 3 slice 2; behaviour doc `docs/models/events.md`.
//!
//! A pure model crate (ADR-0005): it defines events and decides when they fire. `sim-core` owns
//! the state, gathers each tick's facts, and carries out the actions.
//!
//! Each tick, for every event in id order:
//! 1. its **trigger** is checked against the tick's facts (time, area entry, unit destroyed,
//!    contact gained);
//! 2. if it occurred and the event is enabled (and has not fired, unless repeatable), its
//!    **conditions** are checked in order, all of which must hold;
//! 3. if they hold, its **actions** are carried out in order.

#![forbid(unsafe_code)]

mod eval;
pub mod lua;
mod model;

pub use eval::{EventRuntime, EventWorld, TickFacts, UnitFact};
pub use model::{
    validate_events, Area, Audience, Cmp, Condition, DoctrineTarget, EventAction, Inject,
    ScenarioEvent, Trigger,
};
