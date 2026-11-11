//! Operational planning layer (master prompt §7.1; `docs/models/planning-layer.md`).
//!
//! - **Facilities** of a side: command posts (orders reach only units within range of one),
//!   refuelling points, sheltering ports and anchorages (repair and rearm for units stopped
//!   alongside, berth limits).
//! - **Zones** of a side, often in pairs at a choke point:
//!   - a *sense* zone sets the doctrine of the side's own units inside it (EMCON, weapons
//!     control) and raises an alert when the side gains a contact inside it;
//!   - an *engage* zone sets the weapons control and ROE for engaging targets inside it.

use crate::c2::C2State;
use crate::combat::{Health, Magazine};
use crate::command::Command;
use crate::components::{Kinematics, Position, Unit};
use crate::detection::SensorSuite;
use crate::events::{EventLog, SimEventKind};
use crate::hash::StateHash;
use crate::ids::{SideId, UnitId};
use crate::scenario_events::{Message, MessageLog};
use crate::sim::UnitIndex;
use crate::time::{SimClock, Tick};
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use sim_c2::{Doctrine, ResolvedDoctrine};
use sim_events::{Area, Audience};
use sim_geo::GeoPoint;
use std::collections::{BTreeMap, BTreeSet};

/// How often facilities serve the units alongside.
pub const SERVICE_PERIOD_TICKS: u64 = 60;
/// A unit counts as alongside (or at anchor) at or below this speed.
pub const ALONGSIDE_SPEED_MPS: f64 = 0.5;
/// Repair removes this fraction of the unit's damage points per minute alongside.
pub const REPAIR_FRACTION_PER_MIN: f64 = 0.02;
/// Time alongside before a unit is rearmed.
pub const REARM_TICKS: u64 = 1_800;

/// What a facility is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum FacilityKind {
    CommandPost,
    RefuellingPoint,
    ShelteringPort,
    Anchorage,
}

/// What a facility does for units of its side stopped within its radius.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Services {
    /// Puts out fires, stops flooding and removes damage; a full repair restores systems.
    #[serde(default)]
    pub repair: bool,
    /// Restores the unit's weapons to its loadout after [`REARM_TICKS`] alongside.
    #[serde(default)]
    pub rearm: bool,
    /// Refuelling (recorded; fuel is not modelled until air operations, Phase 4).
    #[serde(default)]
    pub refuel: bool,
}

/// A planning-layer facility.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Facility {
    pub id: u32,
    pub name: String,
    pub side: SideId,
    pub kind: FacilityKind,
    pub position: GeoPoint,
    /// Units within this distance are alongside (ports, anchorages) or at the post.
    pub radius_m: f64,
    /// Command posts: orders reach the side's units within this range.
    #[serde(default)]
    pub range_m: Option<f64>,
    /// Units served at once (in unit-id order); 0 = no limit.
    #[serde(default)]
    pub berths: u32,
    #[serde(default)]
    pub services: Services,
}

/// What a zone governs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum ZoneKind {
    /// Own units inside take the zone's doctrine; contacts gained inside raise an alert.
    Sense,
    /// Targets inside are engaged under the zone's weapons control and ROE.
    Engage,
}

/// A sense or engage zone of one side.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Zone {
    pub id: u32,
    pub name: String,
    pub side: SideId,
    pub kind: ZoneKind,
    pub area: Area,
    /// Fields set here override the unit's own, its group's and its side's.
    #[serde(default)]
    pub doctrine: Doctrine,
    /// The choke point this zone belongs to, if any (for display and grouping).
    #[serde(default)]
    pub choke_point: Option<String>,
}

/// A unit being served at a facility.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServiceState {
    pub facility: u32,
    pub since: Tick,
    pub rearmed: bool,
}

/// A unit's systems and weapons as it entered the exercise: what repair and rearming restore.
#[derive(Debug, Clone, PartialEq, Component, Serialize, Deserialize)]
pub struct Loadout {
    pub sensors: Vec<crate::detection::SensorSpec>,
    pub max_speed_mps: f64,
    pub stores: BTreeMap<String, u32>,
}

/// The planning layer in the world.
#[derive(Debug, Clone, Default, PartialEq, Resource, Serialize, Deserialize)]
pub struct PlanningLayer {
    /// Sorted by id.
    #[serde(default)]
    pub facilities: Vec<Facility>,
    /// Sorted by id.
    #[serde(default)]
    pub zones: Vec<Zone>,
    /// Units alongside a facility.
    #[serde(default)]
    pub service: BTreeMap<UnitId, ServiceState>,
}

