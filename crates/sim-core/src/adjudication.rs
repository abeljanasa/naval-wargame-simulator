//! White-cell adjudication overrides (master prompt §10 "manual adjudication override (force
//! hit/miss, adjust damage, move/add/remove units) — every override logged with umpire ID and
//! reason"). Phase 3 slice 1; behaviour doc `docs/models/adjudication.md`.
//!
//! An override is a command (`Adjudicate`), so it is in the command log and replays exactly.
//! Only an `Issuer::WhiteCell` may adjudicate, and a reason is required. Adding units uses the
//! existing `SpawnUnit` command, which the white cell may already issue.

use crate::combat::{apply_hit, destroy, Health, Magazine, WeaponCatalog, WeaponsInFlight};
use crate::components::{Position, Route};
use crate::events::{EventLog, SimEventKind};
use crate::ids::UnitId;
use crate::sim::UnitIndex;
use crate::time::{SimClock, Tick};
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use sim_geo::GeoPoint;
use sim_weapons::{Status, WeaponKind};

/// How many recent overrides the white-cell picture carries.
pub const ADJUDICATION_VIEW_LIMIT: usize = 20;

/// One override.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Adjudication {
    /// Resolve a weapon in flight now: a hit does its full damage with no Ph roll; a miss
    /// removes it.
    ForceWeaponOutcome {
        #[cfg_attr(feature = "specta", specta(type = u32))]
        weapon_id: u64,
        hit: bool,
    },
    /// Set a unit's damage, in damage points. `clear_effects` also puts out fires, stops
    /// flooding and clears critical-hit flags (a repair).
    SetDamage {
        unit: UnitId,
        damage_dp: f64,
        clear_effects: bool,
    },
    /// Sink or shoot down a unit (logged as destroyed).
    DestroyUnit { unit: UnitId },
    /// Take a unit out of the exercise without it counting as destroyed.
    RemoveUnit { unit: UnitId },
    /// Place a unit somewhere else; its route is cleared.
    MoveUnit {
        unit: UnitId,
        position: GeoPoint,
        altitude_m: Option<f64>,
    },
    /// Set the rounds of one weapon a unit carries (0 removes it).
    SetStores {
        unit: UnitId,
        weapon: String,
        count: u32,
    },
}

impl Adjudication {
    pub(crate) fn validate(&self) -> Result<(), String> {
        match self {
            Self::SetDamage { damage_dp, .. } if !(damage_dp.is_finite() && *damage_dp >= 0.0) => {
                Err("damage must be finite and >= 0".into())
            }
            Self::MoveUnit {
                position,
                altitude_m,
                ..
            } if !position.is_valid() || altitude_m.is_some_and(|a| !a.is_finite()) => {
                Err("move needs a valid position".into())
            }
            Self::SetStores { weapon, .. } if weapon.trim().is_empty() => {
                Err("stores need a weapon id".into())
            }
            _ => Ok(()),
        }
    }
}

/// An override as recorded.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct AdjudicationRecord {
    pub tick: Tick,
    pub umpire: String,
    pub reason: String,
    pub action: Adjudication,
}

/// Every override, in order.
#[derive(Debug, Clone, Default, Resource, Serialize, Deserialize)]
pub struct AdjudicationLog(pub Vec<AdjudicationRecord>);

fn entity(world: &World, unit: UnitId) -> Result<Entity, String> {
    world
        .resource::<UnitIndex>()
        .0
        .get(&unit)
        .copied()
        .ok_or_else(|| format!("unknown unit {unit}"))
}

