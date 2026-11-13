//! Scenario files: JSON, versioned, validated on load.
//!
//! A scenario holds the seed, sides, groups, minefields, environment, units and events. A unit
//! names a platform-database class (`"platform": "GEN-FFG"`), which supplies its domain,
//! performance, sensors and radar cross-section. Any of the performance values can be overridden
//! per unit. Events (triggers → conditions → actions) are in `docs/models/events.md`.
//!
//! Format history:
//! * v1: units carried domain, max speed and acceleration inline.
//! * v2: optional `platform` reference; inline values become optional overrides. v1 files still load.
//! * v3: `events`, and `reserve` units held off the board until an event brings them in. v1 and
//!   v2 files still load.
//! * v4: `reference_points` (named map points for planning and briefing). Older files still load.
//! * v5: the planning layer: `facilities`, `zones` and surface `action_groups`
//!   (`docs/models/planning-layer.md`). Older files still load.
//! * v6: logistics: `fuel` (the fuel model for ships and submarines) and unit `fuel_level`
//!   (`docs/models/fuel.md`). Older files still load, without fuel.

#![forbid(unsafe_code)]

pub mod synthetic;

pub use sim_core::{ActionGroup, GroupStation};

use serde::{Deserialize, Serialize};
use sim_core::{
    Carrier, Command, CommandPayload, DamageProfile, Doctrine, DoctrineScope, Domain, EmitterEntry,
    Environment, Facility, FuelSpec, GeoPoint, Guidance, Inject, Issuer, JammerSpec, Mission,
    Posture, ScenarioEvent, Seat, SeatRole, SensorKind, SensorSpec, SideId, Simulation, Tick,
    UnitId, UnitSpawn, WeaponKind, WeaponSpec, Zone, FUEL_FRACTION_OF_DISPLACEMENT,
};
use sim_db::PlatformDb;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub const SCENARIO_FORMAT: &str = "nws-scenario";
pub const SCENARIO_FORMAT_VERSION: u32 = 6;

#[derive(Debug, thiserror::Error)]
pub enum ScenarioError {
    #[error("cannot read scenario file: {0}")]
    Io(#[from] std::io::Error),
    #[error("scenario JSON is malformed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("not a scenario file (format = {0:?})")]
    WrongFormat(String),
    #[error("scenario format version {found} is newer than this build supports ({supported})")]
    FutureVersion { found: u32, supported: u32 },
    #[error("scenario is invalid: {0}")]
    Invalid(String),
    #[error("platform database: {0}")]
    Db(#[from] sim_db::DbError),
    #[error(transparent)]
    Sim(#[from] sim_core::SimError),
}

#[cfg_attr(feature = "specta", derive(specta::Type))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SideDef {
    pub id: SideId,
    pub name: String,
    /// How this side regards the others. Sides not listed are neutral.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub postures: Vec<SidePosture>,
    /// Side-wide doctrine; groups and units inherit unset fields from it.
    #[serde(default, skip_serializing_if = "is_default")]
    pub doctrine: Doctrine,
}

#[cfg_attr(feature = "specta", derive(specta::Type))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SidePosture {
    pub toward: SideId,
    pub posture: Posture,
}

/// A named group (task group, formation) of one side, with its doctrine.
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupDef {
    pub side: SideId,
    pub name: String,
    #[serde(default)]
    pub doctrine: Doctrine,
}

/// A minefield laid before the scenario starts.
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioMinefield {
    pub side: SideId,
    pub centre: GeoPoint,
    pub radius_m: f64,
    pub mines: u32,
    /// Mine type (platform database weapon id).
    pub weapon: String,
}

fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}

/// A unit as written in a scenario file.
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioUnit {
    pub id: UnitId,
    pub name: String,
    pub side: SideId,
    /// Platform database class. When given, it supplies everything below that is left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<Domain>,
    pub position: GeoPoint,
    #[serde(default)]
    pub altitude_m: f64,
    #[serde(default)]
    pub heading_deg: f64,
    /// Initial (and ordered) speed. Defaults to the platform's cruise speed, or 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed_mps: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_speed_mps: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accel_mps2: Option<f64>,
    #[serde(default)]
    pub route: Vec<GeoPoint>,
    /// Group within the side (declared in `groups`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// Doctrine set on this unit; unset fields inherit from its group and side.
    #[serde(default, skip_serializing_if = "is_default")]
    pub doctrine: Doctrine,
    /// A noise jammer (radiates while the unit's EMCON is active). Without one, the unit jams
    /// with its platform's strongest ECM equipment from the database, if it has any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jammer: Option<JammerSpec>,
    /// Mission at start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mission: Option<Mission>,
    /// Held off the board until an event's `spawn_reserve` action brings it in.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reserve: bool,
    /// Fraction of fuel at the start, with the fuel model on (default full; format 6).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fuel_level: Option<f64>,
    /// Aircraft: its airbase facility (Phase 4 slice 9, `docs/models/air-operations.md`). With a
    /// base it starts parked there, unless `airborne`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<u32>,
    /// Aircraft with a base: starts in the air instead of parked.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub airborne: bool,
    /// Aircraft: readiness at the start (default ready).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub readiness: Option<sim_core::Readiness>,
    /// Aircraft in maintenance: for how long, s (none: until changed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maintenance_s: Option<f64>,
    /// Aircraft: the loadout carried at the start (default the platform's first).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loadout: Option<String>,
}

#[cfg_attr(feature = "specta", derive(specta::Type))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scenario {
    pub format: String,
    pub format_version: u32,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Default RNG seed. Batch runs override it per replication.
    // Exported to TypeScript as a number: seeds written by people stay far below 2^53.
    #[cfg_attr(feature = "specta", specta(type = f64))]
    pub seed: u64,
    /// Default run length in game seconds.
    #[cfg_attr(feature = "specta", specta(type = f64))]
    pub duration_s: u64,
    pub sides: Vec<SideDef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<GroupDef>,
    /// Minefields at the start.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub minefields: Vec<ScenarioMinefield>,
    /// Sea state and weather loss for the whole scenario (default: calm, clear).
    #[serde(default, skip_serializing_if = "is_default")]
    pub environment: Environment,
    /// Weather zones over the scenario-wide weather (Phase 4 slice 7, `docs/models/weather.md`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub weather_zones: Vec<sim_core::WeatherZone>,
    /// Sound-velocity profiles by region and season (Phase 4 slice 8, `docs/models/acoustics.md`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sound_profiles: Vec<sim_core::SoundProfile>,
    pub units: Vec<ScenarioUnit>,
    /// Scenario events, checked every tick (`docs/models/events.md`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<ScenarioEvent>,
    /// The inject library: prepared messages sent by events or by the white cell.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub injects: Vec<Inject>,
    /// Player positions. Empty: one white-cell seat and one seat per side
    /// (`Scenario::seats_or_default`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub seats: Vec<Seat>,
    /// Named map points for planning and briefing (format 4).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reference_points: Vec<ReferencePoint>,
    /// Planning layer: command posts, refuelling points, sheltering ports, anchorages (format 5).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub facilities: Vec<Facility>,
    /// Planning layer: sense and engage zones, e.g. at choke points (format 5).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub zones: Vec<Zone>,
    /// Surface action groups: members keep station on the leader (format 5).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub action_groups: Vec<ActionGroup>,
    /// Planning layer: shipping lanes, e.g. through choke points (Phase 4 slice 5).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lanes: Vec<sim_core::Lane>,
    /// The fuel model for ships and submarines (format 6; `docs/models/fuel.md`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub fuel: bool,
}

/// A named point on the map (a rendezvous, a waypoint name, a briefing mark). It has no effect
/// on the simulation; the desktop shows it to the sides that may see it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct ReferencePoint {
    pub name: String,
    pub position: GeoPoint,
    /// Shown to this side only (and the white cell); `None`: every side.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side: Option<SideId>,
}

/// How serious a scenario problem is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// The scenario cannot be saved or played.
    Error,
    /// Allowed, but probably a mistake.
    Warning,
}

/// One problem found by [`Scenario::check`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ScenarioIssue {
    pub severity: Severity,
    pub message: String,
}

fn domain_from_db(s: &str) -> Option<Domain> {
    Some(match s {
        "surface" => Domain::Surface,
        "subsurface" => Domain::Subsurface,
        "air" => Domain::Air,
        "land" => Domain::Land,
        _ => return None,
    })
}

