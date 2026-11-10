//! Per-viewer state snapshots streamed to clients (UI, multiplayer seats).
//!
//! A side's picture holds its own units and its tracks on other sides' units, each with an
//! estimated position, an error ellipse, a classification and an age. Ground truth for all sides
//! is only ever sent to the white cell.

use crate::components::Domain;
use crate::esm::BearingView;
use crate::ids::{SideId, UnitId};
use crate::time::Tick;
use serde::{Deserialize, Serialize};
use sim_ai::Mission;
use sim_c2::{Doctrine, Posture, ResolvedDoctrine};
use sim_geo::GeoPoint;
use sim_sensors::{Classification, Environment, ErrorEllipse};
use sim_weapons::{Status, System};

/// Whose picture a snapshot shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Viewer {
    /// Ground truth for all sides (umpire only).
    WhiteCell,
    /// The perceived picture of one side.
    Side { side: SideId },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct RouteView {
    pub waypoints: Vec<GeoPoint>,
    pub next: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct UnitView {
    pub id: UnitId,
    pub name: String,
    pub side: SideId,
    pub domain: Domain,
    pub position: GeoPoint,
    pub altitude_m: f64,
    pub heading_deg: f64,
    pub speed_mps: f64,
    pub ordered_speed_mps: f64,
    pub max_speed_mps: f64,
    /// Platform database class, e.g. `GEN-FFG`.
    pub platform: Option<String>,
    /// Present for units the viewer controls.
    pub route: Option<RouteView>,
    pub group: Option<String>,
    /// Doctrine in force (unit, then group, then side, then default).
    pub doctrine: ResolvedDoctrine,
    /// Doctrine set on the unit itself (unset fields are inherited).
    pub doctrine_set: Doctrine,
    pub status: Status,
    /// Damage taken as a fraction of the unit's damage points.
    pub damage_fraction: f64,
    pub criticals: Vec<System>,
    pub fires: u32,
    pub flooding: bool,
    /// Rounds carried, by weapon id (sorted).
    pub stores: Vec<StoreView>,
    pub mission: Option<Mission>,
    pub mission_complete: bool,
}

/// Rounds of one weapon carried.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct StoreView {
    pub weapon: String,
    pub count: u32,
}

/// A missile or torpedo in flight, as shown to its own side (and the white cell).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct WeaponView {
    pub id: u32,
    pub side: SideId,
    pub weapon: String,
    pub position: GeoPoint,
    pub altitude_m: f64,
    pub heading_deg: f64,
    /// The seeker has acquired its target.
    pub homing: bool,
}

/// How `side` regards `toward`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PostureView {
    pub side: SideId,
    pub toward: SideId,
    pub posture: Posture,
}

/// One entry in a side's perceived picture. It deliberately carries no target unit id, so a
/// side's client cannot learn ground truth from the stream (fog of war, design principle 4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ContactView {
    /// The side holding the contact.
    pub side: SideId,
    /// Contact number within that side's picture (C1, C2, …).
    pub number: u32,
    /// Estimated position (not the target's true position).
    pub position: GeoPoint,
    /// One-sigma position error at the snapshot time, grown since the last update.
    pub error: ErrorEllipse,
    pub classification: Classification,
    /// Present once the classification reaches `domain`.
    pub domain: Option<Domain>,
    /// The holding side's posture toward the track's side, once identified to class level.
    pub affiliation: Option<Posture>,
    /// Platform category, once classified to category.
    pub category: Option<String>,
    /// Platform class, once classified to class.
    pub platform: Option<String>,
    pub first_seen_s: f64,
    pub last_seen_s: f64,
    /// Own unit whose sensor last detected it.
    pub detected_by: UnitId,
    pub sensor_id: String,
}

/// A full state snapshot for one viewer. Units are sorted by `id`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Snapshot {
    pub protocol_version: u16,
    pub tick: Tick,
    pub game_time_s: f64,
    pub viewer: Viewer,
    pub units: Vec<UnitView>,
    /// Side viewer: that side's contacts. White cell: every side's contacts.
    pub contacts: Vec<ContactView>,
    /// Side viewer: that side's postures toward the others. White cell: every posture set.
    pub postures: Vec<PostureView>,
    /// Sea state and weather, known to every side.
    pub environment: Environment,
    /// Side viewer: that side's recent ESM bearing lines. White cell: every side's.
    pub bearings: Vec<BearingView>,
    /// Side viewer: its own weapons in flight. White cell: every weapon in flight.
    pub weapons: Vec<WeaponView>,
}
