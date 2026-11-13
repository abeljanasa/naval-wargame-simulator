//! Air operations (Phase 4 slices 9–11; `docs/models/air-operations.md`).
//!
//! - **Airbases** are planning-layer facilities of kind `airbase`, ashore or carried by a ship (a
//!   carrier or a ship with a flight deck). Each has runways (or deck spots), a launch interval,
//!   parking for so many aircraft, and a stock of aviation fuel.
//! - **Aircraft** with a base start parked, at a readiness: ready, alert 15, alert 60 or in
//!   maintenance. A parked aircraft is off the map: its whole record waits in the base's hangar
//!   (the [`Air`] resource), so sensors, weapons and AI never see it.
//! - **Sorties:** a launch order gives a mission and (optionally) a loadout; the aircraft launches
//!   after its readiness delay and any rearming, when a runway is free. It returns when the
//!   mission is complete, its target is gone, its weapons are spent, its fuel reaches bingo, or
//!   it is recalled; it lands when a runway is free (recoveries before launches), then turns
//!   round (refuel and rearm) for a time set by its loadout.
//! - **Fuel** (with the scenario's fuel model): aircraft burn aviation fuel by phase of flight;
//!   bingo is the fuel to reach a base plus a reserve; an aircraft out of fuel is lost. Tankers
//!   on station refuel aircraft at bingo that are nearer to them than to a base, a hose at a time.
//!
//! Everything runs in unit-id order with no random draws.

use crate::ai::MissionState;
use crate::combat::Magazine;
use crate::components::{Kinematics, Position, Route, Unit};
use crate::detection::ContactTable;
use crate::events::{EventLog, SimEventKind};
use crate::fuel::Fuel;
use crate::ids::{SideId, UnitId};
use crate::planning::{Facility, FacilityKind, Loadout, PlanningLayer};
use crate::save::UnitRecord;
use crate::sim::UnitIndex;
use crate::time::{SimClock, Tick, TICK_SECONDS};
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use sim_ai::{Mission, AI_PERIOD_TICKS};
use sim_geo::{inverse, GeoPoint};
use std::collections::BTreeMap;

/// Launch delay at alert 15 and alert 60, s.
pub const ALERT15_S: u64 = 900;
pub const ALERT60_S: u64 = 3_600;
/// Seconds between launches (or recoveries) on one runway ashore, and from one deck spot.
pub const LAND_INTERVAL_S: f64 = 120.0;
pub const DECK_INTERVAL_S: f64 = 60.0;
/// Turnaround (refuel, checks) of a fighter, helicopter or UAV, and of a large aircraft, s.
pub const TURNAROUND_S: u64 = 1_200;
pub const LARGE_TURNAROUND_S: u64 = 2_400;
/// Rearming time per weapon loaded, s.
pub const REARM_PER_WEAPON_S: u64 = 120;
/// Fuel spent on takeoff and climb, as a share of capacity (fixed wing; VTOL half of it).
pub const TAKEOFF_FUEL_FRACTION: f64 = 0.02;
/// Fuel kept in reserve on landing, as a share of capacity.
pub const RESERVE_FRACTION: f64 = 0.1;
/// An aircraft is over its base (and can be recovered) within this distance, m.
pub const RECOVERY_RADIUS_M: f64 = 3_000.0;
/// A receiver joins a tanker within this distance, and leaves it beyond five times it, m.
pub const TANKER_CONTACT_M: f64 = 2_000.0;
/// Generic tanker: hoses, and tonnes a minute through each.
pub const DEFAULT_HOSES: u32 = 2;
pub const DEFAULT_TRANSFER_T_PER_MIN: f64 = 1.0;
/// A tanker keeps this share of its capacity over its own bingo.
pub const TANKER_MARGIN_FRACTION: f64 = 0.05;
/// A receiver counts as full above this share.
pub const FULL_FRACTION: f64 = 0.98;

/// How soon a parked aircraft can launch once tasked.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum Readiness {
    /// Crewed and armed: launches when a runway is free.
    #[default]
    Ready,
    /// Launches 15 minutes after tasking.
    Alert15,
    /// Launches 60 minutes after tasking.
    Alert60,
    /// Not available until maintenance ends (or the readiness is changed).
    Maintenance,
}

impl Readiness {
    /// Delay from tasking to launch, s.
    #[must_use]
    pub fn delay_s(self) -> u64 {
        match self {
            Self::Ready | Self::Maintenance => 0,
            Self::Alert15 => ALERT15_S,
            Self::Alert60 => ALERT60_S,
        }
    }

    fn code(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Alert15 => "alert 15",
            Self::Alert60 => "alert 60",
            Self::Maintenance => "maintenance",
        }
    }
}

fn one() -> u32 {
    1
}

/// The runways (or deck) of an airbase facility.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct AirField {
    /// Runways (ashore) or launch and recovery spots (deck), each launching or recovering one
    /// aircraft per interval.
    #[serde(default = "one")]
    pub runways: u32,
    /// Seconds between launches or recoveries on one runway (default 120 ashore, 60 on a deck).
    #[serde(default)]
    pub interval_s: Option<f64>,
    /// Aircraft it can park; 0 = no limit.
    #[serde(default)]
    pub parking: u32,
    /// A ship's flight deck.
    #[serde(default)]
    pub deck: bool,
    /// Fixed-wing aircraft can use it (default: yes ashore, no on a deck; a carrier sets it).
    #[serde(default)]
    pub fixed_wing: Option<bool>,
    /// Aviation fuel held at the start, t; `None` = unlimited.
    #[serde(default)]
    pub aviation_fuel_t: Option<f64>,
}

impl AirField {
    pub(crate) fn validate(&self, name: &str) -> Result<(), String> {
        if self.runways == 0 {
            return Err(format!(
                "airbase {name:?} needs at least one runway or deck spot"
            ));
        }
        if self.interval_s.is_some_and(|s| !(s.is_finite() && s > 0.0))
            || self
                .aviation_fuel_t
                .is_some_and(|t| !(t.is_finite() && t >= 0.0))
        {
            return Err(format!(
                "airbase {name:?} needs a launch interval > 0 and aviation fuel >= 0"
            ));
        }
        Ok(())
    }

    #[must_use]
    pub fn interval(&self) -> f64 {
        self.interval_s.unwrap_or(if self.deck {
            DECK_INTERVAL_S
        } else {
            LAND_INTERVAL_S
        })
    }

    /// Whether an aircraft can use it.
    #[must_use]
    pub fn accepts(&self, spec: &AirSpec) -> bool {
        spec.vtol || self.fixed_wing.unwrap_or(!self.deck)
    }
}

/// One loadout an aircraft can carry (from the platform database).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct LoadoutOption {
    pub id: String,
    pub name: String,
    /// `air_superiority`, `asuw`, `asw` or `strike`.
    pub role: String,
    pub stores: BTreeMap<String, u32>,
}

impl LoadoutOption {
    #[must_use]
    pub fn weapons(&self) -> u32 {
        self.stores.values().sum()
    }
}

fn hoses() -> u32 {
    DEFAULT_HOSES
}

fn transfer() -> f64 {
    DEFAULT_TRANSFER_T_PER_MIN
}