/// Checks a planning layer before it is set.
pub fn validate_planning(facilities: &[Facility], zones: &[Zone]) -> Result<(), String> {
    let mut ids = BTreeSet::new();
    for f in facilities {
        if !ids.insert(f.id) {
            return Err(format!("duplicate facility id {}", f.id));
        }
        if f.name.trim().is_empty() {
            return Err(format!("facility {} has no name", f.id));
        }
        if !f.position.is_valid() || !(f.radius_m.is_finite() && f.radius_m > 0.0) {
            return Err(format!(
                "facility {:?} needs a valid position and a radius > 0",
                f.name
            ));
        }
        if f.range_m.is_some_and(|r| !(r.is_finite() && r > 0.0)) {
            return Err(format!("facility {:?} has a range that is not > 0", f.name));
        }
    }
    let mut ids = BTreeSet::new();
    for z in zones {
        if !ids.insert(z.id) {
            return Err(format!("duplicate zone id {}", z.id));
        }
        if z.name.trim().is_empty() {
            return Err(format!("zone {} has no name", z.id));
        }
        z.area
            .validate()
            .map_err(|e| format!("zone {:?}: {e}", z.name))?;
    }
    Ok(())
}

impl PlanningLayer {
    /// Replaces the facilities and zones (sorted by id). Service in progress at facilities that
    /// still exist carries on.
    pub fn set(&mut self, mut facilities: Vec<Facility>, mut zones: Vec<Zone>) {
        facilities.sort_by_key(|f| f.id);
        zones.sort_by_key(|z| z.id);
        let kept: BTreeSet<u32> = facilities.iter().map(|f| f.id).collect();
        self.service.retain(|_, s| kept.contains(&s.facility));
        self.facilities = facilities;
        self.zones = zones;
    }

    /// The first zone (by id) of `side` and `kind` containing `p`.
    #[must_use]
    pub fn zone_at(&self, side: SideId, kind: ZoneKind, p: GeoPoint) -> Option<&Zone> {
        self.zones
            .iter()
            .find(|z| z.side == side && z.kind == kind && z.area.contains(p))
    }

    /// The doctrine in force for `unit` at `at`: a sense zone of its side there overrides the
    /// unit's, group's and side's doctrine for the fields it sets.
    #[must_use]
    pub fn doctrine_for(&self, c2: &C2State, unit: &Unit, at: GeoPoint) -> ResolvedDoctrine {
        match self.zone_at(unit.side, ZoneKind::Sense, at) {
            Some(z) => c2.doctrine_over(unit, &z.doctrine),
            None => c2.doctrine_for(unit),
        }
    }

    /// The doctrine for `unit` (at `at`) engaging a target believed to be at `target`: an
    /// engage zone of its side containing the target governs; otherwise as
    /// [`PlanningLayer::doctrine_for`].
    #[must_use]
    pub fn engage_doctrine(
        &self,
        c2: &C2State,
        unit: &Unit,
        at: GeoPoint,
        target: GeoPoint,
    ) -> ResolvedDoctrine {
        match self.zone_at(unit.side, ZoneKind::Engage, target) {
            Some(z) => c2.doctrine_over(unit, &z.doctrine),
            None => self.doctrine_for(c2, unit, at),
        }
    }

    /// Adds the layer to the state hash (nothing when empty, so scenarios without one keep
    /// their hashes).
    pub(crate) fn hash(&self, h: &mut StateHash) {
        if self.facilities.is_empty() && self.zones.is_empty() && self.service.is_empty() {
            return;
        }
        // Serialised form: every field, in a fixed order.
        if let Ok(bytes) = rmp_serde::to_vec(self) {
            h.u64(bytes.len() as u64);
            for b in bytes {
                h.u8(b);
            }
        }
    }
}

/// Refuses a player's order to a unit beyond the range of every command post of its side
/// (only when the side has command posts).
pub(crate) fn check_command_post(world: &World, cmd: &Command) -> Result<(), String> {
    if !matches!(cmd.issuer, crate::command::Issuer::Player { .. }) {
        return Ok(());
    }
    let (Some(side), Some(unit)) = (cmd.side, cmd.payload.target_unit()) else {
        return Ok(());
    };
    let layer = world.resource::<PlanningLayer>();
    let mut posts = layer
        .facilities
        .iter()
        .filter(|f| f.side == side && f.kind == FacilityKind::CommandPost)
        .peekable();
    if posts.peek().is_none() {
        return Ok(());
    }
    let Some(at) = world
        .resource::<UnitIndex>()
        .0
        .get(&unit)
        .and_then(|e| world.get::<Position>(*e))
        .map(|p| p.point)
    else {
        return Ok(()); // the order itself reports the unknown unit
    };
    if posts.any(|f| sim_geo::inverse(f.position, at).distance_m <= f.range_m.unwrap_or(f.radius_m))
    {
        Ok(())
    } else {
        Err(format!("{unit} is out of range of every command post"))
    }
}

/// Each tick: alerts for contacts gained in sense zones; every [`SERVICE_PERIOD_TICKS`]:
/// repair and rearming at facilities.
pub(crate) fn planning_system(world: &mut World) {
    if world.resource::<PlanningLayer>().facilities.is_empty()
        && world.resource::<PlanningLayer>().zones.is_empty()
    {
        return;
    }
    let tick = world.resource::<SimClock>().tick;
    sense_alerts(world, tick);
    if tick.0.is_multiple_of(SERVICE_PERIOD_TICKS) {
        serve(world, tick);
    }
}

