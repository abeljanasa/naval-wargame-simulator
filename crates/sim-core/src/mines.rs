//! Basic mines (Phase 2 slice 11; master prompt §8 "Mines (P1 basic)"): fields, laying and
//! transit risk. The encounter rule is in `sim-weapons`; model doc
//! `docs/models/weapons-damage.md` §Mines.
//!
//! Every [`MINE_CHECK_PERIOD_TICKS`], each ship or submarine (unit-id order) that moved since the
//! last check and is inside a field of another side (field-id order) takes one `Weapons` draw
//! against P = 1 − exp(−ρ·W·d). An encounter uses up one mine and rolls the mine's Ph like any
//! weapon arriving (damage, criticals, fire, flooding). The owning side knows its safe lanes
//! and is not at risk.

use crate::combat::{apply_hit_roll, Magazine, WeaponCatalog};
use crate::components::{Domain, Position, Unit};
use crate::events::{EventLog, SimEventKind};
use crate::ids::{SideId, UnitId};
use crate::rng::{RngStreams, StreamId};
use crate::sim::UnitIndex;
use crate::time::SimClock;
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use sim_geo::{inverse, GeoPoint};
use sim_weapons::{mine_encounter_probability, mine_swept_width_m, WeaponKind};
use std::collections::BTreeMap;

/// Transit risk is checked every 10 s.
pub const MINE_CHECK_PERIOD_TICKS: u64 = 10;

/// A circular minefield.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Minefield {
    pub id: u32,
    pub side: SideId,
    pub centre: GeoPoint,
    pub radius_m: f64,
    /// Mines left.
    pub mines: u32,
    /// Mine type (platform database weapon id).
    pub weapon: String,
}

/// Every minefield, and where each unit was at the last check.
#[derive(Debug, Clone, Default, Resource, Serialize, Deserialize)]
pub struct Minefields {
    pub next_id: u32,
    pub fields: Vec<Minefield>,
    pub last_position: BTreeMap<UnitId, GeoPoint>,
}

impl Minefields {
    pub(crate) fn add(
        &mut self,
        side: SideId,
        centre: GeoPoint,
        radius_m: f64,
        mines: u32,
        weapon: String,
    ) -> u32 {
        self.next_id += 1;
        let id = self.next_id;
        self.fields.push(Minefield {
            id,
            side,
            centre,
            radius_m,
            mines,
            weapon,
        });
        id
    }

    /// Fields shown to `side` (its own), or every field for the white cell (`None`).
    pub(crate) fn visible(&self, side: Option<SideId>) -> Vec<Minefield> {
        self.fields
            .iter()
            .filter(|f| side.is_none_or(|s| s == f.side))
            .cloned()
            .collect()
    }
}

/// Lays `mines` mines from `unit`'s stores in a circle of `radius_m` round its position.
pub(crate) fn lay_mines(
    world: &mut World,
    unit: UnitId,
    entity: Entity,
    mines: u32,
    radius_m: f64,
) -> Result<u32, String> {
    let u = world
        .get::<Unit>(entity)
        .cloned()
        .ok_or_else(|| format!("{unit} has no identity"))?;
    if !matches!(u.domain, Domain::Surface | Domain::Subsurface) {
        return Err(format!("{unit} cannot lay mines from the air or land"));
    }
    let at = world
        .get::<Position>(entity)
        .map(|p| p.point)
        .ok_or_else(|| format!("{unit} has no position"))?;
    let catalog = world.resource::<WeaponCatalog>().0.clone();
    let carried = world.get::<Magazine>(entity).and_then(|m| {
        m.0.iter()
            .find(|(w, n)| **n > 0 && catalog.get(*w).is_some_and(|s| s.kind == WeaponKind::Mine))
            .map(|(w, n)| (w.clone(), *n))
    });
    let (weapon, held) = carried.ok_or_else(|| format!("{unit} carries no mines"))?;
    if held < mines {
        return Err(format!("{unit} has {held} × {weapon}, not {mines}"));
    }
    if let Some(mut m) = world.get_mut::<Magazine>(entity) {
        if held == mines {
            m.0.remove(&weapon);
        } else {
            m.0.insert(weapon.clone(), held - mines);
        }
    }
    Ok(world
        .resource_mut::<Minefields>()
        .add(u.side, at, radius_m, mines, weapon))
}

pub(crate) fn mine_system(world: &mut World) {
    let tick = world.resource::<SimClock>().tick;
    if !tick.0.is_multiple_of(MINE_CHECK_PERIOD_TICKS) {
        return;
    }
    if world.resource::<Minefields>().fields.is_empty() {
        return;
    }
    let mut ships: Vec<(UnitId, SideId, GeoPoint)> = world
        .query::<(&Unit, &Position)>()
        .iter(world)
        .filter(|(u, _)| matches!(u.domain, Domain::Surface | Domain::Subsurface))
        .map(|(u, p)| (u.id, u.side, p.point))
        .collect();
    ships.sort_by_key(|(id, _, _)| *id);
    let catalog = world.resource::<WeaponCatalog>().0.clone();
    for (id, side, at) in ships {
        let previous = world
            .resource_mut::<Minefields>()
            .last_position
            .insert(id, at);
        let moved = previous.map_or(0.0, |p| inverse(p, at).distance_m);
        if moved == 0.0 {
            continue;
        }
        let fields: Vec<Minefield> = world.resource::<Minefields>().fields.clone();
        for f in fields {
            if f.side == side || f.mines == 0 {
                continue;
            }
            if inverse(f.centre, at).distance_m > f.radius_m {
                continue;
            }
            let Some(spec) = catalog.get(&f.weapon) else {
                continue;
            };
            let p =
                mine_encounter_probability(f.mines, f.radius_m, mine_swept_width_m(spec), moved);
            let draw = world
                .resource_mut::<RngStreams>()
                .uniform(StreamId::Weapons);
            let encountered = draw < p;
            world.resource_mut::<EventLog>().push(
                tick,
                SimEventKind::MineEncounter {
                    unit: id,
                    field: f.id,
                    distance_m: moved,
                    p,
                    draw,
                    encountered,
                },
            );
            if encountered {
                if let Some(field) = world
                    .resource_mut::<Minefields>()
                    .fields
                    .iter_mut()
                    .find(|x| x.id == f.id)
                {
                    field.mines -= 1;
                }
                apply_hit_roll(world, spec, None, None, id, tick);
                if !world.resource::<UnitIndex>().0.contains_key(&id) {
                    break; // sunk
                }
            }
        }
    }
}