/// An air-to-air refuelling tanker.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TankerSpec {
    #[serde(default = "hoses")]
    pub hoses: u32,
    #[serde(default = "transfer")]
    pub transfer_t_per_min: f64,
}

/// What an aircraft brings to air operations, from its platform and the scenario.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct AirSpec {
    /// Its airbase facility.
    #[serde(default)]
    pub base: Option<u32>,
    /// Parked at its base at the start (else airborne).
    #[serde(default)]
    pub parked: bool,
    #[serde(default)]
    pub readiness: Readiness,
    /// In maintenance for this long at the start, s (readiness `maintenance`; none: until changed).
    #[serde(default)]
    pub maintenance_s: Option<f64>,
    /// The loadouts it can carry.
    #[serde(default)]
    pub loadouts: Vec<LoadoutOption>,
    /// The loadout carried at the start, if one of `loadouts`.
    #[serde(default)]
    pub loadout: Option<String>,
    /// A tanker.
    #[serde(default)]
    pub tanker: Option<TankerSpec>,
    /// Altitude it flies at once launched, m.
    pub cruise_altitude_m: f64,
    /// Cruise speed, m/s.
    pub cruise_mps: f64,
    /// Helicopter or VTOL: uses any deck, and spends less fuel climbing.
    #[serde(default)]
    pub vtol: bool,
    /// A large aircraft (patrol, early warning, tanker, bomber): longer turnaround.
    #[serde(default)]
    pub large: bool,
}

impl AirSpec {
    pub(crate) fn validate(&self) -> Result<(), String> {
        let pos = |v: f64| v.is_finite() && v > 0.0;
        if !(pos(self.cruise_mps) && self.cruise_altitude_m.is_finite())
            || self.maintenance_s.is_some_and(|s| !pos(s))
            || self
                .tanker
                .is_some_and(|t| t.hoses == 0 || !pos(t.transfer_t_per_min))
        {
            return Err("air: invalid cruise, maintenance or tanker figures".into());
        }
        if self.parked && self.base.is_none() {
            return Err("air: a parked aircraft needs a base".into());
        }
        if let Some(l) = &self.loadout {
            if !self.loadouts.iter().any(|o| &o.id == l) {
                return Err(format!("air: unknown loadout {l:?}"));
            }
        }
        Ok(())
    }

    fn option(&self, id: &str) -> Option<&LoadoutOption> {
        self.loadouts.iter().find(|o| o.id == id)
    }

    /// The loadout for a mission: the first, in the mission's order of roles, the aircraft has.
    #[must_use]
    pub fn loadout_for(&self, mission: &Mission) -> Option<&LoadoutOption> {
        let roles: &[&str] = match mission {
            Mission::Strike { .. } => &["asuw", "strike", "asw", "air_superiority"],
            Mission::Patrol { .. }
            | Mission::BarrierPatrol { .. }
            | Mission::Denial { .. }
            | Mission::Escort { .. } => &["air_superiority", "asw", "asuw", "strike"],
            _ => &[],
        };
        roles
            .iter()
            .find_map(|r| self.loadouts.iter().find(|o| o.role == *r))
    }

    fn turnaround_s(&self, loadout: Option<&LoadoutOption>) -> u64 {
        let base = if self.large {
            LARGE_TURNAROUND_S
        } else {
            TURNAROUND_S
        };
        base + REARM_PER_WEAPON_S * u64::from(loadout.map_or(0, LoadoutOption::weapons))
    }
}

/// Where an aircraft is in its cycle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum AirState {
    /// Spawned parked: put in the hangar on the first air pass.
    Starting,
    /// In the hangar; not available before `busy_until` (turnaround, maintenance).
    Parked {
        busy_until: Tick,
    },
    /// Tasked: launches at `ready_at` once a runway is free.
    Launching {
        ready_at: Tick,
        mission: Mission,
        loadout: Option<String>,
    },
    Airborne,
    /// Flying back to an airbase.
    Returning {
        base: u32,
        reason: String,
    },
    /// Flying to a tanker at bingo fuel.
    ToTanker {
        tanker: UnitId,
    },
    /// Taking fuel from a tanker.
    Tanking {
        tanker: UnitId,
        taken_t: f64,
    },
}

/// One aircraft under air operations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Aircraft {
    pub spec: AirSpec,
    /// Home base (where it returns), if any.
    pub base: Option<u32>,
    pub readiness: Readiness,
    /// End of a timed maintenance.
    pub maintenance_until: Option<Tick>,
    pub state: AirState,
    /// The loadout carried.
    pub loadout: Option<String>,
    /// The sortie's mission, kept while it goes to a tanker.
    pub suspended: Option<Mission>,
    /// It launched carrying weapons (so it returns when they are spent).
    pub armed: bool,
}

/// Air operations state (Phase 4 slices 9–11).
#[derive(Debug, Clone, Default, Resource, Serialize, Deserialize)]
pub struct Air {
    pub aircraft: BTreeMap<UnitId, Aircraft>,
    /// Records of aircraft in a hangar (off the map).
    pub parked: BTreeMap<UnitId, UnitRecord>,
    /// By airbase: the tick each runway is next free.
    pub runways: BTreeMap<u32, Vec<Tick>>,
    /// Aviation fuel used from each airbase's stock, t.
    pub fuel_used: BTreeMap<u32, f64>,
}

impl Air {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.aircraft.is_empty()
    }

    /// Whether `unit` is in a hangar.
    #[must_use]
    pub fn is_parked(&self, unit: UnitId) -> bool {
        self.parked.contains_key(&unit)
    }
}

