//! Seats: who may issue commands for which side (Phase 3 slice 5; behaviour doc
//! `docs/models/multi-seat.md`).
//!
//! A seat is a player position: one side's, or the white cell's. The seat table is set by a
//! command (`SetSeats`), so it is in the command log, saved and replayed. Once a table is set:
//! - a player's command must come from a seat in the table, for that seat's side;
//! - an umpire's command must name a white-cell seat.
//!
//! With no table (tests, batch runs) commands are checked by side as before.

use crate::command::{Command, Issuer};
use crate::ids::SideId;
use bevy_ecs::prelude::Resource;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// What a seat may do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum SeatRole {
    /// Ground truth, every side, overrides, injects and time.
    WhiteCell,
    /// One side's picture and units.
    Side { side: SideId },
}

/// A player position.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Seat {
    pub id: u32,
    /// Unique. A white-cell seat's name is the umpire id on its overrides and injects.
    pub name: String,
    #[serde(flatten)]
    pub role: SeatRole,
}

/// The seat table, by id. Empty: no seat checks.
#[derive(Debug, Clone, Default, PartialEq, Eq, Resource, Serialize, Deserialize)]
pub struct Seats(pub BTreeMap<u32, Seat>);

/// Checks a seat table on its own: unique ids and names, names not blank, and at least one
/// white-cell seat.
pub fn validate_seats(seats: &[Seat]) -> Result<(), String> {
    let mut ids = BTreeSet::new();
    let mut names = BTreeSet::new();
    for s in seats {
        if s.name.trim().is_empty() {
            return Err(format!("seat {} has no name", s.id));
        }
        if !ids.insert(s.id) {
            return Err(format!("duplicate seat id {}", s.id));
        }
        if !names.insert(s.name.as_str()) {
            return Err(format!("duplicate seat name {:?}", s.name));
        }
    }
    if !seats.iter().any(|s| s.role == SeatRole::WhiteCell) {
        return Err("there must be a white-cell seat".into());
    }
    Ok(())
}

impl Seats {
    /// May `cmd` be issued from its seat? Scenario and script commands always may.
    pub fn check(&self, cmd: &Command) -> Result<(), String> {
        if self.0.is_empty() {
            return Ok(());
        }
        match &cmd.issuer {
            Issuer::Player { seat } => match self.0.get(seat).map(|s| s.role) {
                Some(SeatRole::Side { side }) if cmd.side == Some(side) => Ok(()),
                Some(SeatRole::Side { side }) => Err(format!(
                    "seat {seat} plays side {}, not the side given",
                    side.0
                )),
                Some(SeatRole::WhiteCell) => Err(format!(
                    "seat {seat} is a white-cell seat; it issues umpire commands"
                )),
                None => Err(format!("no seat {seat}")),
            },
            Issuer::WhiteCell { umpire } => {
                if self
                    .0
                    .values()
                    .any(|s| s.role == SeatRole::WhiteCell && &s.name == umpire)
                {
                    Ok(())
                } else {
                    Err(format!("{umpire:?} is not a white-cell seat"))
                }
            }
            Issuer::Scenario | Issuer::Ai | Issuer::Script => Ok(()),
        }
    }

    /// Seats a viewer of `side` may know about: its own side's (all of them for the white cell).
    #[must_use]
    pub fn visible(&self, side: Option<SideId>) -> Vec<Seat> {
        self.0
            .values()
            .filter(|s| side.is_none_or(|v| s.role == SeatRole::Side { side: v }))
            .cloned()
            .collect()
    }
}