impl Scenario {
    pub fn from_json(text: &str) -> Result<Self, ScenarioError> {
        #[derive(Deserialize)]
        struct Header {
            format: String,
            format_version: u32,
        }
        let header: Header = serde_json::from_str(text)?;
        if header.format != SCENARIO_FORMAT {
            return Err(ScenarioError::WrongFormat(header.format));
        }
        if header.format_version > SCENARIO_FORMAT_VERSION {
            return Err(ScenarioError::FutureVersion {
                found: header.format_version,
                supported: SCENARIO_FORMAT_VERSION,
            });
        }
        let scenario: Scenario = serde_json::from_str(text)?;
        scenario.validate()?;
        Ok(scenario)
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, ScenarioError> {
        Self::from_json(&std::fs::read_to_string(path)?)
    }

    /// Structural checks: unique side and unit ids, every unit on a declared side, and every
    /// unit without a platform carrying its own domain, max speed and acceleration. Fails with
    /// the first problem; [`Scenario::check`] lists them all.
    /// Per-unit value checks run when the spawn command is submitted.
    pub fn validate(&self) -> Result<(), ScenarioError> {
        match self.structural_issues().into_iter().next() {
            Some(e) => Err(ScenarioError::Invalid(e)),
            None => Ok(()),
        }
    }

    fn structural_issues(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut sides = BTreeSet::new();
        for s in &self.sides {
            if !sides.insert(s.id) {
                out.push(format!("duplicate side id {}", s.id));
            }
        }
        for s in &self.sides {
            for p in &s.postures {
                if p.toward == s.id || !sides.contains(&p.toward) {
                    out.push(format!(
                        "side {} has a posture toward {}, which is itself or undeclared",
                        s.id, p.toward
                    ));
                }
            }
        }
        let mut groups = BTreeSet::new();
        for g in &self.groups {
            if !sides.contains(&g.side) || g.name.trim().is_empty() {
                out.push(format!(
                    "group {:?} needs a declared side and a name",
                    g.name
                ));
            }
            if !groups.insert((g.side, g.name.clone())) {
                out.push(format!("duplicate group {:?} on side {}", g.name, g.side));
            }
        }
        let mut units = BTreeSet::new();
        for u in &self.units {
            if let Some(g) = &u.group {
                if !groups.contains(&(u.side, g.clone())) {
                    out.push(format!(
                        "unit {} is in group {g:?}, which its side does not declare",
                        u.id
                    ));
                }
            }
            if !units.insert(u.id) {
                out.push(format!("duplicate unit id {}", u.id));
            }
            if !sides.contains(&u.side) {
                out.push(format!("unit {} is on undeclared side {}", u.id, u.side));
            }
            if u.fuel_level
                .is_some_and(|f| !(f.is_finite() && (0.0..=1.0).contains(&f)))
            {
                out.push(format!(
                    "unit {}: fuel_level must be a fraction from 0 to 1",
                    u.id
                ));
            }
            if u.platform.is_none()
                && (u.domain.is_none() || u.max_speed_mps.is_none() || u.accel_mps2.is_none())
            {
                out.push(format!(
                    "unit {} has no platform, so it needs domain, max_speed_mps and accel_mps2",
                    u.id
                ));
            }
        }
        for m in &self.minefields {
            if !sides.contains(&m.side)
                || !(m.radius_m.is_finite() && m.radius_m > 0.0)
                || m.mines == 0
            {
                out.push(format!(
                    "minefield of {} × {} needs a declared side, a radius > 0 and mines > 0",
                    m.mines, m.weapon
                ));
            }
        }
        if let Err(e) = self.environment.validate() {
            out.push(format!("environment: {e}"));
        }
        out.extend(self.event_issues(&sides));
        if !self.seats.is_empty() {
            if let Err(e) = sim_core::validate_seats(&self.seats) {
                out.push(e);
            }
            for s in &self.seats {
                if let SeatRole::Side { side } = s.role {
                    if !sides.contains(&side) {
                        out.push(format!("seat {:?} plays undeclared side {side}", s.name));
                    }
                }
            }
        }
        let mut names = BTreeSet::new();
        for r in &self.reference_points {
            if r.name.trim().is_empty() {
                out.push("a reference point has no name".into());
            } else if !names.insert(r.name.trim()) {
                out.push(format!("duplicate reference point {:?}", r.name));
            }
            if !r.position.is_valid() {
                out.push(format!(
                    "reference point {:?} has an invalid position",
                    r.name
                ));
            }
            if r.side.is_some_and(|s| !sides.contains(&s)) {
                out.push(format!(
                    "reference point {:?} is for an undeclared side",
                    r.name
                ));
            }
        }
        if let Err(e) = sim_core::validate_planning(&self.facilities, &self.zones) {
            out.push(e);
        }
        let side_of: BTreeMap<UnitId, SideId> = self.units.iter().map(|u| (u.id, u.side)).collect();
        for f in &self.facilities {
            if !sides.contains(&f.side) {
                out.push(format!(
                    "facility {:?} is on undeclared side {}",
                    f.name, f.side
                ));
            }
            for (what, list) in [("subordinate", &f.subordinates), ("relay", &f.relays)] {
                for u in list {
                    if side_of.get(u) != Some(&f.side) {
                        out.push(format!(
                            "facility {:?}: {what} {u} is not a unit of its side",
                            f.name
                        ));
                    }
                }
            }
        }
        for u in &self.units {
            if let Some(b) = u.base {
                let ok = self.facilities.iter().any(|f| {
                    f.id == b && f.side == u.side && f.kind == sim_core::FacilityKind::Airbase
                });
                if !ok {
                    out.push(format!(
                        "unit {}: base {b} is not an airbase of its side",
                        u.id
                    ));
                }
            }
        }
        for z in &self.zones {
            if !sides.contains(&z.side) {
                out.push(format!(
                    "zone {:?} is on undeclared side {}",
                    z.name, z.side
                ));
            }
            for u in &z.assets {
                if side_of.get(u) != Some(&z.side) {
                    out.push(format!(
                        "zone {:?}: unit {u} to task is not a unit of its side",
                        z.name
                    ));
                }
            }
        }
        out.extend(self.action_group_issues());
        if self.duration_s == 0 {
            out.push("duration_s must be > 0".into());
        }
        out
    }

    /// Action groups: named, leader and members units of the group's side, each member in one
    /// group only and without a mission of its own, stations finite with a range > 0.
    fn action_group_issues(&self) -> Vec<String> {
        let mut out = Vec::new();
        let side_of: BTreeMap<UnitId, SideId> = self.units.iter().map(|u| (u.id, u.side)).collect();
        let mut names = BTreeSet::new();
        let mut placed = BTreeSet::new();
        for g in &self.action_groups {
            if g.name.trim().is_empty() || !names.insert(g.name.trim()) {
                out.push(format!("action group {:?} needs a unique name", g.name));
            }
            if self
                .groups
                .iter()
                .any(|d| d.side == g.side && d.name == g.name)
            {
                out.push(format!(
                    "action group {:?} has the name of a declared group",
                    g.name
                ));
            }
            for id in std::iter::once(g.leader).chain(g.members.iter().map(|m| m.unit)) {
                if let Some(other) = self
                    .units
                    .iter()
                    .find(|u| u.id == id)
                    .and_then(|u| u.group.as_ref())
                {
                    out.push(format!(
                        "unit {id} is in group {other:?}, so it cannot join action group {:?}",
                        g.name
                    ));
                }
            }
            if side_of.get(&g.leader) != Some(&g.side) {
                out.push(format!(
                    "action group {:?}: leader {} is not a unit of side {}",
                    g.name, g.leader, g.side
                ));
            }
            for m in &g.members {
                if m.unit == g.leader || side_of.get(&m.unit) != Some(&g.side) {
                    out.push(format!(
                        "action group {:?}: member {} must be another unit of side {}",
                        g.name, m.unit, g.side
                    ));
                }
                if !placed.insert(m.unit) {
                    out.push(format!("unit {} is in more than one action group", m.unit));
                }
                if self
                    .units
                    .iter()
                    .any(|u| u.id == m.unit && u.mission.is_some())
                {
                    out.push(format!(
                        "unit {} has its own mission, so it cannot keep station in {:?}",
                        m.unit, g.name
                    ));
                }
                if !(m.bearing_deg.is_finite() && m.range_m.is_finite() && m.range_m > 0.0) {
                    out.push(format!(
                        "action group {:?}: member {} needs a finite bearing and a range > 0",
                        g.name, m.unit
                    ));
                }
            }
        }
        out
    }

    /// Every problem the scenario editor should show: the structural checks, each unit's
    /// platform resolution, minefield mine types, and a trial build (commands and scripts),
    /// then warnings for likely mistakes. Errors first.
    #[must_use]
    pub fn check(&self, db: &PlatformDb) -> Vec<ScenarioIssue> {
        let error = |message: String| ScenarioIssue {
            severity: Severity::Error,
            message,
        };
        let warning = |message: String| ScenarioIssue {
            severity: Severity::Warning,
            message,
        };
        let mut issues: Vec<ScenarioIssue> =
            self.structural_issues().into_iter().map(error).collect();
        for u in &self.units {
            if let Err(e) = resolve_unit(u, db, self.fuel) {
                issues.push(error(match e {
                    ScenarioError::Invalid(m) => m,
                    e => format!("unit {}: {e}", u.id),
                }));
            }
        }
        for m in &self.minefields {
            match db.weapon(&m.weapon) {
                Ok(Some(w)) if w.2 == "mine" => {}
                Ok(Some(_)) => issues.push(error(format!(
                    "minefield weapon {} is not a mine",
                    m.weapon
                ))),
                Ok(None) => issues.push(error(format!(
                    "minefield weapon {} is not in the platform database",
                    m.weapon
                ))),
                Err(e) => issues.push(error(format!("platform database: {e}"))),
            }
        }
        if issues.is_empty() {
            // Everything the loader checks when the commands are submitted, including scripts.
            if let Err(e) = self.build(None, db) {
                issues.push(error(e.to_string()));
            }
        }
        // Warnings.
        for s in &self.sides {
            if !self.units.iter().any(|u| u.side == s.id && !u.reserve) {
                issues.push(warning(format!(
                    "side {} ({}) starts with no units",
                    s.id, s.name
                )));
            }
        }
        let land = sim_geo::land::embedded();
        let mut seen = BTreeSet::new();
        for u in &self.units {
            if !seen.insert(u.name.trim()) {
                issues.push(warning(format!(
                    "more than one unit is called {:?}",
                    u.name
                )));
            }
            let sea = match u.domain {
                Some(d) => matches!(d, Domain::Surface | Domain::Subsurface),
                None => u
                    .platform
                    .as_deref()
                    .and_then(|p| db.platform(p).ok().flatten())
                    .is_some_and(|p| p.domain == "surface" || p.domain == "subsurface"),
            };
            if sea && u.position.is_valid() && land.is_land(u.position) {
                issues.push(warning(format!(
                    "unit {} ({}) starts on land",
                    u.id, u.name
                )));
            }
        }
        for f in &self.facilities {
            if f.area.as_ref().is_some_and(|a| !a.contains(f.position)) {
                issues.push(warning(format!(
                    "facility {:?}: its point is outside its area, so units steer for a point                      outside the water it serves",
                    f.name
                )));
            }
        }
        issues.sort_by_key(|i| i.severity);
        issues
    }

    /// Events: their own checks, then every side and unit they name must be declared, and
    /// `spawn_reserve` must name a reserve unit.
    fn event_issues(&self, sides: &BTreeSet<SideId>) -> Vec<String> {
        use sim_core::{Audience, Condition, DoctrineTarget, EventAction, Trigger};
        let mut out = Vec::new();
        if let Err(e) = sim_core::validate_events(&self.events, &self.injects) {
            out.push(e);
        }
        for i in &self.injects {
            if let Audience::Side { side } = i.to {
                if !sides.contains(&SideId(side)) {
                    out.push(format!(
                        "inject {:?} is addressed to undeclared side S{side}",
                        i.id
                    ));
                }
            }
        }
        let reserves: BTreeSet<u64> = self
            .units
            .iter()
            .filter(|u| u.reserve)
            .map(|u| u.id.0)
            .collect();
        let units: BTreeSet<u64> = self.units.iter().map(|u| u.id.0).collect();
        for e in &self.events {
            let mut side_refs: Vec<u16> = Vec::new();
            let mut unit_refs: Vec<u64> = Vec::new();
            match &e.trigger {
                Trigger::AreaEntered { side, unit, .. } | Trigger::UnitDestroyed { side, unit } => {
                    side_refs.extend(side);
                    unit_refs.extend(unit);
                }
                Trigger::ContactGained { side, target_side } => {
                    side_refs.push(*side);
                    side_refs.extend(target_side);
                }
                Trigger::Time { .. } | Trigger::Every { .. } => {}
            }
            for c in &e.conditions {
                match c {
                    Condition::UnitAlive { unit } | Condition::UnitGone { unit } => {
                        unit_refs.push(*unit);
                    }
                    Condition::UnitsAlive { side, .. } => side_refs.push(*side),
                    _ => {}
                }
            }
            for a in &e.actions {
                match a {
                    EventAction::Message {
                        to: Audience::Side { side },
                        ..
                    } => side_refs.push(*side),
                    EventAction::SetPosture { side, toward, .. } => {
                        side_refs.extend([*side, *toward]);
                    }
                    EventAction::SetDoctrine { scope, .. } => match scope {
                        DoctrineTarget::Side { side } | DoctrineTarget::Group { side, .. } => {
                            side_refs.push(*side);
                        }
                        DoctrineTarget::Unit { unit } => unit_refs.push(*unit),
                    },
                    EventAction::SetSpeed { unit, .. }
                    | EventAction::SetRoute { unit, .. }
                    | EventAction::AssignMission { unit, .. } => unit_refs.push(*unit),
                    EventAction::SpawnReserve { unit } if !reserves.contains(unit) => {
                        out.push(format!(
                            "event {:?} spawns U{unit}, which is not a reserve unit",
                            e.id
                        ));
                    }
                    _ => {}
                }
            }
            if let Some(s) = side_refs.iter().find(|s| !sides.contains(&SideId(**s))) {
                out.push(format!("event {:?} names undeclared side S{s}", e.id));
            }
            if let Some(u) = unit_refs.iter().find(|u| !units.contains(u)) {
                out.push(format!("event {:?} names undeclared unit U{u}", e.id));
            }
        }
        out
    }

    /// The scenario's seats, or the default: seat 0 `White cell`, then seat n for the n-th side,
    /// named after it.
    #[must_use]
    pub fn seats_or_default(&self) -> Vec<Seat> {
        if !self.seats.is_empty() {
            return self.seats.clone();
        }
        std::iter::once(Seat {
            id: 0,
            name: "White cell".into(),
            role: SeatRole::WhiteCell,
        })
        .chain(self.sides.iter().zip(1u32..).map(|(s, id)| Seat {
            id,
            name: s.name.clone(),
            role: SeatRole::Side { side: s.id },
        }))
        .collect()
    }

    /// Run length in ticks. `TICK_SECONDS` is 1.0, so seconds and ticks are the same number (ADR-0003).
    #[must_use]
    pub fn duration_ticks(&self) -> Tick {
        Tick(self.duration_s)
    }

    /// Resolves every unit against the platform database into a spawn command payload.
    pub fn resolve(&self, db: &PlatformDb) -> Result<Vec<UnitSpawn>, ScenarioError> {
        self.units
            .iter()
            .map(|u| resolve_unit(u, db, self.fuel))
            .collect()
    }

    /// Builds a simulation at tick 0, with the initial units queued as scenario commands.
    pub fn build(&self, seed: Option<u64>, db: &PlatformDb) -> Result<Simulation, ScenarioError> {
        let spawns = self.resolve(db)?;
        let mut sim = Simulation::new(seed.unwrap_or(self.seed));
        // Ships and submarines navigate round the bundled coastline (docs/models/land-avoidance.md).
        sim.set_land_mask(sim_geo::land::embedded());
        // Environment, the ESM emitter library, postures and doctrine first, so they are in
        // force when the units spawn.
        let mut c2 = Vec::new();
        let catalog = weapon_catalog(
            &spawns,
            self.minefields.iter().map(|m| m.weapon.as_str()),
            db,
        )?;
        if !catalog.is_empty() {
            c2.push(CommandPayload::SetWeaponCatalog { weapons: catalog });
        }
        let library = emitter_library(&spawns, db)?;
        if !library.is_empty() {
            c2.push(CommandPayload::SetEmitterLibrary { entries: library });
        }
        if self.environment != Environment::default() {
            c2.push(CommandPayload::SetEnvironment {
                environment: self.environment,
            });
        }
        if !self.weather_zones.is_empty() {
            c2.push(CommandPayload::SetWeather {
                zones: self.weather_zones.clone(),
            });
        }
        if !self.sound_profiles.is_empty() {
            c2.push(CommandPayload::SetSoundProfiles {
                profiles: self.sound_profiles.clone(),
            });
        }
        for s in &self.sides {
            for p in &s.postures {
                c2.push(CommandPayload::SetPosture {
                    side: s.id,
                    toward: p.toward,
                    posture: p.posture,
                });
            }
            if s.doctrine != Doctrine::default() {
                c2.push(CommandPayload::SetDoctrine {
                    scope: DoctrineScope::Side { side: s.id },
                    doctrine: s.doctrine,
                });
            }
        }
        for g in &self.groups {
            c2.push(CommandPayload::SetDoctrine {
                scope: DoctrineScope::Group {
                    side: g.side,
                    group: g.name.clone(),
                },
                doctrine: g.doctrine,
            });
        }
        // Minefields need the catalogue (sent above) for their mine type.
        for m in &self.minefields {
            c2.push(CommandPayload::CreateMinefield {
                side: m.side,
                centre: m.centre,
                radius_m: m.radius_m,
                mines: m.mines,
                weapon: m.weapon.clone(),
            });
        }
        if !self.facilities.is_empty()
            || !self.zones.is_empty()
            || !self.action_groups.is_empty()
            || !self.lanes.is_empty()
        {
            c2.push(CommandPayload::SetPlanning {
                facilities: self.facilities.clone(),
                zones: self.zones.clone(),
                action_groups: (!self.action_groups.is_empty()).then(|| self.action_groups.clone()),
                lanes: (!self.lanes.is_empty()).then(|| self.lanes.clone()),
            });
        }
        if !self.seats.is_empty() {
            c2.push(CommandPayload::SetSeats {
                seats: self.seats.clone(),
            });
        }
        for payload in c2 {
            sim.submit(Command {
                tick: Tick(0),
                issuer: Issuer::Scenario,
                side: None,
                payload,
            })?;
        }
        let mut reserves = Vec::new();
        let stations: BTreeMap<UnitId, Mission> = self
            .action_groups
            .iter()
            .flat_map(|g| {
                g.members.iter().map(move |m| {
                    (
                        m.unit,
                        Mission::Escort {
                            escorted: g.leader.0,
                            bearing_deg: m.bearing_deg,
                            range_m: m.range_m,
                        },
                    )
                })
            })
            .collect();
        let in_group: BTreeMap<UnitId, String> = self
            .action_groups
            .iter()
            .flat_map(|g| {
                std::iter::once(g.leader)
                    .chain(g.members.iter().map(|m| m.unit))
                    .map(move |u| (u, g.name.clone()))
            })
            .collect();
        for g in &self.action_groups {
            if g.doctrine != Doctrine::default() {
                sim.submit(Command {
                    tick: Tick(0),
                    issuer: Issuer::Scenario,
                    side: None,
                    payload: CommandPayload::SetDoctrine {
                        scope: DoctrineScope::Group {
                            side: g.side,
                            group: g.name.clone(),
                        },
                        doctrine: g.doctrine,
                    },
                })?;
            }
        }
        for (mut s, u) in spawns.into_iter().zip(&self.units) {
            if let Some(m) = stations.get(&u.id) {
                s.mission = Some(m.clone());
            }
            if let Some(name) = in_group.get(&u.id) {
                s.group = Some(name.clone());
            }
            if u.reserve {
                reserves.push(s);
                continue;
            }
            sim.submit(Command {
                tick: Tick(0),
                issuer: Issuer::Scenario,
                side: None,
                payload: CommandPayload::SpawnUnit(s),
            })?;
        }
        // Last, so area triggers see the units already placed.
        if !self.events.is_empty() || !reserves.is_empty() || !self.injects.is_empty() {
            sim.submit(Command {
                tick: Tick(0),
                issuer: Issuer::Scenario,
                side: None,
                payload: CommandPayload::SetEvents {
                    events: self.events.clone(),
                    reserves,
                    injects: self.injects.clone(),
                },
            })?;
        }
        Ok(sim)
    }
}

fn resolve_unit(u: &ScenarioUnit, db: &PlatformDb, fuel: bool) -> Result<UnitSpawn, ScenarioError> {
    let invalid = |msg: String| ScenarioError::Invalid(format!("unit {}: {msg}", u.id));
    let (
        domain,
        max_speed,
        cruise,
        accel,
        sensors,
        rcs,
        turn,
        noise,
        category,
        length,
        stores,
        damage,
        fuel_basis,
        (draft_m, displacement_t),
    ) = match &u.platform {
        Some(pid) => {
            let p = db
                .platform(pid)?
                .ok_or_else(|| invalid(format!("unknown platform {pid}")))?;
            let db_domain = domain_from_db(&p.domain).ok_or_else(|| {
                invalid(format!("platform {pid} has unknown domain {}", p.domain))
            })?;
            if let Some(d) = u.domain {
                if d != db_domain {
                    return Err(invalid(format!(
                        "domain {d:?} contradicts platform {pid} ({db_domain:?})"
                    )));
                }
            }
            let sensors = db
                .sensors_of(pid)?
                .into_iter()
                .map(|s| {
                    let kind = SensorKind::from_db(&s.kind).ok_or_else(|| {
                        invalid(format!(
                            "sensor {} has unknown kind {}",
                            s.sensor_id, s.kind
                        ))
                    })?;
                    Ok(SensorSpec {
                        sensor_id: s.sensor_id,
                        kind,
                        mount_height_m: s.mount_height_m,
                        min_range_m: s.min_range_m,
                        max_range_m: s.max_range_m,
                        ref_range_m: s.ref_detect_range_m,
                        ref_rcs_m2: s.ref_target_rcs_m2,
                        max_target_altitude_m: s.max_target_altitude_m,
                        clutter_db_per_sea_state: s.clutter_db_per_sea_state,
                        beamwidth_deg: s.beamwidth_deg,
                        active: s.is_active,
                        sonar: s.sonar.iter().any(Option::is_some).then(|| {
                            sim_core::SonarFigures {
                                frequency_khz: s.sonar[0],
                                di_db: s.sonar[1],
                                dt_db: s.sonar[2],
                                source_level_db: s.sonar[3],
                                self_noise_db: s.sonar[4],
                            }
                        }),
                    })
                })
                .collect::<Result<Vec<_>, ScenarioError>>()?;
            (
                db_domain,
                u.max_speed_mps.unwrap_or(p.max_speed_mps),
                p.cruise_speed_mps,
                u.accel_mps2.unwrap_or(p.accel_mps2),
                sensors,
                db.rcs_m2(pid)?,
                turn_performance(&p.category, p.length_m),
                radiated_noise_db(&p.category),
                Some(p.category.clone()),
                p.length_m,
                db.stores_of(pid)?.into_iter().collect(),
                db.damage_profile_of(pid)?
                    .map(|(dp, mk, fire, flood)| DamageProfile {
                        damage_points: dp,
                        mission_kill_fraction: mk,
                        fire_susceptibility: fire,
                        flooding_susceptibility: flood,
                    }),
                // (capacity t, endurance m, cruise m/s) for ships, submarines and aircraft with
                // figures; an aircraft's fuel is a share of its maximum take-off mass.
                match (p.endurance_m, p.displacement_t) {
                    (Some(e), Some(d))
                        if db_domain == Domain::Air
                            && e > 0.0
                            && d > 0.0
                            && p.cruise_speed_mps > 0.0 =>
                    {
                        Some((
                            d * aircraft_fuel_fraction(&p.category),
                            e,
                            p.cruise_speed_mps,
                        ))
                    }
                    (Some(e), Some(d))
                        if matches!(db_domain, Domain::Surface | Domain::Subsurface)
                            && e > 0.0
                            && d > 0.0
                            && p.cruise_speed_mps > 0.0 =>
                    {
                        Some((d * FUEL_FRACTION_OF_DISPLACEMENT, e, p.cruise_speed_mps))
                    }
                    _ => None,
                },
                (p.draft_m, p.displacement_t),
            )
        }
        // `validate` guarantees these are present when there is no platform.
        None => (
            u.domain.ok_or_else(|| invalid("missing domain".into()))?,
            u.max_speed_mps
                .ok_or_else(|| invalid("missing max_speed_mps".into()))?,
            0.0,
            u.accel_mps2
                .ok_or_else(|| invalid("missing accel_mps2".into()))?,
            vec![],
            None,
            (None, None),
            None,
            None,
            None,
            BTreeMap::new(),
            None,
            None,
            (None, None),
        ),
    };
    let mut stores = stores;
    let air = if domain == Domain::Air && (u.platform.is_some() || u.base.is_some()) {
        let a = air_spec(u, db, category.as_deref(), cruise, max_speed)?;
        // It carries the loadout it starts with.
        if let Some(o) = a
            .loadout
            .as_deref()
            .and_then(|l| a.loadouts.iter().find(|o| o.id == l))
        {
            stores = o.stores.clone();
        }
        Some(a)
    } else {
        if u.base.is_some() || u.readiness.is_some() || u.loadout.is_some() || u.airborne {
            return Err(invalid(
                "only aircraft have a base, readiness, loadout or airborne start".into(),
            ));
        }
        None
    };
    Ok(UnitSpawn {
        id: u.id,
        name: u.name.clone(),
        side: u.side,
        domain,
        position: u.position,
        altitude_m: u.altitude_m,
        heading_deg: u.heading_deg,
        speed_mps: u.speed_mps.unwrap_or(cruise).min(max_speed),
        max_speed_mps: max_speed,
        accel_mps2: accel,
        route: u.route.clone(),
        platform: u.platform.clone(),
        sensors,
        rcs_m2: rcs,
        max_turn_rate_dps: turn.0,
        min_turn_radius_m: turn.1,
        group: u.group.clone(),
        doctrine: u.doctrine,
        // A jammer from the scenario, else the platform's strongest ECM (migration 0004).
        jammer: match (&u.jammer, &u.platform) {
            (Some(j), _) => Some(*j),
            (None, Some(pid)) => db
                .jammer_of(pid)?
                .map(|jn_db_at_1km| sim_core::JammerSpec { jn_db_at_1km }),
            (None, None) => None,
        },
        radiated_noise_db: noise,
        category,
        length_m: length,
        stores,
        damage_profile: damage,
        mission: u.mission.clone(),
        draft_m,
        displacement_t,
        fuel: fuel_basis
            .filter(|_| fuel)
            .map(|(capacity_t, endurance_m, cruise_mps)| FuelSpec {
                capacity_t,
                endurance_m,
                cruise_mps,
                level: u.fuel_level.unwrap_or(1.0),
                bingo: None,
                kind: if domain == Domain::Air {
                    sim_core::FuelKind::Aviation
                } else {
                    sim_core::FuelKind::Marine
                },
                idle_fraction: (domain == Domain::Air).then_some(sim_core::AIRCRAFT_IDLE_FRACTION),
            }),
        air,
    })
}

/// Share of an aircraft's maximum take-off mass that is fuel, by category (generic).
fn aircraft_fuel_fraction(category: &str) -> f64 {
    match category {
        "fighter" => 0.28,
        "helicopter" => 0.15,
        "mpa" => 0.35,
        "aew" => 0.3,
        "tanker" => 0.45,
        "uav" => 0.35,
        _ => 0.3,
    }
}

/// Altitude an aircraft flies at once launched, by category, m (generic).
fn cruise_altitude_m(category: &str) -> f64 {
    match category {
        "fighter" => 8_000.0,
        "helicopter" => 150.0,
        "mpa" => 3_000.0,
        "aew" | "tanker" => 9_000.0,
        _ => 5_000.0,
    }
}

/// An aircraft's part in air operations (`docs/models/air-operations.md`): its loadouts from the
/// database, the scenario's base, readiness and loadout, and what its category makes it.
fn air_spec(
    u: &ScenarioUnit,
    db: &PlatformDb,
    category: Option<&str>,
    cruise_mps: f64,
    max_speed_mps: f64,
) -> Result<sim_core::AirSpec, ScenarioError> {
    let invalid = |msg: String| ScenarioError::Invalid(format!("unit {}: {msg}", u.id));
    let loadouts: Vec<sim_core::LoadoutOption> = match &u.platform {
        Some(pid) => db
            .loadouts_of(pid)?
            .into_iter()
            .map(|(id, name, role, stores)| sim_core::LoadoutOption {
                id,
                name,
                role,
                stores: stores.into_iter().collect(),
            })
            .collect(),
        None => Vec::new(),
    };
    let loadout = match &u.loadout {
        Some(l) if !loadouts.iter().any(|o| &o.id == l) => {
            return Err(invalid(format!("unknown loadout {l:?}")));
        }
        Some(l) => Some(l.clone()),
        None => loadouts.first().map(|o| o.id.clone()),
    };
    let category = category.unwrap_or("");
    let cruise = if cruise_mps > 0.0 {
        cruise_mps
    } else {
        0.6 * max_speed_mps
    };
    Ok(sim_core::AirSpec {
        base: u.base,
        parked: u.base.is_some() && !u.airborne,
        readiness: u.readiness.unwrap_or_default(),
        maintenance_s: u.maintenance_s,
        loadouts,
        loadout,
        tanker: (category == "tanker").then_some(sim_core::TankerSpec {
            hoses: sim_core::DEFAULT_HOSES,
            transfer_t_per_min: sim_core::DEFAULT_TRANSFER_T_PER_MIN,
        }),
        cruise_altitude_m: if u.altitude_m > 0.0 {
            u.altitude_m
        } else {
            cruise_altitude_m(category)
        },
        cruise_mps: cruise,
        // Helicopters, and slow (rotary or tilt) UAVs, use any deck.
        vtol: category == "helicopter" || (category == "uav" && max_speed_mps <= 150.0 * 0.514_444),
        large: matches!(category, "mpa" | "aew" | "tanker"),
    })
}

/// The weapon catalogue: every weapon carried by a unit in this scenario, from the database.
fn weapon_catalog<'a>(
    spawns: &'a [UnitSpawn],
    mine_types: impl Iterator<Item = &'a str>,
    db: &PlatformDb,
) -> Result<Vec<WeaponSpec>, ScenarioError> {
    let ids: BTreeSet<&str> = spawns
        .iter()
        .flat_map(|s| {
            // Every loadout an aircraft can be given, not only the one it starts with.
            s.stores.keys().chain(
                s.air
                    .iter()
                    .flat_map(|a| a.loadouts.iter().flat_map(|o| o.stores.keys())),
            )
        })
        .map(String::as_str)
        .chain(mine_types)
        .collect();
    let mut out = Vec::new();
    for want in ids {
        let Some((id, name, kind, guidance, min, max, speed, warhead, ph, domains)) =
            db.weapon(want)?
        else {
            return Err(ScenarioError::Invalid(format!("unknown weapon {want}")));
        };
        let kind = WeaponKind::from_db(&kind).ok_or_else(|| {
            ScenarioError::Invalid(format!("weapon {id} has unknown kind {kind}"))
        })?;
        // Ranges against particular domains (a gun against aircraft), migration 0005.
        let max_range_by_target = db.weapon_target_ranges(&id)?;
        out.push(WeaponSpec {
            id,
            name,
            kind,
            guidance: Guidance::from_db(&guidance),
            min_range_m: min,
            max_range_m: max,
            speed_mps: speed,
            warhead_kg: warhead,
            base_ph: ph,
            target_domains: domains.split(',').map(|d| d.trim().to_owned()).collect(),
            max_range_by_target,
        });
    }
    Ok(out)
}