/// Registers a spawned aircraft (from `SpawnUnit`).
pub(crate) fn register(world: &mut World, unit: UnitId, spec: &AirSpec) -> Result<(), String> {
    spec.validate()?;
    let tick = world.resource::<SimClock>().tick;
    let state = if spec.parked {
        AirState::Starting
    } else {
        AirState::Airborne
    };
    let maintenance_until = match (spec.readiness, spec.maintenance_s) {
        (Readiness::Maintenance, Some(s)) => Some(Tick(tick.0 + secs(s))),
        _ => None,
    };
    let armed = spec
        .loadout
        .as_deref()
        .and_then(|l| spec.option(l))
        .is_some_and(|o| o.weapons() > 0);
    world.resource_mut::<Air>().aircraft.insert(
        unit,
        Aircraft {
            spec: spec.clone(),
            base: spec.base,
            readiness: spec.readiness,
            maintenance_until,
            state,
            loadout: spec.loadout.clone(),
            suspended: None,
            armed,
        },
    );
    Ok(())
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn secs(s: f64) -> u64 {
    s.max(0.0).ceil() as u64
}

fn airbase(world: &World, id: u32) -> Option<Facility> {
    world
        .resource::<PlanningLayer>()
        .facility(id)
        .filter(|f| f.kind == FacilityKind::Airbase && f.air.is_some())
        .cloned()
}

fn base_point(world: &World, f: &Facility) -> Option<GeoPoint> {
    crate::planning::post_position(world, f)
}

/// Aircraft at (parked, launching) a base.
fn on_base(air: &Air, base: u32) -> u32 {
    #[allow(clippy::cast_possible_truncation)]
    let n = air
        .aircraft
        .values()
        .filter(|a| {
            a.base == Some(base)
                && matches!(
                    a.state,
                    AirState::Parked { .. } | AirState::Launching { .. } | AirState::Starting
                )
        })
        .count() as u32;
    n
}

/// Where `spec` (of `side`, now at `at`) can land: its home base if it can, else the nearest
/// airbase of its side that accepts it and has room. (`base id`, position, distance m.)
fn recovery_base(
    world: &World,
    air: &Air,
    side: SideId,
    craft: &Aircraft,
    at: GeoPoint,
) -> Option<(u32, GeoPoint, f64)> {
    let fits = |f: &Facility| {
        let a = f.air.as_ref()?;
        let room = a.parking == 0
            || on_base(air, f.id) < a.parking
            || (craft.base == Some(f.id) && on_base(air, f.id) <= a.parking);
        (f.side == side && a.accepts(&craft.spec) && room).then_some(())?;
        let p = base_point(world, f)?;
        Some((f.id, p, inverse(at, p).distance_m))
    };
    let layer = world.resource::<PlanningLayer>();
    if let Some(home) = craft.base.and_then(|b| layer.facility(b)) {
        if let Some(r) = fits(home).filter(|_| home.kind == FacilityKind::Airbase) {
            return Some(r);
        }
    }
    layer
        .facilities
        .iter()
        .filter(|f| f.kind == FacilityKind::Airbase)
        .filter_map(fits)
        .min_by(|a, b| a.2.total_cmp(&b.2).then(a.0.cmp(&b.0)))
}

/// Fuel an aircraft needs to fly `distance_m` at cruise and land with the reserve, t.
#[must_use]
pub fn fuel_needed_t(f: &Fuel, distance_m: f64) -> f64 {
    f.capacity_t * (distance_m / f.endurance_m + RESERVE_FRACTION)
}

/// Aviation fuel an airbase has left, t (`None`: unlimited).
fn stock_left(air: &Air, f: &Facility) -> Option<f64> {
    let start = f.air.as_ref()?.aviation_fuel_t?;
    Some((start - air.fuel_used.get(&f.id).copied().unwrap_or(0.0)).max(0.0))
}

/// Takes the next free runway slot of `base` now, if any.
fn take_runway(air: &mut Air, base: &Facility, now: Tick) -> bool {
    let Some(a) = &base.air else {
        return false;
    };
    let slots = air
        .runways
        .entry(base.id)
        .or_insert_with(|| vec![Tick(0); a.runways as usize]);
    slots.resize(a.runways as usize, Tick(0));
    let Some(slot) = slots.iter_mut().filter(|t| **t <= now).min() else {
        return false;
    };
    *slot = Tick(now.0 + secs(a.interval()));
    true
}

/// Orders a parked aircraft to launch on `mission`, with `loadout` (or the mission's).
pub(crate) fn order_launch(
    world: &mut World,
    issuer_side: Option<SideId>,
    unit: UnitId,
    mission: &Mission,
    loadout: Option<&str>,
) -> Result<(), String> {
    mission.validate()?;
    let tick = world.resource::<SimClock>().tick;
    let air = world.resource::<Air>();
    let craft = air
        .aircraft
        .get(&unit)
        .ok_or_else(|| format!("{unit} is not an aircraft under air operations"))?;
    let record = air
        .parked
        .get(&unit)
        .ok_or_else(|| format!("{unit} is not parked at a base"))?;
    authorise(issuer_side, record.unit.side, unit)?;
    if record
        .health
        .as_ref()
        .is_some_and(|h| h.status != sim_weapons::Status::Operational)
    {
        return Err(format!("{unit} is damaged and cannot fly"));
    }
    let busy_until = match craft.state {
        AirState::Parked { busy_until } => busy_until,
        _ => return Err(format!("{unit} is already tasked")),
    };
    if craft.readiness == Readiness::Maintenance {
        return Err(format!("{unit} is in maintenance"));
    }
    let chosen = match loadout {
        Some(id) => Some(
            craft
                .spec
                .option(id)
                .ok_or_else(|| format!("{unit} has no loadout {id:?}"))?
                .clone(),
        ),
        None => craft.spec.loadout_for(mission).cloned().or_else(|| {
            craft
                .loadout
                .as_deref()
                .and_then(|l| craft.spec.option(l))
                .cloned()
        }),
    };
    let rearm = match &chosen {
        Some(o) if craft.loadout.as_deref() != Some(o.id.as_str()) => {
            REARM_PER_WEAPON_S * u64::from(o.weapons())
        }
        _ => 0,
    };
    let ready_at = Tick((tick.0 + craft.readiness.delay_s()).max(busy_until.0) + rearm);
    let chosen_id = chosen.map(|o| o.id);
    if let Some(c) = world.resource_mut::<Air>().aircraft.get_mut(&unit) {
        c.state = AirState::Launching {
            ready_at,
            mission: mission.clone(),
            loadout: chosen_id.clone(),
        };
    }
    #[allow(clippy::cast_precision_loss)]
    world.resource_mut::<EventLog>().push(
        tick,
        SimEventKind::SortieOrdered {
            unit,
            ready_in_s: ready_at.0.saturating_sub(tick.0) as f64,
            loadout: chosen_id,
        },
    );
    Ok(())
}

fn authorise(issuer_side: Option<SideId>, side: SideId, unit: UnitId) -> Result<(), String> {
    match issuer_side {
        Some(s) if s != side => Err(format!("{unit} does not belong to the issuing side")),
        _ => Ok(()),
    }
}

/// Recalls an aircraft: a launch not yet made is cancelled; one in the air returns to `base`
/// (or its own, or the nearest that takes it).
pub(crate) fn order_recall(
    world: &mut World,
    issuer_side: Option<SideId>,
    unit: UnitId,
    base: Option<u32>,
) -> Result<(), String> {
    let tick = world.resource::<SimClock>().tick;
    let craft = world
        .resource::<Air>()
        .aircraft
        .get(&unit)
        .cloned()
        .ok_or_else(|| format!("{unit} is not an aircraft under air operations"))?;
    if let AirState::Launching { .. } = craft.state {
        let side = world
            .resource::<Air>()
            .parked
            .get(&unit)
            .map(|r| r.unit.side)
            .ok_or_else(|| format!("{unit} is not parked"))?;
        authorise(issuer_side, side, unit)?;
        if let Some(c) = world.resource_mut::<Air>().aircraft.get_mut(&unit) {
            c.state = AirState::Parked { busy_until: tick };
        }
        return Ok(());
    }
    let e = *world
        .resource::<UnitIndex>()
        .0
        .get(&unit)
        .ok_or_else(|| format!("{unit} is not in the air"))?;
    let (side, at) = match (world.get::<Unit>(e), world.get::<Position>(e)) {
        (Some(u), Some(p)) => (u.side, p.point),
        _ => return Err(format!("{unit} is not in the air")),
    };
    authorise(issuer_side, side, unit)?;
    let to = match base {
        Some(b) => {
            let f = airbase(world, b).ok_or_else(|| format!("no airbase {b}"))?;
            if f.side != side || !f.air.as_ref().is_some_and(|a| a.accepts(&craft.spec)) {
                return Err(format!("{unit} cannot use airbase {b}"));
            }
            b
        }
        None => {
            let air = world.resource::<Air>();
            recovery_base(world, air, side, &craft, at)
                .map(|r| r.0)
                .ok_or_else(|| format!("{unit} has no airbase to return to"))?
        }
    };
    if let Some(c) = world.resource_mut::<Air>().aircraft.get_mut(&unit) {
        c.base = Some(to);
    }
    send_home(world, unit, e, to, "recalled", tick);
    Ok(())
}

/// Sets a parked aircraft's readiness (and a maintenance period).
pub(crate) fn order_readiness(
    world: &mut World,
    issuer_side: Option<SideId>,
    unit: UnitId,
    readiness: Readiness,
    maintenance_s: Option<f64>,
) -> Result<(), String> {
    if maintenance_s.is_some_and(|s| !(s.is_finite() && s > 0.0)) {
        return Err("maintenance must last > 0 s".into());
    }
    let tick = world.resource::<SimClock>().tick;
    let side = {
        let air = world.resource::<Air>();
        if !air.aircraft.contains_key(&unit) {
            return Err(format!("{unit} is not an aircraft under air operations"));
        }
        match air.parked.get(&unit) {
            Some(r) => r.unit.side,
            None => {
                let e = world.resource::<UnitIndex>().0.get(&unit).copied();
                e.and_then(|e| world.get::<Unit>(e).map(|u| u.side))
                    .ok_or_else(|| format!("unknown unit {unit}"))?
            }
        }
    };
    authorise(issuer_side, side, unit)?;
    if let Some(c) = world.resource_mut::<Air>().aircraft.get_mut(&unit) {
        c.readiness = readiness;
        c.maintenance_until = match (readiness, maintenance_s) {
            (Readiness::Maintenance, Some(s)) => Some(Tick(tick.0 + secs(s))),
            _ => None,
        };
        if readiness == Readiness::Maintenance {
            if let AirState::Launching { .. } = c.state {
                c.state = AirState::Parked { busy_until: tick };
            }
        }
    }
    world
        .resource_mut::<EventLog>()
        .push(tick, SimEventKind::ReadinessChanged { unit, readiness });
    Ok(())
}

/// Sends an airborne aircraft back to `base`; its mission ends.
fn send_home(world: &mut World, unit: UnitId, e: Entity, base: u32, reason: &str, tick: Tick) {
    if let Some(mut m) = world.get_mut::<MissionState>(e) {
        *m = MissionState::default();
    }
    if let Some(c) = world.resource_mut::<Air>().aircraft.get_mut(&unit) {
        c.state = AirState::Returning {
            base,
            reason: reason.into(),
        };
        c.suspended = None;
    }
    steer_home(world, unit, e, base, tick);
    world.resource_mut::<EventLog>().push(
        tick,
        SimEventKind::AircraftReturning {
            unit,
            base,
            reason: reason.into(),
        },
    );
}

fn steer_home(world: &mut World, unit: UnitId, e: Entity, base: u32, tick: Tick) {
    let Some(p) = airbase(world, base).and_then(|f| base_point(world, &f)) else {
        return;
    };
    crate::ai::set_route(world, e, unit, p, tick);
    let cruise = world
        .resource::<Air>()
        .aircraft
        .get(&unit)
        .map(|c| c.spec.cruise_mps);
    if let (Some(mut k), Some(v)) = (world.get_mut::<Kinematics>(e), cruise) {
        k.ordered_speed_mps = v.min(k.max_speed_mps);
    }
}

/// The air pass, every tick after orders are applied (`docs/models/air-operations.md`).
pub(crate) fn air_system(world: &mut World) {
    if world.resource::<Air>().is_empty() {
        return;
    }
    let tick = world.resource::<SimClock>().tick;
    park_starters(world, tick);
    lose_with_base(world, tick);
    end_maintenance(world, tick);
    recover(world, tick);
    launch(world, tick);
    fly(world, tick);
    tank(world);
    // Aircraft gone (destroyed, removed) leave air operations.
    let gone: Vec<UnitId> = {
        let air = world.resource::<Air>();
        let index = &world.resource::<UnitIndex>().0;
        air.aircraft
            .iter()
            .filter(|(id, a)| {
                !index.contains_key(id)
                    && !air.parked.contains_key(id)
                    && !matches!(a.state, AirState::Starting)
            })
            .map(|(id, _)| *id)
            .collect()
    };
    let mut air = world.resource_mut::<Air>();
    for id in gone {
        air.aircraft.remove(&id);
    }
}

/// Puts the aircraft spawned parked into their hangars.
fn park_starters(world: &mut World, tick: Tick) {
    let starting: Vec<UnitId> = world
        .resource::<Air>()
        .aircraft
        .iter()
        .filter(|(_, a)| a.state == AirState::Starting)
        .map(|(id, _)| *id)
        .collect();
    for id in starting {
        let base = world
            .resource::<Air>()
            .aircraft
            .get(&id)
            .and_then(|a| a.base);
        let point = base
            .and_then(|b| airbase(world, b))
            .and_then(|f| base_point(world, &f));
        let Some(e) = world.resource::<UnitIndex>().0.get(&id).copied() else {
            continue;
        };
        match point {
            Some(p) => {
                if let Some(mut r) = crate::sim::take_record(world, e) {
                    ground(&mut r, p);
                    let busy = world
                        .resource::<Air>()
                        .aircraft
                        .get(&id)
                        .and_then(|a| a.maintenance_until)
                        .unwrap_or(tick);
                    let mut air = world.resource_mut::<Air>();
                    air.parked.insert(id, r);
                    if let Some(a) = air.aircraft.get_mut(&id) {
                        a.state = AirState::Parked { busy_until: busy };
                    }
                }
            }
            // No such airbase (yet): it starts airborne instead.
            None => {
                if let Some(a) = world.resource_mut::<Air>().aircraft.get_mut(&id) {
                    a.state = AirState::Airborne;
                }
            }
        }
    }
}

/// A record as it sits in a hangar: at the base, stopped, on the ground, with no route.
fn ground(r: &mut UnitRecord, at: GeoPoint) {
    r.position.point = at;
    r.position.altitude_m = 0.0;
    r.kinematics.speed_mps = 0.0;
    r.kinematics.ordered_speed_mps = 0.0;
    r.route = Route::default();
    r.mission = MissionState::default();
}

/// Aircraft parked on a carrier that is lost are lost with it.
fn lose_with_base(world: &mut World, tick: Tick) {
    let lost: Vec<UnitId> = {
        let air = world.resource::<Air>();
        air.parked
            .keys()
            .filter(|id| {
                air.aircraft
                    .get(id)
                    .and_then(|a| a.base)
                    .and_then(|b| world.resource::<PlanningLayer>().facility(b))
                    .is_some_and(|f| base_point(world, f).is_none())
            })
            .copied()
            .collect()
    };
    for id in lost {
        let mut air = world.resource_mut::<Air>();
        air.parked.remove(&id);
        air.aircraft.remove(&id);
        let mut log = world.resource_mut::<EventLog>();
        log.push(
            tick,
            SimEventKind::AircraftLost {
                unit: id,
                reason: "lost with its base".into(),
            },
        );
        log.push(tick, SimEventKind::UnitDestroyed { unit: id });
    }
}

fn end_maintenance(world: &mut World, tick: Tick) {
    let done: Vec<UnitId> = world
        .resource::<Air>()
        .aircraft
        .iter()
        .filter(|(_, a)| a.maintenance_until.is_some_and(|t| t <= tick))
        .map(|(id, _)| *id)
        .collect();
    for id in done {
        if let Some(a) = world.resource_mut::<Air>().aircraft.get_mut(&id) {
            a.maintenance_until = None;
            a.readiness = Readiness::Ready;
        }
        world.resource_mut::<EventLog>().push(
            tick,
            SimEventKind::ReadinessChanged {
                unit: id,
                readiness: Readiness::Ready,
            },
        );
    }
}

/// Lands returning aircraft over their base, a runway slot each; a base with no room sends them
/// on to another.
fn recover(world: &mut World, tick: Tick) {
    let returning: Vec<(UnitId, u32)> = world
        .resource::<Air>()
        .aircraft
        .iter()
        .filter_map(|(id, a)| match a.state {
            AirState::Returning { base, .. } => Some((*id, base)),
            _ => None,
        })
        .collect();
    for (id, base) in returning {
        let Some(e) = world.resource::<UnitIndex>().0.get(&id).copied() else {
            continue;
        };
        let (Some(at), Some(side)) = (
            world.get::<Position>(e).map(|p| p.point),
            world.get::<Unit>(e).map(|u| u.side),
        ) else {
            continue;
        };
        let f = airbase(world, base);
        let point = f.as_ref().and_then(|f| base_point(world, f));
        let craft = world.resource::<Air>().aircraft.get(&id).cloned();
        let Some(craft) = craft else { continue };
        let room = f.as_ref().is_some_and(|f| {
            f.air.as_ref().is_some_and(|a| {
                a.accepts(&craft.spec)
                    && (a.parking == 0 || on_base(world.resource::<Air>(), f.id) < a.parking)
            })
        });
        let (Some(f), Some(point), true) = (f, point, room) else {
            // Base lost or full: on to another, if there is one.
            let next = recovery_base(world, world.resource::<Air>(), side, &craft, at)
                .filter(|r| r.0 != base);
            if let Some((b, _, _)) = next {
                if let Some(c) = world.resource_mut::<Air>().aircraft.get_mut(&id) {
                    c.base = Some(b);
                }
                send_home(world, id, e, b, "its base cannot take it", tick);
            }
            continue;
        };
        if tick.0.is_multiple_of(AI_PERIOD_TICKS) {
            steer_home(world, id, e, base, tick);
        }
        if inverse(at, point).distance_m > RECOVERY_RADIUS_M
            || !take_runway(&mut world.resource_mut::<Air>(), &f, tick)
        {
            continue;
        }
        land(world, id, e, &f, point, tick);
    }
}

fn land(world: &mut World, id: UnitId, e: Entity, f: &Facility, at: GeoPoint, tick: Tick) {
    let Some(mut r) = crate::sim::take_record(world, e) else {
        return;
    };
    ground(&mut r, at);
    let mut air = world.resource_mut::<Air>();
    let Some(craft) = air.aircraft.get(&id).cloned() else {
        return;
    };
    let option = craft
        .loadout
        .as_deref()
        .and_then(|l| craft.spec.option(l))
        .cloned();
    // Refuel from the base's stock.
    let mut taken = 0.0;
    if let Some(fuel) = r.fuel.as_mut() {
        let want = (fuel.capacity_t - fuel.fuel_t).max(0.0);
        taken = stock_left(&air, f).map_or(want, |left| want.min(left));
        fuel.fuel_t += taken;
        *air.fuel_used.entry(f.id).or_insert(0.0) += taken;
    }
    // Rearm with the loadout carried.
    if let Some(o) = &option {
        r.magazine = Magazine(o.stores.clone());
        if let Some(l) = r.loadout.as_mut() {
            l.stores.clone_from(&o.stores);
        }
    }
    let turnaround = craft.spec.turnaround_s(option.as_ref());
    air.parked.insert(id, r);
    if let Some(a) = air.aircraft.get_mut(&id) {
        a.base = Some(f.id);
        a.state = AirState::Parked {
            busy_until: Tick(tick.0 + turnaround),
        };
        a.suspended = None;
    }
    #[allow(clippy::cast_precision_loss)]
    world.resource_mut::<EventLog>().push(
        tick,
        SimEventKind::AircraftRecovered {
            unit: id,
            base: f.id,
            fuel_t: taken,
            turnaround_s: turnaround as f64,
        },
    );
}

/// Launches tasked aircraft whose time has come, a runway slot each, in (time, unit) order.
fn launch(world: &mut World, tick: Tick) {
    let mut due: Vec<(Tick, UnitId)> = world
        .resource::<Air>()
        .aircraft
        .iter()
        .filter_map(|(id, a)| match a.state {
            AirState::Launching { ready_at, .. } if ready_at <= tick => Some((ready_at, *id)),
            _ => None,
        })
        .collect();
    due.sort();
    for (_, id) in due {
        let Some(craft) = world.resource::<Air>().aircraft.get(&id).cloned() else {
            continue;
        };
        let AirState::Launching {
            mission, loadout, ..
        } = craft.state.clone()
        else {
            continue;
        };
        let Some(f) = craft.base.and_then(|b| airbase(world, b)) else {
            continue;
        };
        let Some(at) = base_point(world, &f) else {
            continue;
        };
        if !take_runway(&mut world.resource_mut::<Air>(), &f, tick) {
            continue;
        }
        let Some(mut r) = world.resource_mut::<Air>().parked.remove(&id) else {
            continue;
        };
        let option = loadout
            .as_deref()
            .and_then(|l| craft.spec.option(l))
            .cloned();
        if let Some(o) = &option {
            r.magazine = Magazine(o.stores.clone());
            match r.loadout.as_mut() {
                Some(l) => l.stores.clone_from(&o.stores),
                None => {
                    r.loadout = Some(Loadout {
                        sensors: r.sensors.0.clone(),
                        max_speed_mps: r.kinematics.max_speed_mps,
                        stores: o.stores.clone(),
                    });
                }
            }
        }
        r.position.point = at;
        r.position.altitude_m = craft.spec.cruise_altitude_m;
        r.kinematics.speed_mps = craft.spec.cruise_mps.min(r.kinematics.max_speed_mps);
        r.kinematics.ordered_speed_mps = r.kinematics.speed_mps;
        r.route = Route::default();
        r.mission = MissionState {
            mission: Some(mission.clone()),
            ..MissionState::default()
        };
        if let Some(fuel) = r.fuel.as_mut() {
            let climb = TAKEOFF_FUEL_FRACTION * if craft.spec.vtol { 0.5 } else { 1.0 };
            fuel.fuel_t = (fuel.fuel_t - climb * fuel.capacity_t).max(0.0);
        }
        crate::sim::spawn_record(world, r);
        let armed = option.as_ref().is_some_and(|o| o.weapons() > 0);
        if let Some(a) = world.resource_mut::<Air>().aircraft.get_mut(&id) {
            a.state = AirState::Airborne;
            a.loadout = loadout.clone();
            a.armed = armed;
        }
        world.resource_mut::<EventLog>().push(
            tick,
            SimEventKind::SortieLaunched {
                unit: id,
                base: f.id,
                loadout,
                mission: mission_name(&mission).into(),
            },
        );
    }
}

/// A mission's name for the log.
#[must_use]
pub fn mission_name(m: &Mission) -> &'static str {
    match m {
        Mission::Patrol { .. } => "patrol",
        Mission::Ferry { .. } => "ferry",
        Mission::Escort { .. } => "escort",
        Mission::Strike { .. } => "strike",
        Mission::Support { .. } => "support",
        Mission::Mining { .. } => "mining",
        Mission::MineClearing { .. } => "mine clearing",
        Mission::Cargo { .. } => "cargo",
        Mission::BarrierPatrol { .. } => "barrier patrol",
        Mission::Denial { .. } => "denial",
    }
}