/// Applies an override from `umpire`. The record and the `Adjudicated` event are written first,
/// so the log explains every change that follows it.
#[allow(clippy::too_many_lines)]
pub(crate) fn adjudicate(
    world: &mut World,
    umpire: &str,
    reason: &str,
    action: &Adjudication,
) -> Result<(), String> {
    let tick = world.resource::<SimClock>().tick;
    // Check the target exists before logging anything.
    match action {
        Adjudication::ForceWeaponOutcome { weapon_id, .. } => {
            if !world
                .resource::<WeaponsInFlight>()
                .list
                .iter()
                .any(|w| w.id == *weapon_id)
            {
                return Err(format!("no weapon {weapon_id} in flight"));
            }
        }
        Adjudication::SetStores { unit, weapon, .. } => {
            entity(world, *unit)?;
            if !world.resource::<WeaponCatalog>().0.contains_key(weapon) {
                return Err(format!("unknown weapon {weapon}"));
            }
        }
        Adjudication::SetDamage { unit, .. }
        | Adjudication::DestroyUnit { unit }
        | Adjudication::RemoveUnit { unit }
        | Adjudication::MoveUnit { unit, .. } => {
            entity(world, *unit)?;
        }
    }
    let record = AdjudicationRecord {
        tick,
        umpire: umpire.to_owned(),
        reason: reason.to_owned(),
        action: action.clone(),
    };
    world.resource_mut::<EventLog>().push(
        tick,
        SimEventKind::Adjudicated {
            umpire: record.umpire.clone(),
            reason: record.reason.clone(),
            action: action.clone(),
        },
    );
    world.resource_mut::<AdjudicationLog>().0.push(record);

    match action {
        Adjudication::ForceWeaponOutcome { weapon_id, hit } => {
            let w = {
                let mut wif = world.resource_mut::<WeaponsInFlight>();
                let idx = wif.list.iter().position(|w| w.id == *weapon_id);
                idx.map(|i| wif.list.remove(i))
            };
            let Some(w) = w else {
                return Ok(());
            };
            if *hit {
                let spec = world.resource::<WeaponCatalog>().0.get(&w.weapon).cloned();
                if let Some(spec) = spec {
                    apply_hit(
                        world,
                        w.target,
                        spec.warhead_kg,
                        matches!(spec.kind, WeaponKind::Torpedo | WeaponKind::Mine),
                        tick,
                    );
                }
            } else {
                world.resource_mut::<EventLog>().push(
                    tick,
                    SimEventKind::WeaponMissed {
                        weapon_id: *weapon_id,
                        reason: "adjudicated miss".into(),
                    },
                );
            }
        }
        Adjudication::SetDamage {
            unit,
            damage_dp,
            clear_effects,
        } => {
            let e = entity(world, *unit)?;
            let status = world.get_mut::<Health>(e).map(|mut h| {
                h.damage = *damage_dp;
                if *clear_effects {
                    h.fires = 0;
                    h.flooding = false;
                    h.criticals.clear();
                }
                h.status = Status::assess(&h.profile, h.damage, &h.criticals);
                h.status
            });
            if status == Some(Status::Destroyed) {
                destroy(world, *unit, e, tick);
            }
        }
        Adjudication::DestroyUnit { unit } => {
            let e = entity(world, *unit)?;
            destroy(world, *unit, e, tick);
        }
        Adjudication::RemoveUnit { unit } => {
            let e = entity(world, *unit)?;
            world.despawn(e);
            world.resource_mut::<UnitIndex>().0.remove(unit);
        }
        Adjudication::MoveUnit {
            unit,
            position,
            altitude_m,
        } => {
            let e = entity(world, *unit)?;
            if let Some(mut p) = world.get_mut::<Position>(e) {
                p.point = *position;
                if let Some(a) = altitude_m {
                    p.altitude_m = *a;
                }
            }
            if let Some(mut r) = world.get_mut::<Route>(e) {
                r.waypoints.clear();
                r.next = 0;
            }
        }
        Adjudication::SetStores {
            unit,
            weapon,
            count,
        } => {
            let e = entity(world, *unit)?;
            if let Some(mut m) = world.get_mut::<Magazine>(e) {
                if *count == 0 {
                    m.0.remove(weapon);
                } else {
                    m.0.insert(weapon.clone(), *count);
                }
            }
        }
    }
    Ok(())
}