/// The ESM emitter library for the radars fitted in this scenario: each radar type with every
/// database platform class that carries it (not only the classes present in the scenario).
fn emitter_library(
    spawns: &[UnitSpawn],
    db: &PlatformDb,
) -> Result<Vec<EmitterEntry>, ScenarioError> {
    let radars: BTreeSet<&str> = spawns
        .iter()
        .flat_map(|s| s.sensors.iter())
        .filter(|s| s.kind == SensorKind::Radar)
        .map(|s| s.sensor_id.as_str())
        .collect();
    radars
        .into_iter()
        .map(|r| {
            let carriers = db
                .carriers_of(r)?
                .into_iter()
                .map(|(platform, category, domain)| Carrier {
                    platform,
                    category,
                    domain,
                })
                .collect();
            Ok(EmitterEntry {
                emitter: r.to_owned(),
                carriers,
            })
        })
        .collect()
}

/// Generic radiated noise at 10 kn by platform category, dB (the database has no acoustic data
/// yet; see docs/models/sonar.md). `None` for categories that are not waterborne, and for units
/// without a platform (the engine's domain default then applies).
fn radiated_noise_db(category: &str) -> Option<f64> {
    match category {
        "submarine" => Some(115.0),
        "surface_combatant" => Some(140.0),
        "auxiliary" => Some(145.0),
        "merchant" => Some(150.0),
        _ => None,
    }
}

