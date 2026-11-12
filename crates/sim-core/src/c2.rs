//! Command-and-control state in the world: postures between sides and doctrine set on sides and
//! groups (unit doctrine lives on [`Unit`]). The rules are in `sim-c2` (ADR-0005); model doc
//! `docs/models/c2-doctrine.md`.

use crate::components::Unit;
use crate::ids::SideId;
use bevy_ecs::prelude::Resource;
use serde::{Deserialize, Serialize};
use sim_c2::{resolve, Doctrine, Posture, ResolvedDoctrine};
use std::collections::BTreeMap;

/// Postures and side/group doctrine. Every map is a `BTreeMap`, so iteration (and the hash) is
/// in key order (ADR-0003).
#[derive(Debug, Clone, Default, Resource, Serialize, Deserialize)]
pub struct C2State {
    /// (side, toward) → how `side` regards `toward`. Missing pairs are neutral.
    #[serde(default)]
    pub postures: BTreeMap<(SideId, SideId), Posture>,
    #[serde(default)]
    pub side_doctrine: BTreeMap<SideId, Doctrine>,
    /// Keyed by (side, group name): group names are per side.
    #[serde(default)]
    pub group_doctrine: BTreeMap<(SideId, String), Doctrine>,
}

impl C2State {
    /// How `side` regards `toward`. A side is always friendly to itself.
    #[must_use]
    pub fn posture(&self, side: SideId, toward: SideId) -> Posture {
        if side == toward {
            return Posture::Friendly;
        }
        self.postures
            .get(&(side, toward))
            .copied()
            .unwrap_or_default()
    }

    /// The doctrine in force for a unit: unit, then its group, then its side, then the default.
    #[must_use]
    pub fn doctrine_for(&self, unit: &Unit) -> ResolvedDoctrine {
        let none = Doctrine::default();
        let group = unit
            .group
            .as_ref()
            .and_then(|g| self.group_doctrine.get(&(unit.side, g.clone())))
            .unwrap_or(&none);
        let side = self.side_doctrine.get(&unit.side).unwrap_or(&none);
        resolve(&[&unit.doctrine, group, side])
    }

    /// The doctrine for a unit with `over` (a zone's rules) above its own, its group's and its
    /// side's.
    #[must_use]
    pub fn doctrine_over(&self, unit: &Unit, over: &sim_c2::Doctrine) -> ResolvedDoctrine {
        let none = sim_c2::Doctrine::default();
        let group = unit
            .group
            .as_ref()
            .and_then(|g| self.group_doctrine.get(&(unit.side, g.clone())))
            .unwrap_or(&none);
        let side = self.side_doctrine.get(&unit.side).unwrap_or(&none);
        resolve(&[over, &unit.doctrine, group, side])
    }
}