/// Airborne aircraft: lost when out of fuel; home when the sortie is over or at bingo (or to a
/// tanker nearer than a base); steering for those on their way.
#[allow(clippy::too_many_lines)]
fn fly(world: &mut World, tick: Tick) {
    let flying: Vec<UnitId> = world
        .resource::<Air>()
        .aircraft
        .iter()
        .filter(|(_, a)| {
            matches!(
                a.state,
                AirState::Airborne | AirState::ToTanker { .. } | AirState::Tanking { .. }
            )
        })
        .map(|(id, _)| *id)
        .collect();
    for id in flying {
        let Some(e) = world.resource::<UnitIndex>().0.get(&id).copied() else {
            continue;
        };
        let (Some(at), Some(side)) = (
            world.get::<Position>(e).map(|p| p.point),
            world.get::<Unit>(e).map(|u| u.side),
        ) else {
            continue;
        };
        let fuel = world.get::<Fuel>(e).copied();
        if fuel.is_some_and(|f| f.empty()) {
            crash(world, id, e, "out of fuel", tick);
            continue;
        }
        let Some(craft) = world.resource::<Air>().aircraft.get(&id).cloned() else {
            continue;
        };
        let home = recovery_base(world, world.resource::<Air>(), side, &craft, at);
        // Bingo: fuel to reach the base (or nowhere) and land with the reserve.
        if let (Some(f), AirState::Airborne) = (fuel, &craft.state) {
            let to_base = home.map(|h| h.2);
            let need = fuel_needed_t(&f, to_base.unwrap_or(0.0));
            if f.fuel_t <= need {
                let tanker = if craft.spec.tanker.is_none() {
                    nearest_tanker(world, id, side, at, f.capacity_t - f.fuel_t)
                } else {
                    None
                };
                match (tanker, home) {
                    (Some((t, d)), h) if h.is_none_or(|h| d < h.2) => {
                        let mission = world
                            .get_mut::<MissionState>(e)
                            .and_then(|mut m| m.mission.take());
                        if let Some(c) = world.resource_mut::<Air>().aircraft.get_mut(&id) {
                            c.state = AirState::ToTanker { tanker: t };
                            c.suspended = mission;
                        }
                        world.resource_mut::<EventLog>().push(
                            tick,
                            SimEventKind::BingoFuel {
                                unit: id,
                                fuel_t: f.fuel_t,
                                tanker: Some(t),
                            },
                        );
                        steer_to(world, id, e, t, tick);
                    }
                    (_, Some((b, _, _))) => {
                        world.resource_mut::<EventLog>().push(
                            tick,
                            SimEventKind::BingoFuel {
                                unit: id,
                                fuel_t: f.fuel_t,
                                tanker: None,
                            },
                        );
                        send_home(world, id, e, b, "bingo fuel", tick);
                    }
                    _ => {}
                }
                continue;
            }
        }
        if !tick.0.is_multiple_of(AI_PERIOD_TICKS) {
            continue;
        }
        match craft.state {
            AirState::Airborne => {
                let Some((b, _, _)) = home else { continue };
                if let Some(reason) = sortie_over(world, e, side, &craft) {
                    send_home(world, id, e, b, reason, tick);
                }
            }
            AirState::ToTanker { tanker } => {
                let alive = world.resource::<UnitIndex>().0.contains_key(&tanker)
                    && world
                        .resource::<Air>()
                        .aircraft
                        .get(&tanker)
                        .is_some_and(|t| t.state == AirState::Airborne);
                if alive {
                    steer_to(world, id, e, tanker, tick);
                } else {
                    // The tanker left: fall back to bingo on the next pass.
                    resume(world, id, e);
                }
            }
            _ => {}
        }
    }
}