/// Generic turn performance by platform class (the database has no turn data yet; see
/// docs/models/geodesy-and-movement.md §Turning). Returns (max rate deg/s, min radius m).
fn turn_performance(category: &str, length_m: Option<f64>) -> (Option<f64>, Option<f64>) {
    match category {
        // Ships: tactical diameter ≈ 5 hull lengths → radius 2.5 lengths, capped at 3°/s.
        "surface_combatant" | "auxiliary" | "merchant" | "submarine" => {
            (Some(3.0), Some(2.5 * length_m.unwrap_or(100.0)))
        }
        "fighter" => (Some(15.0), None),
        "helicopter" | "uav" => (Some(8.0), None),
        "mpa" | "aew" | "tanker" => (Some(3.0), None),
        // Land units and fixed sites: no turn limit.
        _ => (None, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INLINE: &str = r#"{
        "format": "nws-scenario", "format_version": 1, "name": "t", "seed": 1, "duration_s": 10,
        "sides": [{"id": 1, "name": "Blue"}],
        "units": [{"id": 1, "name": "A", "side": 1, "domain": "surface",
                   "position": {"lat_deg": 0.0, "lon_deg": 0.0},
                   "max_speed_mps": 10.0, "accel_mps2": 0.1}]
    }"#;

    const WITH_PLATFORM: &str = r#"{
        "format": "nws-scenario", "format_version": 2, "name": "t", "seed": 1, "duration_s": 10,
        "sides": [{"id": 1, "name": "Blue"}],
        "units": [{"id": 1, "name": "F", "side": 1, "platform": "GEN-FFG",
                   "position": {"lat_deg": 0.0, "lon_deg": 0.0}}]
    }"#;

    fn db() -> PlatformDb {
        sim_db::embedded::open_platform_db().unwrap()
    }

    #[test]
    fn v1_inline_units_still_load() {
        let s = Scenario::from_json(INLINE).unwrap();
        let mut sim = s.build(None, &db()).unwrap();
        sim.step();
        let snap = sim.snapshot(sim_core::Viewer::WhiteCell);
        assert_eq!(snap.units.len(), 1);
        assert_eq!(snap.units[0].platform, None);
    }

    #[test]
    fn platform_supplies_performance_sensors_and_rcs() {
        let s = Scenario::from_json(WITH_PLATFORM).unwrap();
        let spawn = &s.resolve(&db()).unwrap()[0];
        assert_eq!(spawn.domain, Domain::Surface);
        assert!((spawn.max_speed_mps - 28.0 * 1852.0 / 3600.0).abs() < 1e-9);
        // Starts at the platform's cruise speed (16 kn).
        assert!((spawn.speed_mps - 16.0 * 1852.0 / 3600.0).abs() < 1e-9);
        assert_eq!(spawn.rcs_m2, Some(5000.0));
        assert!(spawn
            .sensors
            .iter()
            .any(|s| s.kind == SensorKind::Radar && s.sensor_id == "SEN-RDR-3D-MR"));
    }

    #[test]
    fn overrides_win_and_contradictions_are_rejected() {
        let over = WITH_PLATFORM.replace(
            r#""platform": "GEN-FFG","#,
            r#""platform": "GEN-FFG", "max_speed_mps": 5.0, "speed_mps": 9.0,"#,
        );
        let spawn = &Scenario::from_json(&over).unwrap().resolve(&db()).unwrap()[0];
        assert!((spawn.max_speed_mps - 5.0).abs() < 1e-12);
        assert!(
            (spawn.speed_mps - 5.0).abs() < 1e-12,
            "initial speed capped at max"
        );

        let wrong = WITH_PLATFORM.replace(
            r#""platform": "GEN-FFG","#,
            r#""platform": "GEN-FFG", "domain": "air","#,
        );
        assert!(matches!(
            Scenario::from_json(&wrong).unwrap().resolve(&db()),
            Err(ScenarioError::Invalid(_))
        ));
        let unknown = WITH_PLATFORM.replace("GEN-FFG", "GEN-NOPE");
        assert!(matches!(
            Scenario::from_json(&unknown).unwrap().resolve(&db()),
            Err(ScenarioError::Invalid(_))
        ));
    }

    #[test]
    fn rejects_wrong_format_future_version_and_incomplete_units() {
        let wrong = INLINE.replace("nws-scenario", "other");
        assert!(matches!(
            Scenario::from_json(&wrong),
            Err(ScenarioError::WrongFormat(_))
        ));
        let future = INLINE.replace("\"format_version\": 1", "\"format_version\": 99");
        assert!(matches!(
            Scenario::from_json(&future),
            Err(ScenarioError::FutureVersion { .. })
        ));
        let no_speed = INLINE.replace("\"max_speed_mps\": 10.0, ", "");
        assert!(matches!(
            Scenario::from_json(&no_speed),
            Err(ScenarioError::Invalid(_))
        ));
        let bad_side = INLINE.replace("\"side\": 1", "\"side\": 2");
        assert!(matches!(
            Scenario::from_json(&bad_side),
            Err(ScenarioError::Invalid(_))
        ));
    }

    const WITH_C2: &str = r#"{
        "format": "nws-scenario", "format_version": 2, "name": "c2", "seed": 1, "duration_s": 10,
        "sides": [
            {"id": 1, "name": "Blue", "postures": [{"toward": 2, "posture": "hostile"}],
             "doctrine": {"wcs": "free", "emcon": "silent"}},
            {"id": 2, "name": "Red"}
        ],
        "groups": [{"side": 1, "name": "TG1", "doctrine": {"wcs": "safe"}}],
        "units": [{"id": 1, "name": "A", "side": 1, "domain": "surface", "group": "TG1",
                   "doctrine": {"emcon": "active"},
                   "position": {"lat_deg": 0.0, "lon_deg": 0.0},
                   "max_speed_mps": 10.0, "accel_mps2": 0.1}]
    }"#;

    #[test]
    fn postures_groups_and_doctrine_load_and_apply() {
        let s = Scenario::from_json(WITH_C2).unwrap();
        let mut sim = s.build(None, &db()).unwrap();
        sim.step();
        let snap = sim.snapshot(sim_core::Viewer::Side { side: SideId(1) });
        let d = snap.units[0].doctrine;
        assert_eq!(d.wcs, sim_core::Wcs::Safe, "group over side");
        assert_eq!(d.emcon, sim_core::Emcon::Active, "unit over side");
        assert_eq!(snap.postures.len(), 1);
        assert_eq!(snap.postures[0].posture, Posture::Hostile);
    }

    #[test]
    fn bad_postures_and_groups_are_rejected() {
        for (from, to) in [
            (r#""toward": 2"#, r#""toward": 1"#),
            (r#""toward": 2"#, r#""toward": 9"#),
            (r#""group": "TG1""#, r#""group": "TG9""#),
            (r#""side": 1, "name": "TG1""#, r#""side": 7, "name": "TG1""#),
        ] {
            let bad = WITH_C2.replacen(from, to, 1);
            assert!(
                matches!(Scenario::from_json(&bad), Err(ScenarioError::Invalid(_))),
                "{to}"
            );
        }
    }

    #[test]
    fn environment_and_jammers_load_and_are_validated() {
        let with = INLINE
            .replacen(
                r#""duration_s": 10,"#,
                r#""duration_s": 10, "environment": {"sea_state": 4, "weather_loss_db": 2.0},"#,
                1,
            )
            .replacen(
                r#""accel_mps2": 0.1}"#,
                r#""accel_mps2": 0.1, "jammer": {"jn_db_at_1km": 50.0}}"#,
                1,
            );
        let s = Scenario::from_json(&with).unwrap();
        assert_eq!(s.environment.sea_state, 4);
        let spawn = &s.resolve(&db()).unwrap()[0];
        assert_eq!(spawn.jammer.map(|j| j.jn_db_at_1km), Some(50.0));
        let mut sim = s.build(None, &db()).unwrap();
        sim.step();
        let env = sim.snapshot(sim_core::Viewer::WhiteCell).environment;
        assert_eq!((env.sea_state, env.weather_loss_db), (4, 2.0));
        let rough = with.replacen(r#""sea_state": 4"#, r#""sea_state": 11"#, 1);
        assert!(matches!(
            Scenario::from_json(&rough),
            Err(ScenarioError::Invalid(_))
        ));
    }
    const WITH_EVENTS: &str = r#"{
        "format": "nws-scenario", "format_version": 3, "name": "t", "seed": 1, "duration_s": 100,
        "sides": [{"id": 1, "name": "Blue"}, {"id": 2, "name": "Red"}],
        "units": [{"id": 1, "name": "A", "side": 1, "platform": "GEN-FFG",
                   "position": {"lat_deg": 0.0, "lon_deg": 0.0}},
                  {"id": 2, "name": "B", "side": 1, "platform": "GEN-FFG", "reserve": true,
                   "position": {"lat_deg": 0.5, "lon_deg": 0.0}}],
        "events": [{"id": "in", "trigger": {"type": "time", "at_s": 50},
                    "actions": [{"type": "spawn_reserve", "unit": 2},
                                {"type": "message", "to": "side", "side": 1, "text": "B joins"}]}]
    }"#;

    #[test]
    fn events_and_reserves_load_and_a_reserve_waits_for_its_event() {
        let s = Scenario::from_json(WITH_EVENTS).unwrap();
        let mut sim = s.build(None, &db()).unwrap();
        sim.run_until(Tick(10));
        let ids = |sim: &mut Simulation| -> Vec<u64> {
            sim.snapshot(sim_core::Viewer::WhiteCell)
                .units
                .iter()
                .map(|u| u.id.0)
                .collect()
        };
        assert_eq!(ids(&mut sim), vec![1]);
        sim.run_until(Tick(60));
        assert_eq!(ids(&mut sim), vec![1, 2]);
        // Written back out, the reserve flag and the events survive.
        let again = Scenario::from_json(&serde_json::to_string(&s).unwrap()).unwrap();
        assert!(again.units[1].reserve);
        assert_eq!(again.events, s.events);
    }

    #[test]
    fn default_seats_are_the_white_cell_and_one_per_side_and_declared_seats_are_checked() {
        let s = Scenario::from_json(WITH_EVENTS).unwrap();
        let seats = s.seats_or_default();
        let got: Vec<(u32, &str, SeatRole)> = seats
            .iter()
            .map(|x| (x.id, x.name.as_str(), x.role))
            .collect();
        assert_eq!(
            got,
            vec![
                (0, "White cell", SeatRole::WhiteCell),
                (1, "Blue", SeatRole::Side { side: SideId(1) }),
                (2, "Red", SeatRole::Side { side: SideId(2) }),
            ]
        );
        let declared = WITH_EVENTS.replacen(
            r#""events": ["#,
            r#""seats": [{"id": 5, "name": "Umpire", "role": "white_cell"},
                         {"id": 6, "name": "Blue CO", "role": "side", "side": 1}], "events": ["#,
            1,
        );
        let s = Scenario::from_json(&declared).unwrap();
        assert_eq!(s.seats_or_default().len(), 2);
        let mut sim = s.build(None, &db()).unwrap();
        sim.step();
        assert_eq!(sim.snapshot(sim_core::Viewer::WhiteCell).seats.len(), 2);
        for bad in [
            declared.replacen(r#""side": 1}], "events""#, r#""side": 4}], "events""#, 1),
            declared.replacen(r#""role": "white_cell""#, r#""role": "side", "side": 2"#, 1),
        ] {
            assert!(
                matches!(Scenario::from_json(&bad), Err(ScenarioError::Invalid(_))),
                "{bad}"
            );
        }
    }

    #[test]
    fn events_naming_undeclared_sides_or_units_or_non_reserves_are_rejected() {
        for (from, to) in [
            (r#""side": 1, "text""#, r#""side": 9, "text""#),
            (r#""unit": 2}"#, r#""unit": 1}"#),
            (r#""unit": 2}"#, r#""unit": 7}"#),
            (r#""at_s": 50"#, r#""at_s": -1"#),
            (
                r#""events": ["#,
                r#""injects": [{"id": "i", "title": "t", "to": "side", "side": 5, "text": "x"}], "events": ["#,
            ),
        ] {
            let bad = WITH_EVENTS.replacen(from, to, 1);
            assert!(
                matches!(Scenario::from_json(&bad), Err(ScenarioError::Invalid(_))),
                "{to}"
            );
        }
    }

    /// A scenario as the editor sends it: parsed without the loader's first-error check.
    fn raw(json: &str) -> Scenario {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn check_lists_every_error_at_once_errors_first() {
        let s = raw(r#"{
            "format": "nws-scenario", "format_version": 4, "name": "t", "seed": 1, "duration_s": 0,
            "sides": [{"id": 1, "name": "Blue"}, {"id": 2, "name": "Red"}],
            "minefields": [{"side": 1, "centre": {"lat_deg": 0.0, "lon_deg": 0.0},
                            "radius_m": 1000.0, "mines": 5, "weapon": "WPN-GUN-127"}],
            "units": [
                {"id": 1, "name": "A", "side": 1, "platform": "GEN-FFG",
                 "position": {"lat_deg": 0.0, "lon_deg": 0.0}},
                {"id": 1, "name": "B", "side": 3, "platform": "GEN-FFG",
                 "position": {"lat_deg": 0.0, "lon_deg": 0.1}},
                {"id": 2, "name": "C", "side": 1, "platform": "NO-SUCH",
                 "position": {"lat_deg": 0.0, "lon_deg": 0.2}}
            ]
        }"#);
        let issues = s.check(&db());
        let errors: Vec<&str> = issues
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .map(|i| i.message.as_str())
            .collect();
        for expected in [
            "duplicate unit id",
            "undeclared side",
            "unknown platform NO-SUCH",
            "duration_s",
            "is not a mine",
        ] {
            assert!(
                errors.iter().any(|e| e.contains(expected)),
                "{expected}: {errors:?}"
            );
        }
        // Red has no units: a warning, listed after every error.
        let first_warning = issues
            .iter()
            .position(|i| i.severity == Severity::Warning)
            .unwrap();
        assert!(issues[first_warning..]
            .iter()
            .all(|i| i.severity == Severity::Warning));
        assert!(issues[first_warning].message.contains("Red"));
    }

    #[test]
    fn a_clean_scenario_has_only_warnings_and_builds() {
        // A frigate placed in central Australia, and two units with the same name.
        let s = raw(r#"{
            "format": "nws-scenario", "format_version": 4, "name": "t", "seed": 1, "duration_s": 60,
            "sides": [{"id": 1, "name": "Blue"}],
            "units": [
                {"id": 1, "name": "A", "side": 1, "platform": "GEN-FFG",
                 "position": {"lat_deg": -25.0, "lon_deg": 134.0}},
                {"id": 2, "name": "A", "side": 1, "platform": "GEN-FFG",
                 "position": {"lat_deg": -39.0, "lon_deg": 146.0}}
            ]
        }"#);
        let issues = s.check(&db());
        assert!(
            issues.iter().all(|i| i.severity == Severity::Warning),
            "{issues:?}"
        );
        assert!(issues.iter().any(|i| i.message.contains("starts on land")));
        assert!(issues
            .iter()
            .any(|i| i.message.contains("more than one unit")));
        assert!(s.build(None, &db()).is_ok());
        // The shipped scenarios are clean.
        for name in [
            "phase1-strait-transit",
            "phase2-surface-action",
            "phase3-ttx-demo",
            "phase3-planning-demo",
            "phase3-pilot",
            "phase3-pilot-mindoro",
        ] {
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../data/scenarios")
                .join(format!("{name}.json"));
            let s = Scenario::load(path).unwrap();
            let errors: Vec<_> = s
                .check(&db())
                .into_iter()
                .filter(|i| i.severity == Severity::Error)
                .collect();
            assert!(errors.is_empty(), "{name}: {errors:?}");
        }
    }

    #[test]
    fn the_fuel_model_is_opt_in_and_sized_from_the_database() {
        let json = r#"{
            "format": "nws-scenario", "format_version": 6, "name": "t", "seed": 1,
            "duration_s": 60, "fuel": true,
            "sides": [{"id": 1, "name": "Blue"}],
            "units": [
                {"id": 1, "name": "F", "side": 1, "platform": "GEN-FFG", "fuel_level": 0.4,
                 "position": {"lat_deg": -39.0, "lon_deg": 146.0}},
                {"id": 2, "name": "N", "side": 1, "platform": "GEN-SSN",
                 "position": {"lat_deg": -39.1, "lon_deg": 146.0}}
            ]
        }"#;
        let s = Scenario::from_json(json).unwrap();
        let spawns = s.resolve(&db()).unwrap();
        let f = spawns[0].fuel.unwrap();
        // GEN-FFG: 5 000 t full load, 4 500 nm at 16 kn.
        assert!((f.capacity_t - 500.0).abs() < 1e-9);
        assert!((f.endurance_m - 4_500.0 * 1_852.0).abs() < 1e-6);
        assert!((f.cruise_mps - 16.0 * 1_852.0 / 3_600.0).abs() < 1e-9);
        assert!((f.level - 0.4).abs() < 1e-12);
        assert_eq!(
            spawns[1].fuel, None,
            "no endurance in the database: no fuel model"
        );
        // Off by default, and out-of-range levels are refused.
        let off = Scenario::from_json(&json.replace(r#""fuel": true,"#, "")).unwrap();
        assert!(off.resolve(&db()).unwrap().iter().all(|u| u.fuel.is_none()));
        assert!(matches!(
            Scenario::from_json(&json.replace("0.4", "1.4")),
            Err(ScenarioError::Invalid(_))
        ));
    }

    #[test]
    fn reference_points_load_round_trip_and_are_checked() {
        let json = r#"{
            "format": "nws-scenario", "format_version": 4, "name": "t", "seed": 1, "duration_s": 60,
            "sides": [{"id": 1, "name": "Blue"}],
            "units": [],
            "reference_points": [
                {"name": "RV Alpha", "position": {"lat_deg": -39.0, "lon_deg": 146.0}},
                {"name": "Blue box", "position": {"lat_deg": -39.5, "lon_deg": 146.5}, "side": 1}
            ]
        }"#;
        let s = Scenario::from_json(json).unwrap();
        assert_eq!(s.reference_points.len(), 2);
        assert_eq!(s.reference_points[1].side, Some(SideId(1)));
        let back = Scenario::from_json(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back.reference_points, s.reference_points);
        for (from, to) in [
            (r#""name": "Blue box""#, r#""name": "RV Alpha""#),
            (r#""side": 1}"#, r#""side": 7}"#),
            (r#""name": "RV Alpha""#, r#""name": " ""#),
            (r#""lat_deg": -39.0"#, r#""lat_deg": 99.0"#),
        ] {
            let bad = json.replacen(from, to, 1);
            assert!(
                matches!(Scenario::from_json(&bad), Err(ScenarioError::Invalid(_))),
                "{to}"
            );
        }
    }

    const PLANNED: &str = r#"{
        "format": "nws-scenario", "format_version": 5, "name": "t", "seed": 1, "duration_s": 600,
        "sides": [{"id": 1, "name": "Blue"}, {"id": 2, "name": "Red"}],
        "units": [
            {"id": 1, "name": "Lead", "side": 1, "platform": "GEN-FFG",
             "position": {"lat_deg": -39.0, "lon_deg": 146.0}, "heading_deg": 90.0},
            {"id": 2, "name": "Wing", "side": 1, "platform": "GEN-FFG",
             "position": {"lat_deg": -39.05, "lon_deg": 146.0}},
            {"id": 3, "name": "Red", "side": 2, "platform": "GEN-FFG",
             "position": {"lat_deg": -39.0, "lon_deg": 147.0}}
        ],
        "facilities": [
            {"id": 1, "name": "Blue HQ", "side": 1, "kind": "command_post",
             "position": {"lat_deg": -39.0, "lon_deg": 146.0}, "radius_m": 500.0,
             "range_m": 200000.0},
            {"id": 2, "name": "Haven", "side": 1, "kind": "sheltering_port",
             "position": {"lat_deg": -38.9, "lon_deg": 146.0}, "radius_m": 2000.0,
             "berths": 2, "services": {"repair": true, "rearm": true}}
        ],
        "zones": [
            {"id": 1, "name": "Narrows watch", "side": 1, "kind": "sense",
             "area": {"shape": "circle", "centre": {"lat_deg": -39.0, "lon_deg": 146.8},
                      "radius_m": 20000.0},
             "choke_point": "Narrows"},
            {"id": 2, "name": "Narrows kill box", "side": 1, "kind": "engage",
             "area": {"shape": "circle", "centre": {"lat_deg": -39.0, "lon_deg": 146.8},
                      "radius_m": 8000.0},
             "doctrine": {"wcs": "free"}, "choke_point": "Narrows"}
        ],
        "action_groups": [
            {"name": "SAG 1", "side": 1, "leader": 1,
             "members": [{"unit": 2, "bearing_deg": 180.0, "range_m": 3000.0}],
             "doctrine": {"emcon": "silent"}}
        ]
    }"#;

    #[test]
    fn planning_layer_and_action_groups_load_build_and_hold_station() {
        let s = Scenario::from_json(PLANNED).unwrap();
        assert_eq!(s.facilities.len(), 2);
        assert_eq!(s.zones.len(), 2);
        let back = Scenario::from_json(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back.facilities, s.facilities);
        assert_eq!(back.action_groups, s.action_groups);
        let mut sim = s.build(None, &db()).unwrap();
        sim.run_until(Tick(600));
        let save = sim.save();
        assert_eq!(save.planning.facilities.len(), 2);
        assert_eq!(save.planning.zones.len(), 2);
        // The member was given an escort mission on the leader and has closed on its station,
        // 3 km astern of it.
        let wing = save.units.iter().find(|u| u.unit.id == UnitId(2)).unwrap();
        assert!(matches!(
            wing.mission.mission,
            Some(Mission::Escort { escorted: 1, .. })
        ));
        let lead = save.units.iter().find(|u| u.unit.id == UnitId(1)).unwrap();
        let gap = sim_geo::inverse(lead.position.point, wing.position.point).distance_m;
        assert!((1_000.0..6_000.0).contains(&gap), "{gap}");
        // Both ships are in the group, under its doctrine.
        assert_eq!(wing.unit.group.as_deref(), Some("SAG 1"));
        assert_eq!(lead.unit.group.as_deref(), Some("SAG 1"));
        let blue = sim.snapshot(sim_core::Viewer::Side { side: SideId(1) });
        assert!(blue
            .units
            .iter()
            .filter(|u| u.id.0 <= 2)
            .all(|u| u.doctrine.emcon == sim_core::Emcon::Silent));
        let checked = s.check(&db());
        assert!(
            checked.iter().all(|i| i.severity == Severity::Warning),
            "{checked:?}"
        );
    }

    #[test]
    fn bad_planning_layers_and_action_groups_are_rejected() {
        for (from, to) in [
            // A member in the other side's group, a leader of another side.
            (r#""leader": 1"#, r#""leader": 3"#),
            (r#""unit": 2, "bearing_deg""#, r#""unit": 3, "bearing_deg""#),
            (r#""unit": 2, "bearing_deg""#, r#""unit": 1, "bearing_deg""#),
            (r#""range_m": 3000.0"#, r#""range_m": 0.0"#),
            // Facilities and zones on undeclared sides, duplicate ids, a zone without a size.
            (
                r#""name": "Haven", "side": 1"#,
                r#""name": "Haven", "side": 9"#,
            ),
            (r#""id": 2, "name": "Haven""#, r#""id": 1, "name": "Haven""#),
            (r#""radius_m": 8000.0"#, r#""radius_m": 0.0"#),
            (
                r#""name": "Narrows watch", "side": 1"#,
                r#""name": "Narrows watch", "side": 4"#,
            ),
        ] {
            let bad = PLANNED.replacen(from, to, 1);
            assert!(
                matches!(Scenario::from_json(&bad), Err(ScenarioError::Invalid(_))),
                "{to}"
            );
        }
        // A member with its own mission cannot also keep station.
        let own = PLANNED.replacen(
            r#""name": "Wing", "side": 1,"#,
            r#""name": "Wing", "side": 1, "mission": {"kind": "ferry", "destination": {"lat_deg": -38.0, "lon_deg": 146.0}},"#,
            1,
        );
        assert!(matches!(
            Scenario::from_json(&own),
            Err(ScenarioError::Invalid(m)) if m.contains("own mission")
        ));
    }
}