fn sense_alerts(world: &mut World, tick: Tick) {
    let gained: Vec<(SideId, u32, UnitId)> = world
        .resource::<EventLog>()
        .events
        .iter()
        .rev()
        .take_while(|e| e.tick == tick)
        .filter_map(|e| match e.kind {
            SimEventKind::ContactGained {
                side,
                number,
                target,
            } => Some((side, number, target)),
            _ => None,
        })
        .collect();
    for (side, number, target) in gained.into_iter().rev() {
        let Some(at) = world
            .resource::<UnitIndex>()
            .0
            .get(&target)
            .and_then(|e| world.get::<Position>(*e))
            .map(|p| p.point)
        else {
            continue;
        };
        let Some((zone, name)) = world
            .resource::<PlanningLayer>()
            .zone_at(side, ZoneKind::Sense, at)
            .map(|z| (z.id, z.name.clone()))
        else {
            continue;
        };
        world.resource_mut::<MessageLog>().0.push(Message {
            tick,
            from: name.clone(),
            to: Audience::Side { side: side.0 },
            text: format!("C{number} detected in {name}"),
            title: None,
            inject: None,
        });
        world.resource_mut::<EventLog>().push(
            tick,
            SimEventKind::ZoneAlert {
                side,
                zone,
                contact: number,
            },
        );
    }
}

fn serve(world: &mut World, tick: Tick) {
    let facilities: Vec<Facility> = world
        .resource::<PlanningLayer>()
        .facilities
        .iter()
        .filter(|f| f.services.repair || f.services.rearm)
        .cloned()
        .collect();
    // Units that could be alongside, in id order.
    let mut units: Vec<(UnitId, Entity, SideId, GeoPoint, f64)> = world
        .query::<(Entity, &Unit, &Position, &Kinematics)>()
        .iter(world)
        .map(|(e, u, p, k)| (u.id, e, u.side, p.point, k.speed_mps))
        .collect();
    units.sort_by_key(|u| u.0);
    let mut alongside: BTreeMap<UnitId, (u32, Entity, Services)> = BTreeMap::new();
    for f in &facilities {
        let mut served = 0u32;
        for &(id, e, side, at, speed) in &units {
            if side != f.side
                || speed > ALONGSIDE_SPEED_MPS
                || alongside.contains_key(&id)
                || sim_geo::inverse(f.position, at).distance_m > f.radius_m
            {
                continue;
            }
            if f.berths > 0 && served >= f.berths {
                break;
            }
            served += 1;
            alongside.insert(id, (f.id, e, f.services));
        }
    }
    // Units that left (or whose facility changed) start again.
    {
        let mut layer = world.resource_mut::<PlanningLayer>();
        layer
            .service
            .retain(|id, s| alongside.get(id).is_some_and(|a| a.0 == s.facility));
        for (id, (fac, _, _)) in &alongside {
            layer.service.entry(*id).or_insert(ServiceState {
                facility: *fac,
                since: tick,
                rearmed: false,
            });
        }
    }
    for (id, (fac, e, services)) in alongside {
        if services.repair {
            repair(world, id, e, fac, tick);
        }
        if services.rearm {
            let due = world
                .resource::<PlanningLayer>()
                .service
                .get(&id)
                .is_some_and(|s| !s.rearmed && tick.0 - s.since.0 >= REARM_TICKS);
            if due {
                if let Some(stores) = world.get::<Loadout>(e).map(|l| l.stores.clone()) {
                    if let Some(mut m) = world.get_mut::<Magazine>(e) {
                        m.0 = stores;
                    }
                }
                if let Some(s) = world.resource_mut::<PlanningLayer>().service.get_mut(&id) {
                    s.rearmed = true;
                }
                world.resource_mut::<EventLog>().push(
                    tick,
                    SimEventKind::UnitRearmed {
                        unit: id,
                        facility: fac,
                    },
                );
            }
        }
    }
}

fn repair(world: &mut World, id: UnitId, e: Entity, facility: u32, tick: Tick) {
    let Some(mut h) = world.get_mut::<Health>(e) else {
        return;
    };
    if h.damage <= 0.0 && h.fires == 0 && !h.flooding && h.criticals.is_empty() {
        return;
    }
    h.fires = 0;
    h.flooding = false;
    let step = REPAIR_FRACTION_PER_MIN * h.profile.damage_points;
    h.damage = (h.damage - step).max(0.0);
    let complete = h.damage <= 0.0;
    if complete {
        h.criticals.clear();
    }
    h.status = sim_weapons::Status::assess(&h.profile, h.damage, &h.criticals);
    if !complete {
        return;
    }
    // A full repair restores the sensors and speed lost to critical hits.
    if let Some(l) = world.get::<Loadout>(e).cloned() {
        if let Some(mut s) = world.get_mut::<SensorSuite>(e) {
            s.0 = l.sensors;
        }
        if let Some(mut k) = world.get_mut::<Kinematics>(e) {
            k.max_speed_mps = l.max_speed_mps;
        }
    }
    world
        .resource_mut::<EventLog>()
        .push(tick, SimEventKind::UnitRepaired { unit: id, facility });
}