/// Why a sortie is over, if it is: mission complete, target gone, weapons spent.
fn sortie_over(world: &World, e: Entity, side: SideId, craft: &Aircraft) -> Option<&'static str> {
    let m = world.get::<MissionState>(e)?;
    if m.complete {
        return Some("mission complete");
    }
    if let Some(Mission::Strike { contact }) = &m.mission {
        let known = world
            .resource::<ContactTable>()
            .iter()
            .any(|(s, c)| s == side && c.number == *contact);
        if !known {
            return Some("target gone");
        }
    }
    let engages = m.mission.as_ref().is_some_and(Mission::engages);
    let empty = world
        .get::<Magazine>(e)
        .is_none_or(|g| g.0.values().all(|n| *n == 0));
    (craft.armed && engages && empty).then_some("weapons expended")
}

/// The nearest tanker of `side` on station with fuel to give `need_t`: (tanker, distance m).
fn nearest_tanker(
    world: &World,
    me: UnitId,
    side: SideId,
    at: GeoPoint,
    need_t: f64,
) -> Option<(UnitId, f64)> {
    let air = world.resource::<Air>();
    air.aircraft
        .iter()
        .filter(|(id, a)| **id != me && a.spec.tanker.is_some() && a.state == AirState::Airborne)
        .filter_map(|(id, _)| {
            let e = *world.resource::<UnitIndex>().0.get(id)?;
            let u = world.get::<Unit>(e)?;
            if u.side != side {
                return None;
            }
            let p = world.get::<Position>(e)?.point;
            (offload_t(world, *id, e) >= 0.5 * need_t).then_some((*id, inverse(at, p).distance_m))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
}

/// Fuel a tanker can give: what it has over its own bingo and a margin, t.
fn offload_t(world: &World, tanker: UnitId, e: Entity) -> f64 {
    let (Some(f), Some(p), Some(u)) = (
        world.get::<Fuel>(e),
        world.get::<Position>(e),
        world.get::<Unit>(e),
    ) else {
        return 0.0;
    };
    let air = world.resource::<Air>();
    let Some(craft) = air.aircraft.get(&tanker) else {
        return 0.0;
    };
    let home = recovery_base(world, air, u.side, craft, p.point).map_or(0.0, |h| h.2);
    (f.fuel_t - fuel_needed_t(f, home) - TANKER_MARGIN_FRACTION * f.capacity_t).max(0.0)
}

fn steer_to(world: &mut World, id: UnitId, e: Entity, tanker: UnitId, tick: Tick) {
    let Some(te) = world.resource::<UnitIndex>().0.get(&tanker).copied() else {
        return;
    };
    let (Some(p), Some(k)) = (
        world.get::<Position>(te).map(|p| p.point),
        world.get::<Kinematics>(te).copied(),
    ) else {
        return;
    };
    crate::ai::set_route(world, e, id, p, tick);
    if let Some(mut mine) = world.get_mut::<Kinematics>(e) {
        mine.ordered_speed_mps = (k.speed_mps + 50.0).min(mine.max_speed_mps);
    }
}

/// Back on the sortie after tanking (or when the tanker left).
fn resume(world: &mut World, id: UnitId, e: Entity) {
    let mission = world
        .resource_mut::<Air>()
        .aircraft
        .get_mut(&id)
        .and_then(|c| {
            c.state = AirState::Airborne;
            c.suspended.take()
        });
    if let Some(mut m) = world.get_mut::<MissionState>(e) {
        if m.mission.is_none() {
            m.mission = mission;
        }
    }
}

fn crash(world: &mut World, id: UnitId, e: Entity, reason: &str, tick: Tick) {
    world.resource_mut::<EventLog>().push(
        tick,
        SimEventKind::AircraftLost {
            unit: id,
            reason: reason.into(),
        },
    );
    crate::combat::destroy(world, id, e, tick);
    world.resource_mut::<Air>().aircraft.remove(&id);
}

/// Air-to-air refuelling: receivers within reach of their tanker take a hose when one is free
/// (unit-id order), and take fuel at the hose's rate until full or the tanker has no more to give.
fn tank(world: &mut World) {
    let tick = world.resource::<SimClock>().tick;
    let pairs: Vec<(UnitId, UnitId, bool)> = world
        .resource::<Air>()
        .aircraft
        .iter()
        .filter_map(|(id, a)| match a.state {
            AirState::ToTanker { tanker } => Some((*id, tanker, false)),
            AirState::Tanking { tanker, .. } => Some((*id, tanker, true)),
            _ => None,
        })
        .collect();
    for (id, tanker, joined) in pairs {
        let index = &world.resource::<UnitIndex>().0;
        let (Some(e), Some(te)) = (index.get(&id).copied(), index.get(&tanker).copied()) else {
            continue;
        };
        let (Some(p), Some(tp)) = (
            world.get::<Position>(e).map(|p| p.point),
            world.get::<Position>(te).map(|p| p.point),
        ) else {
            continue;
        };
        let d = inverse(p, tp).distance_m;
        let spec = world
            .resource::<Air>()
            .aircraft
            .get(&tanker)
            .and_then(|t| t.spec.tanker);
        let Some(spec) = spec else {
            resume(world, id, e);
            continue;
        };
        if !joined {
            let busy = world
                .resource::<Air>()
                .aircraft
                .values()
                .filter(|a| matches!(a.state, AirState::Tanking { tanker: t, .. } if t == tanker))
                .count();
            if d <= TANKER_CONTACT_M && busy < spec.hoses as usize {
                if let Some(c) = world.resource_mut::<Air>().aircraft.get_mut(&id) {
                    c.state = AirState::Tanking {
                        tanker,
                        taken_t: 0.0,
                    };
                }
                world.resource_mut::<EventLog>().push(
                    tick,
                    SimEventKind::TankingStarted {
                        receiver: id,
                        tanker,
                    },
                );
            }
            continue;
        }
        // In contact: keep station on the tanker and take fuel.
        let tk = world.get::<Kinematics>(te).copied();
        crate::ai::set_route(world, e, id, tp, tick);
        if let (Some(mut k), Some(tk)) = (world.get_mut::<Kinematics>(e), tk) {
            k.ordered_speed_mps = tk.speed_mps.min(k.max_speed_mps);
        }
        if d > 5.0 * TANKER_CONTACT_M {
            if let Some(c) = world.resource_mut::<Air>().aircraft.get_mut(&id) {
                c.state = AirState::ToTanker { tanker };
            }
            continue;
        }
        let room = world
            .get::<Fuel>(e)
            .map_or(0.0, |f| (f.capacity_t * FULL_FRACTION - f.fuel_t).max(0.0));
        let give = offload_t(world, tanker, te);
        let flow = (spec.transfer_t_per_min / 60.0 * TICK_SECONDS)
            .min(room)
            .min(give);
        if flow > 0.0 {
            if let Some(AirState::Tanking { taken_t, .. }) = world
                .resource_mut::<Air>()
                .aircraft
                .get_mut(&id)
                .map(|c| &mut c.state)
            {
                *taken_t += flow;
            }
            if let Some(mut f) = world.get_mut::<Fuel>(e) {
                f.fuel_t += flow;
            }
            if let Some(mut f) = world.get_mut::<Fuel>(te) {
                f.fuel_t -= flow;
            }
        }
        if room - flow <= 1e-9 || give - flow <= 1e-9 {
            let taken_t = match world.resource::<Air>().aircraft.get(&id).map(|c| &c.state) {
                Some(AirState::Tanking { taken_t, .. }) => *taken_t,
                _ => 0.0,
            };
            let fuel_t = world.get::<Fuel>(e).map_or(0.0, |f| f.fuel_t);
            resume(world, id, e);
            world.resource_mut::<EventLog>().push(
                tick,
                SimEventKind::TankingComplete {
                    receiver: id,
                    tanker,
                    fuel_t,
                    taken_t,
                },
            );
        }
    }
}

/// A parked aircraft, as a seat sees it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ParkedView {
    pub unit: UnitId,
    pub name: String,
    pub platform: Option<String>,
    pub readiness: Readiness,
    /// Tasked and waiting to launch.
    pub launching: bool,
    /// Seconds until it can launch (turnaround, maintenance, alert delay once tasked).
    pub ready_in_s: f64,
    pub loadout: Option<String>,
    pub loadouts: Vec<LoadoutOption>,
    pub fuel_fraction: Option<f64>,
    /// Damaged: it cannot fly.
    pub damaged: bool,
}

/// An airbase and its aircraft on the ground, as a seat sees it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct AirBaseView {
    pub facility: u32,
    pub name: String,
    pub side: SideId,
    pub position: Option<GeoPoint>,
    pub runways: u32,
    pub parking: u32,
    pub deck: bool,
    /// Aviation fuel left, t (`None`: unlimited).
    pub aviation_fuel_t: Option<f64>,
    pub aircraft: Vec<ParkedView>,
}

/// An aircraft in the air, as a seat sees it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct AirUnitView {
    pub base: Option<u32>,
    /// `airborne`, `returning`, `to_tanker` or `tanking`.
    pub state: String,
    pub loadout: Option<String>,
    /// Fuel share at which it turns for home from here (`None`: no fuel model).
    pub bingo_fraction: Option<f64>,
    pub tanker: bool,
}

/// The airbases `side` sees (all for the white cell), with their parked aircraft.
pub(crate) fn base_views(world: &World, side: Option<SideId>, tick: Tick) -> Vec<AirBaseView> {
    let air = world.resource::<Air>();
    world
        .resource::<PlanningLayer>()
        .facilities
        .iter()
        .filter(|f| f.kind == FacilityKind::Airbase && side.is_none_or(|s| s == f.side))
        .filter_map(|f| {
            let a = f.air.as_ref()?;
            let aircraft = air
                .parked
                .iter()
                .filter_map(|(id, r)| {
                    let c = air.aircraft.get(id)?;
                    (c.base == Some(f.id)).then_some(())?;
                    let (launching, until) = match &c.state {
                        AirState::Parked { busy_until } => (
                            false,
                            c.maintenance_until.unwrap_or(*busy_until).max(*busy_until),
                        ),
                        AirState::Launching { ready_at, .. } => (true, *ready_at),
                        _ => (false, tick),
                    };
                    #[allow(clippy::cast_precision_loss)]
                    Some(ParkedView {
                        unit: *id,
                        name: r.unit.name.clone(),
                        platform: r.unit.platform.clone(),
                        readiness: c.readiness,
                        launching,
                        ready_in_s: until.0.saturating_sub(tick.0) as f64,
                        loadout: match &c.state {
                            AirState::Launching { loadout, .. } => loadout.clone(),
                            _ => c.loadout.clone(),
                        },
                        loadouts: c.spec.loadouts.clone(),
                        fuel_fraction: r.fuel.map(|f| f.fraction()),
                        damaged: r
                            .health
                            .as_ref()
                            .is_some_and(|h| h.status != sim_weapons::Status::Operational),
                    })
                })
                .collect();
            Some(AirBaseView {
                facility: f.id,
                name: f.name.clone(),
                side: f.side,
                position: base_point(world, f),
                runways: a.runways,
                parking: a.parking,
                deck: a.deck,
                aviation_fuel_t: stock_left(air, f),
                aircraft,
            })
        })
        .collect()
}

/// The air state of an aircraft in the air.
pub(crate) fn unit_view(world: &World, unit: UnitId) -> Option<AirUnitView> {
    let air = world.resource::<Air>();
    let c = air.aircraft.get(&unit)?;
    let state = match c.state {
        AirState::Airborne => "airborne",
        AirState::Returning { .. } => "returning",
        AirState::ToTanker { .. } => "to_tanker",
        AirState::Tanking { .. } => "tanking",
        _ => return None,
    };
    let e = *world.resource::<UnitIndex>().0.get(&unit)?;
    let bingo_fraction = match (
        world.get::<Fuel>(e),
        world.get::<Position>(e),
        world.get::<Unit>(e),
    ) {
        (Some(f), Some(p), Some(u)) => {
            let d = recovery_base(world, air, u.side, c, p.point).map_or(0.0, |h| h.2);
            Some((fuel_needed_t(f, d) / f.capacity_t).min(1.0))
        }
        _ => None,
    };
    Some(AirUnitView {
        base: c.base,
        state: state.into(),
        loadout: c.loadout.clone(),
        bingo_fraction,
        tanker: c.spec.tanker.is_some(),
    })
}

/// Readiness in words, for the log and the AAR.
#[must_use]
pub fn readiness_text(r: Readiness) -> &'static str {
    r.code()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> AirSpec {
        AirSpec {
            base: Some(1),
            parked: true,
            readiness: Readiness::Ready,
            maintenance_s: None,
            loadouts: vec![
                LoadoutOption {
                    id: "F/CAP".into(),
                    name: "CAP".into(),
                    role: "air_superiority".into(),
                    stores: [("AAM".to_owned(), 6)].into(),
                },
                LoadoutOption {
                    id: "F/ASUW".into(),
                    name: "Anti-ship".into(),
                    role: "asuw".into(),
                    stores: [("ASM".to_owned(), 2), ("AAM".to_owned(), 2)].into(),
                },
            ],
            loadout: Some("F/CAP".into()),
            tanker: None,
            cruise_altitude_m: 8_000.0,
            cruise_mps: 247.0,
            vtol: false,
            large: false,
        }
    }

    #[test]
    fn readiness_delays() {
        assert_eq!(Readiness::Ready.delay_s(), 0);
        assert_eq!(Readiness::Alert15.delay_s(), 900);
        assert_eq!(Readiness::Alert60.delay_s(), 3_600);
    }

    #[test]
    fn the_mission_picks_the_loadout() {
        let s = spec();
        let strike = Mission::Strike { contact: 1 };
        let patrol = Mission::Patrol {
            centre: GeoPoint::new(0.0, 0.0),
            radius_m: 1.0,
        };
        assert_eq!(s.loadout_for(&strike).unwrap().id, "F/ASUW");
        assert_eq!(s.loadout_for(&patrol).unwrap().id, "F/CAP");
        let ferry = Mission::Ferry {
            destination: GeoPoint::new(0.0, 0.0),
        };
        assert!(
            s.loadout_for(&ferry).is_none(),
            "ferry keeps what it carries"
        );
    }

    #[test]
    fn turnaround_grows_with_the_weapons_loaded() {
        let s = spec();
        // 20 min, plus 2 min a weapon: 6 missiles, 32 min; 4 weapons, 28 min.
        assert_eq!(s.turnaround_s(s.option("F/CAP")), 1_200 + 6 * 120);
        assert_eq!(s.turnaround_s(s.option("F/ASUW")), 1_200 + 4 * 120);
        let mut big = s;
        big.large = true;
        assert_eq!(big.turnaround_s(None), 2_400);
    }

    #[test]
    fn bingo_is_the_fuel_home_plus_the_reserve() {
        // 5 t, 1,500 nm range: 500 nm home is a third of the tank, plus the 10 % reserve.
        let f = Fuel::from_spec(&crate::fuel::FuelSpec {
            capacity_t: 5.0,
            endurance_m: 1_500.0 * 1_852.0,
            cruise_mps: 247.0,
            level: 1.0,
            bingo: None,
            kind: crate::fuel::FuelKind::Aviation,
            idle_fraction: Some(crate::fuel::AIRCRAFT_IDLE_FRACTION),
        });
        let need = fuel_needed_t(&f, 500.0 * 1_852.0);
        assert!((need - 5.0 * (1.0 / 3.0 + 0.1)).abs() < 1e-9, "{need}");
    }

    #[test]
    fn decks_take_helicopters_and_carriers_take_jets() {
        let jet = spec();
        let mut helo = spec();
        helo.vtol = true;
        let deck = AirField {
            runways: 1,
            interval_s: None,
            parking: 2,
            deck: true,
            fixed_wing: None,
            aviation_fuel_t: None,
        };
        assert!(deck.accepts(&helo) && !deck.accepts(&jet));
        let carrier = AirField {
            fixed_wing: Some(true),
            ..deck.clone()
        };
        assert!(carrier.accepts(&jet));
        assert!((deck.interval() - DECK_INTERVAL_S).abs() < 1e-12);
        let field = AirField {
            deck: false,
            ..deck
        };
        assert!(field.accepts(&jet));
        assert!((field.interval() - LAND_INTERVAL_S).abs() < 1e-12);
    }
}
