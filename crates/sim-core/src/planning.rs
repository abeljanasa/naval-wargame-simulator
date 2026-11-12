//! Operational planning layer (master prompt §7.1; `docs/models/planning-layer.md`).
//!
//! - **Facilities** of a side:
//!   - command posts, ashore or carried by a unit, with subordinates and an alternate that takes
//!     over after a handover delay when the post is lost (orders in the gap are lost or queued);
//!   - refuelling points, sheltering ports and anchorages: repair and rearm for units stopped
//!     alongside, with berth limits; mission-killed units on a mission divert to the nearest port
//!     that repairs, and resume the mission when repaired.
//! - **Zones** of a side, often in pairs at a choke point:
//!   - a *sense* zone sets the doctrine of the side's own units inside it and raises an alert
//!     when the side gains a contact inside it;
//!   - an *engage* zone sets the weapons control and ROE for engaging targets inside it, and its
//!     assigned shooters engage contacts inside it;
//!   - other sides' units entering a choke point are logged.

use crate::ai::{engage_first_weapon, ensure_moving, set_route, MissionState};
use crate::c2::C2State;
use crate::combat::{Health, Magazine, WeaponsInFlight};
use crate::command::{Command, Issuer};
use crate::comms;
use crate::components::{Kinematics, Position, Route, Unit};
use crate::detection::{ContactTable, SensorSuite};
use crate::events::{EventLog, SimEventKind};
use crate::hash::StateHash;
use crate::ids::{SideId, UnitId};
use crate::scenario_events::{Message, MessageLog};
use crate::sim::{apply_command, UnitIndex};
use crate::time::{SimClock, Tick};
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use sim_ai::{Mission, AI_PERIOD_TICKS};
use sim_c2::comms::LinkStatus;
use sim_c2::{Doctrine, ResolvedDoctrine};
use sim_events::{Area, Audience};
use sim_geo::GeoPoint;
use sim_weapons::Status;
use std::collections::{BTreeMap, BTreeSet};

/// How often facilities serve the units alongside.
pub const SERVICE_PERIOD_TICKS: u64 = 60;
/// A unit counts as alongside (or at anchor) at or below this speed.
pub const ALONGSIDE_SPEED_MPS: f64 = 0.5;
/// Repair removes this fraction of the unit's damage points per minute alongside.
pub const REPAIR_FRACTION_PER_MIN: f64 = 0.02;
/// Time alongside before a unit is rearmed.
pub const REARM_TICKS: u64 = 1_800;
/// Default delay before an alternate command post takes over, seconds.
pub const DEFAULT_HANDOVER_S: f64 = 300.0;
/// Default fuel transfer rate of a refuelling point, tonnes per hour.
pub const DEFAULT_TRANSFER_T_PER_H: f64 = 300.0;
/// A unit refuelling from an oiler under way steers for a point this far ahead of it, metres.
pub const RAS_LEAD_M: f64 = 2_000.0;
/// Water a unit needs under its keel, metres.
pub const UNDER_KEEL_CLEARANCE_M: f64 = 1.0;

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

/// What happens to a player's order issued while a command post is down and its alternate has
/// not yet taken over.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum GapPolicy {
    /// Refused.
    #[default]
    Lose,
    /// Held, and carried out when the alternate takes over (if the unit is then in its range).
    Queue,
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
    /// Refuels ships and submarines that carry a fuel model (`docs/models/fuel.md`).
    #[serde(default)]
    pub refuel: bool,
}

fn default_handover() -> f64 {
    DEFAULT_HANDOVER_S
}

/// A planning-layer facility.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Facility {
    pub id: u32,
    pub name: String,
    pub side: SideId,
    pub kind: FacilityKind,
    /// Where it is (for a command post carried by a unit: where it starts; it then moves with
    /// the unit).
    pub position: GeoPoint,
    /// Units within this distance are alongside (ports, anchorages) or at the post.
    pub radius_m: f64,
    /// Command posts: orders reach subordinates within this range (default: `radius_m`).
    #[serde(default)]
    pub range_m: Option<f64>,
    /// Units served at once (in unit-id order); 0 = no limit.
    #[serde(default)]
    pub berths: u32,
    #[serde(default)]
    pub services: Services,
    /// Command posts: the unit carrying the post (a flagship, an aircraft, a land unit). The
    /// post is down once the unit is lost.
    #[serde(default)]
    pub unit: Option<UnitId>,
    /// Command posts: the units it commands; empty = every unit of its side.
    #[serde(default)]
    pub subordinates: Vec<UnitId>,
    /// Command posts: the post that takes over this post's subordinates when it is down.
    #[serde(default)]
    pub alternate: Option<u32>,
    /// Command posts: seconds from going down to the alternate taking over.
    #[serde(default = "default_handover")]
    pub handover_s: f64,
    /// Command posts: orders issued in the gap are lost or queued.
    #[serde(default)]
    pub orders_in_gap: GapPolicy,
    /// Command posts: the link is VHF/UHF only, so limited by the radio horizon. Default: beyond
    /// line of sight (HF or satellite).
    #[serde(default)]
    pub line_of_sight: bool,
    /// Command posts ashore: antenna height, m (default 30).
    #[serde(default)]
    pub antenna_m: Option<f64>,
    /// Command posts: the link's anti-jam margin, dB (default 10).
    #[serde(default)]
    pub anti_jam_db: Option<f64>,
    /// Command posts: units of its side that relay its traffic when the direct link fails.
    #[serde(default)]
    pub relays: Vec<UnitId>,
    /// Refuelling: tonnes per hour transferred to one unit (default 300).
    #[serde(default)]
    pub transfer_t_per_h: Option<f64>,
    /// Refuelling: tonnes held at the start; `None` = unlimited (a depot ashore).
    #[serde(default)]
    pub fuel_t: Option<f64>,
    /// The facility's water (ports, anchorages, fixed points): a polygon or circle used instead of
    /// `radius_m` round `position`, which stays the point units steer for.
    #[serde(default)]
    pub area: Option<Area>,
    /// Depth of water, metres: units whose draft plus 1 m clearance exceeds it cannot use it.
    #[serde(default)]
    pub depth_m: Option<f64>,
    /// Holding limit: above this sea state units do not stay or come here.
    #[serde(default)]
    pub max_sea_state: Option<u8>,
    /// Anchorages: AI-run units whose mission is complete hold here until tasked again.
    #[serde(default)]
    pub staging: bool,
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
    /// The choke point this zone belongs to, if any.
    #[serde(default)]
    pub choke_point: Option<String>,
    /// Engage zones: units that engage contacts inside the zone on their own (one round per
    /// contact per AI period, under the zone's rules).
    #[serde(default)]
    pub shooters: Vec<UnitId>,
    /// The zone's layer in a choke point's defence (outer, middle, inner).
    #[serde(default)]
    pub layer: Option<ZoneLayer>,
    /// Sense zones: the sensor types this layer is for; coverage counts only these (empty: all).
    #[serde(default)]
    pub sensors: Vec<crate::detection::SensorKind>,
}

/// A zone's layer in a choke point's defence: sense far out, engage in the inner layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum ZoneLayer {
    Outer,
    Middle,
    Inner,
}

/// A unit being served at a facility.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServiceState {
    pub facility: u32,
    pub since: Tick,
    pub rearmed: bool,
    /// Filled up on this visit (logged once; topping up continues).
    #[serde(default)]
    pub refuelled: bool,
}

/// A unit sent to a facility, with the mission it resumes once repaired or refuelled.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Diversion {
    pub facility: u32,
    pub mission: Option<Mission>,
    #[serde(default)]
    pub reason: DivertReason,
}

/// A player's order held while its command post is down.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueuedOrder {
    pub release: Tick,
    pub seq: u64,
    pub command: Command,
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
    /// Command posts that are down, and since when.
    #[serde(default)]
    pub down: BTreeMap<u32, Tick>,
    /// Posts down while jammed or with comms knocked out (they come back); lost posts are not here.
    #[serde(default)]
    pub disrupted: BTreeMap<u32, LinkStatus>,
    /// Command posts carried by a unit that has been in play (only those can be lost).
    #[serde(default)]
    pub manned: BTreeSet<u32>,
    /// Orders held for a command post's alternate.
    #[serde(default)]
    pub queued: Vec<QueuedOrder>,
    /// Units on their way to (or at) a port for repair.
    #[serde(default)]
    pub diverted: BTreeMap<UnitId, Diversion>,
    /// (choke point, unit): other sides' units inside a choke point now.
    #[serde(default)]
    pub inside: BTreeSet<(String, UnitId)>,
    /// Tonnes of fuel each facility has issued.
    #[serde(default)]
    pub issued: BTreeMap<u32, f64>,
    /// Units waiting for a berth, and where.
    #[serde(default)]
    pub waiting: BTreeMap<UnitId, u32>,
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
        if f.antenna_m.is_some_and(|h| !(h.is_finite() && h >= 0.0))
            || f.anti_jam_db.is_some_and(|a| !a.is_finite())
        {
            return Err(format!(
                "facility {:?} has an invalid antenna height or anti-jam margin",
                f.name
            ));
        }
        if f.unit.is_some()
            && !matches!(
                f.kind,
                FacilityKind::CommandPost | FacilityKind::RefuellingPoint
            )
        {
            return Err(format!(
                "facility {:?}: only command posts and refuelling points are carried by a unit",
                f.name
            ));
        }
        if let Some(a) = &f.area {
            a.validate()
                .map_err(|e| format!("facility {:?} area: {e}", f.name))?;
            if f.unit.is_some() {
                return Err(format!(
                    "facility {:?}: a facility carried by a unit has no fixed area",
                    f.name
                ));
            }
        }
        if f.depth_m.is_some_and(|d| !(d.is_finite() && d > 0.0))
            || f.max_sea_state.is_some_and(|s| s > 9)
        {
            return Err(format!(
                "facility {:?} needs a depth > 0 and a holding limit of sea state 0-9",
                f.name
            ));
        }
        if f.staging && f.kind != FacilityKind::Anchorage {
            return Err(format!(
                "facility {:?}: only anchorages are staging areas",
                f.name
            ));
        }
        if f.transfer_t_per_h
            .is_some_and(|r| !(r.is_finite() && r > 0.0))
            || f.fuel_t.is_some_and(|t| !(t.is_finite() && t >= 0.0))
        {
            return Err(format!(
                "facility {:?} needs a transfer rate > 0 and fuel >= 0",
                f.name
            ));
        }
        if !f.relays.is_empty() && f.kind != FacilityKind::CommandPost {
            return Err(format!(
                "facility {:?}: only command posts have relays",
                f.name
            ));
        }
        if !(f.handover_s.is_finite() && f.handover_s >= 0.0) {
            return Err(format!(
                "facility {:?} has a negative handover delay",
                f.name
            ));
        }
    }
    for f in facilities {
        if let Some(a) = f.alternate {
            let ok = facilities.iter().any(|g| {
                g.id == a && g.id != f.id && g.side == f.side && g.kind == FacilityKind::CommandPost
            });
            if f.kind != FacilityKind::CommandPost || !ok {
                return Err(format!(
                    "facility {:?}: an alternate must be another command post of the same side",
                    f.name
                ));
            }
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
        if z.kind == ZoneKind::Engage && !z.sensors.is_empty() {
            return Err(format!(
                "zone {:?}: only sense zones list sensor types",
                z.name
            ));
        }
        if z.kind == ZoneKind::Sense && !z.shooters.is_empty() {
            return Err(format!(
                "zone {:?}: only engage zones have shooters",
                z.name
            ));
        }
    }
    Ok(())
}

impl PlanningLayer {
    /// Replaces the facilities and zones (sorted by id). Service, diversions and queued orders
    /// at facilities that still exist carry on.
    pub fn set(&mut self, mut facilities: Vec<Facility>, mut zones: Vec<Zone>) {
        facilities.sort_by_key(|f| f.id);
        zones.sort_by_key(|z| z.id);
        let kept: BTreeSet<u32> = facilities.iter().map(|f| f.id).collect();
        self.service.retain(|_, s| kept.contains(&s.facility));
        self.diverted.retain(|_, d| kept.contains(&d.facility));
        self.waiting.retain(|_, f| kept.contains(f));
        self.down.retain(|f, _| kept.contains(f));
        self.disrupted.retain(|f, _| kept.contains(f));
        self.manned.retain(|f| kept.contains(f));
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

    /// Fuel a refuelling point has left; `None` = unlimited.
    #[must_use]
    pub fn fuel_left(&self, f: &Facility) -> Option<f64> {
        f.fuel_t
            .map(|t| (t - self.issued.get(&f.id).copied().unwrap_or(0.0)).max(0.0))
    }

    fn facility(&self, id: u32) -> Option<&Facility> {
        self.facilities.iter().find(|f| f.id == id)
    }

    /// Adds the layer to the state hash (nothing when empty, so scenarios without one keep
    /// their hashes).
    pub(crate) fn hash(&self, h: &mut StateHash) {
        if *self == Self::default() {
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

fn position_of(world: &World, unit: UnitId) -> Option<GeoPoint> {
    world
        .resource::<UnitIndex>()
        .0
        .get(&unit)
        .and_then(|e| world.get::<Position>(*e))
        .map(|p| p.point)
}

/// Where a command post is now (`None`: carried by a unit that has been lost).
fn post_position(world: &World, f: &Facility) -> Option<GeoPoint> {
    match f.unit {
        Some(u) => position_of(world, u),
        None => Some(f.position),
    }
}

/// When the post went down (`None`: it is up).
fn down_since(world: &World, layer: &PlanningLayer, f: &Facility, now: Tick) -> Option<Tick> {
    if let Some(t) = layer.down.get(&f.id) {
        return Some(*t);
    }
    (post_position(world, f).is_none()).then_some(now)
}

fn clock(t: Tick) -> String {
    let s = t.0;
    format!("{:02}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn handover_tick(since: Tick, f: &Facility) -> Tick {
    // `handover_s` is checked finite and ≥ 0.
    Tick(since.0 + f.handover_s.ceil() as u64)
}

/// Whether a command may go ahead under the side's command posts.
pub(crate) enum Authority {
    Ok,
    Refused(String),
    Queued { release: Tick, reason: String },
}

/// The posts that can command `unit` now: each post that commands it and is up, or the
/// alternate of one that is down once the handover delay has passed. Also the earliest pending
/// handover, and whether every commanding post is down. `None`: no post of the side commands it.
#[allow(clippy::type_complexity)]
fn acting_posts<'a>(
    world: &World,
    layer: &'a PlanningLayer,
    side: SideId,
    unit: UnitId,
    now: Tick,
) -> Option<(
    Vec<&'a Facility>,
    Option<(Tick, &'a Facility, &'a Facility)>,
    bool,
)> {
    let commanding: Vec<&Facility> = layer
        .facilities
        .iter()
        .filter(|f| f.side == side && f.kind == FacilityKind::CommandPost)
        .filter(|f| f.subordinates.is_empty() || f.subordinates.contains(&unit))
        .collect();
    if commanding.is_empty() {
        return None;
    }
    let mut acting = Vec::new();
    let mut pending: Option<(Tick, &Facility, &Facility)> = None;
    for p in &commanding {
        match down_since(world, layer, p, now) {
            None => acting.push(*p),
            Some(since) => {
                let Some(alt) = p.alternate.and_then(|a| layer.facility(a)) else {
                    continue;
                };
                if down_since(world, layer, alt, now).is_some() {
                    continue;
                }
                let at_tick = handover_tick(since, p);
                if now >= at_tick {
                    acting.push(alt);
                } else if pending.is_none_or(|(t, _, _)| at_tick < t) {
                    pending = Some((at_tick, p, alt));
                }
            }
        }
    }
    let all_down = commanding
        .iter()
        .all(|p| down_since(world, layer, p, now).is_some());
    Some((acting, pending, all_down))
}

/// Command-post authority for a player's order to a unit: a post that commands it and is up (or
/// that post's alternate once the handover delay has passed) must have a comms link to it
/// (`docs/models/comms.md`). Sides without command posts, and orders not from a player, are not
/// limited.
pub(crate) fn command_authority(world: &World, cmd: &Command) -> Authority {
    if !matches!(cmd.issuer, Issuer::Player { .. }) {
        return Authority::Ok;
    }
    let (Some(side), Some(unit)) = (cmd.side, cmd.payload.target_unit()) else {
        return Authority::Ok;
    };
    let layer = world.resource::<PlanningLayer>();
    let has_posts = layer
        .facilities
        .iter()
        .any(|f| f.side == side && f.kind == FacilityKind::CommandPost);
    if !has_posts || position_of(world, unit).is_none() {
        return Authority::Ok; // an unknown unit is reported by the order itself
    }
    let now = world.resource::<SimClock>().tick;
    let Some((acting, pending, all_down)) = acting_posts(world, layer, side, unit, now) else {
        return Authority::Refused(format!("{unit} is not assigned to a command post"));
    };
    let jammers = comms::jammers(world);
    let mut failure: Option<(LinkStatus, &Facility)> = None;
    for p in acting {
        let (status, _) = comms::link(world, &jammers, p, unit);
        if status == LinkStatus::Up {
            return Authority::Ok;
        }
        if failure.is_none() {
            failure = Some((status, p));
        }
    }
    if let Some((release, p, alt)) = pending {
        let reason = format!(
            "{} is down; command passes to {} at {}",
            p.name,
            alt.name,
            clock(release)
        );
        return match p.orders_in_gap {
            GapPolicy::Queue => Authority::Queued { release, reason },
            GapPolicy::Lose => Authority::Refused(reason),
        };
    }
    if let Some((status, p)) = failure {
        return Authority::Refused(format!(
            "{unit} cannot be reached from {}: {}",
            p.name,
            status.describe()
        ));
    }
    if all_down {
        return Authority::Refused(format!("{unit}'s command posts are down"));
    }
    Authority::Refused(format!("{unit} cannot be reached from any command post"))
}

/// The comms of each of `side`'s units, when the side has command posts (unit-id order).
pub(crate) fn comms_views(world: &World, side: SideId) -> Vec<comms::CommsView> {
    let layer = world.resource::<PlanningLayer>();
    if !layer
        .facilities
        .iter()
        .any(|f| f.side == side && f.kind == FacilityKind::CommandPost)
    {
        return Vec::new();
    }
    let now = world.resource::<SimClock>().tick;
    let jammers = comms::jammers(world);
    let units: Vec<UnitId> = world
        .resource::<UnitIndex>()
        .0
        .iter()
        .filter(|(_, e)| world.get::<Unit>(**e).is_some_and(|u| u.side == side))
        .map(|(id, _)| *id)
        .collect();
    units
        .into_iter()
        .map(|unit| {
            let mut view = comms::CommsView {
                unit,
                status: LinkStatus::NoPost,
                post: None,
                via: None,
            };
            let Some((acting, _, _)) = acting_posts(world, layer, side, unit, now) else {
                return view;
            };
            for p in acting {
                let (status, via) = comms::link(world, &jammers, p, unit);
                if view.post.is_none() || status == LinkStatus::Up {
                    view = comms::CommsView {
                        unit,
                        status,
                        post: Some(p.id),
                        via,
                    };
                }
                if status == LinkStatus::Up {
                    break;
                }
            }
            view
        })
        .collect()
}

/// Command posts of `side` (every side: `None`) and whether each is up.
pub(crate) fn post_views(world: &World, side: Option<SideId>) -> Vec<comms::PostView> {
    let layer = world.resource::<PlanningLayer>();
    let now = world.resource::<SimClock>().tick;
    layer
        .facilities
        .iter()
        .filter(|f| f.kind == FacilityKind::CommandPost && side.is_none_or(|s| s == f.side))
        .map(|f| {
            let since = down_since(world, layer, f, now);
            comms::PostView {
                facility: f.id,
                up: since.is_none(),
                status: match since {
                    None => LinkStatus::Up,
                    Some(_) => layer
                        .disrupted
                        .get(&f.id)
                        .copied()
                        .unwrap_or(LinkStatus::NoPost),
                },
                since_s: since.map(Tick::seconds),
            }
        })
        .collect()
}

/// Each tick: command posts lost with their units, queued orders, and alerts for contacts gained
/// in sense zones. Every AI period: engage-zone shooters and choke-point transits. Every
/// [`SERVICE_PERIOD_TICKS`]: diversions to port, repair and rearming.
pub(crate) fn planning_system(world: &mut World) {
    {
        let layer = world.resource::<PlanningLayer>();
        if layer.facilities.is_empty() && layer.zones.is_empty() && layer.queued.is_empty() {
            return;
        }
    }
    let tick = world.resource::<SimClock>().tick;
    posts_down(world, tick);
    release_queued(world, tick);
    sense_alerts(world, tick);
    if tick.0.is_multiple_of(AI_PERIOD_TICKS) {
        zone_shooters(world, tick);
        choke_transits(world, tick);
    }
    if tick.0.is_multiple_of(SERVICE_PERIOD_TICKS) {
        divert(world, tick);
        serve(world, tick);
    }
}

fn posts_down(world: &mut World, tick: Tick) {
    let (manned, lost): (Vec<u32>, Vec<u32>) = {
        let layer = world.resource::<PlanningLayer>();
        let carried = layer
            .facilities
            .iter()
            .filter(|f| f.kind == FacilityKind::CommandPost && f.unit.is_some());
        let manned = carried
            .clone()
            .filter(|f| !layer.manned.contains(&f.id) && post_position(world, f).is_some())
            .map(|f| f.id)
            .collect();
        let lost = carried
            .filter(|f| layer.manned.contains(&f.id) && !layer.down.contains_key(&f.id))
            .filter(|f| post_position(world, f).is_none())
            .map(|f| f.id)
            .collect();
        (manned, lost)
    };
    world.resource_mut::<PlanningLayer>().manned.extend(manned);
    for id in lost {
        let mut layer = world.resource_mut::<PlanningLayer>();
        layer.disrupted.remove(&id);
        layer.down.entry(id).or_insert(tick);
        world
            .resource_mut::<EventLog>()
            .push(tick, SimEventKind::CommandPostDown { facility: id });
    }
    // Jamming and comms criticals: posts go down and come back.
    let jammers = comms::jammers(world);
    let changes: Vec<(u32, Option<LinkStatus>)> = {
        let layer = world.resource::<PlanningLayer>();
        layer
            .facilities
            .iter()
            .filter(|f| f.kind == FacilityKind::CommandPost)
            .filter(|f| post_position(world, f).is_some())
            .filter_map(|f| {
                let now = comms::disruption(world, &jammers, f);
                let was = layer.disrupted.get(&f.id).copied();
                (now != was).then_some((f.id, now))
            })
            .collect()
    };
    for (id, now) in changes {
        let mut layer = world.resource_mut::<PlanningLayer>();
        let kind = match now {
            Some(status) => {
                let first = layer.disrupted.insert(id, status).is_none();
                layer.down.entry(id).or_insert(tick);
                if !first {
                    continue; // still down, now for another reason
                }
                SimEventKind::CommandPostDisrupted {
                    facility: id,
                    status,
                }
            }
            None => {
                layer.disrupted.remove(&id);
                layer.down.remove(&id);
                SimEventKind::CommandPostRestored { facility: id }
            }
        };
        world.resource_mut::<EventLog>().push(tick, kind);
    }
}

fn release_queued(world: &mut World, tick: Tick) {
    let due: Vec<QueuedOrder> = {
        let mut layer = world.resource_mut::<PlanningLayer>();
        let (due, keep): (Vec<_>, Vec<_>) = std::mem::take(&mut layer.queued)
            .into_iter()
            .partition(|q| q.release <= tick);
        layer.queued = keep;
        due
    };
    for q in due {
        let outcome = match command_authority(world, &q.command) {
            Authority::Ok => apply_command(world, &q.command),
            Authority::Refused(r) | Authority::Queued { reason: r, .. } => Err(r),
        };
        let kind = match outcome {
            Ok(()) => SimEventKind::QueuedOrderApplied { seq: q.seq },
            Err(reason) => SimEventKind::QueuedOrderDropped { seq: q.seq, reason },
        };
        world.resource_mut::<EventLog>().push(tick, kind);
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
        let Some(at) = position_of(world, target) else {
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

/// Engage-zone shooters: for each engage zone (id order), each of its side's contacts believed
/// inside and not already under attack (contact-number order) is engaged by the first assigned
/// shooter that can, one round, at most once per shooter per period.
fn zone_shooters(world: &mut World, tick: Tick) {
    let zones: Vec<Zone> = world
        .resource::<PlanningLayer>()
        .zones
        .iter()
        .filter(|z| z.kind == ZoneKind::Engage && !z.shooters.is_empty())
        .cloned()
        .collect();
    let mut fired: BTreeSet<UnitId> = BTreeSet::new();
    for z in zones {
        let under_attack: BTreeSet<u32> = world
            .resource::<WeaponsInFlight>()
            .list
            .iter()
            .filter(|w| w.side == z.side)
            .map(|w| w.contact)
            .collect();
        let mut contacts: Vec<u32> = world
            .resource::<ContactTable>()
            .iter()
            .filter(|(s, c)| {
                *s == z.side
                    && !under_attack.contains(&c.number)
                    && z.area.contains(c.estimate_at(tick).position)
            })
            .map(|(_, c)| c.number)
            .collect();
        contacts.sort_unstable();
        for contact in contacts {
            for &shooter in &z.shooters {
                if fired.contains(&shooter) {
                    continue;
                }
                let Some(e) = world.resource::<UnitIndex>().0.get(&shooter).copied() else {
                    continue;
                };
                let ready = world.get::<Unit>(e).is_some_and(|u| u.side == z.side)
                    && world
                        .get::<Health>(e)
                        .is_some_and(|h| h.status == Status::Operational);
                if !ready {
                    continue;
                }
                if let Some(weapon) = engage_first_weapon(world, shooter, e, z.side, contact, tick)
                {
                    fired.insert(shooter);
                    world.resource_mut::<EventLog>().push(
                        tick,
                        SimEventKind::ZoneEngage {
                            zone: z.id,
                            unit: shooter,
                            contact,
                            weapon,
                        },
                    );
                    break;
                }
            }
        }
    }
}

/// Logs other sides' units entering a choke point (any zone carrying its name).
fn choke_transits(world: &mut World, tick: Tick) {
    let mut chokes: BTreeMap<String, (SideId, Vec<Area>)> = BTreeMap::new();
    for z in &world.resource::<PlanningLayer>().zones {
        if let Some(name) = &z.choke_point {
            chokes
                .entry(name.clone())
                .or_insert_with(|| (z.side, Vec::new()))
                .1
                .push(z.area.clone());
        }
    }
    if chokes.is_empty() {
        return;
    }
    let mut units: Vec<(UnitId, SideId, GeoPoint)> = world
        .query::<(&Unit, &Position)>()
        .iter(world)
        .map(|(u, p)| (u.id, u.side, p.point))
        .collect();
    units.sort_by_key(|u| u.0);
    let mut now_inside = BTreeSet::new();
    for (name, (owner, areas)) in &chokes {
        for &(id, side, at) in &units {
            if side != *owner && areas.iter().any(|a| a.contains(at)) {
                now_inside.insert((name.clone(), id));
            }
        }
    }
    let entered: Vec<(String, UnitId)> = {
        let layer = world.resource::<PlanningLayer>();
        now_inside.difference(&layer.inside).cloned().collect()
    };
    world.resource_mut::<PlanningLayer>().inside = now_inside;
    for (choke_point, unit) in entered {
        world
            .resource_mut::<EventLog>()
            .push(tick, SimEventKind::ChokePointTransit { choke_point, unit });
    }
}

/// Why a unit was sent to a facility.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DivertReason {
    /// Mission-killed: repair, then resume.
    #[default]
    Repair,
    /// Below bingo fuel: refuel, then resume.
    Fuel,
    /// Sea state at or above the unit's limit: shelter in port until it eases, then resume.
    Weather,
    /// Mission complete: hold at a staging anchorage until given a new mission.
    Staging,
}

/// The sea state at which a surface unit of this length runs for shelter (original design:
/// small craft at 5, medium ships at 7, large ships at 8).
#[must_use]
pub fn weather_limit(length_m: f64) -> u8 {
    if length_m < 60.0 {
        5
    } else if length_m < 120.0 {
        7
    } else {
        8
    }
}

/// Whether `p` is in the facility: inside its area, or within its radius of where it is now
/// (`at`; a carried facility always uses its radius).
fn inside(f: &Facility, at: GeoPoint, p: GeoPoint) -> bool {
    match (&f.area, f.unit) {
        (Some(a), None) => a.contains(p),
        _ => sim_geo::inverse(at, p).distance_m <= f.radius_m,
    }
}

/// Whether a unit of this draft can use the facility: draft plus under-keel clearance within its
/// depth. Unknown draft or depth always fits.
fn fits(f: &Facility, draft_m: Option<f64>) -> bool {
    match (f.depth_m, draft_m) {
        (Some(depth), Some(d)) => d + UNDER_KEEL_CLEARANCE_M <= depth,
        _ => true,
    }
}

/// Whether the sea state exceeds the facility's holding limit.
fn over_limit(f: &Facility, sea_state: u8) -> bool {
    f.max_sea_state.is_some_and(|m| sea_state > m)
}

/// Units diverted, with their facility (where it is now) and its radius, for holding alongside.
fn diverted_now(world: &World) -> Vec<(UnitId, Entity, Facility, GeoPoint)> {
    let layer = world.resource::<PlanningLayer>();
    layer
        .diverted
        .iter()
        .filter_map(|(id, d)| {
            let f = layer.facility(d.facility)?.clone();
            let at = post_position(world, &f)?;
            let e = *world.resource::<UnitIndex>().0.get(id)?;
            Some((*id, e, f, at))
        })
        .collect()
}

/// A surface unit at or above its weather limit.
fn threatened(world: &World, e: Entity, sea_state: u8) -> bool {
    let (Some(u), Some(sig)) = (
        world.get::<Unit>(e),
        world.get::<crate::detection::Signature>(e),
    ) else {
        return false;
    };
    let length = sig
        .length_m
        .unwrap_or_else(|| crate::optical::default_length_m(u.domain));
    u.domain == crate::components::Domain::Surface && sea_state >= weather_limit(length)
}

/// Every [`SERVICE_PERIOD_TICKS`], AI-run units are sent to a facility of their side
/// (`docs/models/planning-layer.md`, `docs/models/fuel.md`), in this order:
/// - mission-killed units on a mission: the nearest port that repairs;
/// - units on a mission below bingo fuel: the nearest refuelling point with fuel left;
/// - surface units on a mission at or above their weather limit: the nearest sheltering port;
/// - units whose mission is complete: the nearest staging anchorage.
///
/// Only facilities they fit (draft) and that are within their holding limit count. Units stop
/// alongside (or keep company with an oiler) and resume their mission once repaired, full, or
/// the weather eases; units holding at an anchorage stay until given a new mission.
fn divert(world: &mut World, tick: Tick) {
    let sea = world.resource::<crate::detection::EnvState>().0.sea_state;
    weather_changes(world, tick, sea);
    for reason in [
        DivertReason::Repair,
        DivertReason::Fuel,
        DivertReason::Weather,
        DivertReason::Staging,
    ] {
        divert_for(world, tick, reason, sea);
    }
    hold_alongside(world, tick);
}

/// Weather eased: units sheltering resume. Weather worsened: units holding at a staging
/// anchorage above its holding limit (or above their own limit) leave it, to shelter.
fn weather_changes(world: &mut World, tick: Tick, sea: u8) {
    let diverted: Vec<(UnitId, Entity, DivertReason, Option<Facility>)> = {
        let layer = world.resource::<PlanningLayer>();
        layer
            .diverted
            .iter()
            .filter_map(|(id, d)| {
                let e = *world.resource::<UnitIndex>().0.get(id)?;
                Some((*id, e, d.reason, layer.facility(d.facility).cloned()))
            })
            .collect()
    };
    for (id, e, reason, f) in diverted {
        match reason {
            DivertReason::Weather if !threatened(world, e, sea) => {
                resume(world, id, e, DivertReason::Weather);
                world
                    .resource_mut::<EventLog>()
                    .push(tick, SimEventKind::WeatherEased { unit: id });
            }
            DivertReason::Staging
                if threatened(world, e, sea) || f.as_ref().is_some_and(|f| over_limit(f, sea)) =>
            {
                world.resource_mut::<PlanningLayer>().diverted.remove(&id);
            }
            _ => {}
        }
    }
}

fn divert_for(world: &mut World, tick: Tick, reason: DivertReason, sea: u8) {
    let points: Vec<(Facility, GeoPoint)> = {
        let layer = world.resource::<PlanningLayer>();
        layer
            .facilities
            .iter()
            .filter(|f| f.kind != FacilityKind::CommandPost && !over_limit(f, sea))
            .filter(|f| match reason {
                DivertReason::Repair => f.services.repair,
                DivertReason::Fuel => f.services.refuel && layer.fuel_left(f) != Some(0.0),
                DivertReason::Weather => f.kind == FacilityKind::ShelteringPort,
                DivertReason::Staging => f.kind == FacilityKind::Anchorage && f.staging,
            })
            .filter_map(|f| post_position(world, f).map(|p| (f.clone(), p)))
            .collect()
    };
    if points.is_empty() {
        return;
    }
    let mut candidates: Vec<(UnitId, Entity, SideId, GeoPoint, Option<f64>)> = world
        .query::<(
            Entity,
            &Unit,
            &Position,
            &Health,
            &MissionState,
            Option<&crate::fuel::Fuel>,
        )>()
        .iter(world)
        .filter(|(_, _, _, h, m, fuel)| {
            m.mission.is_some()
                && match reason {
                    DivertReason::Repair => h.status == Status::MissionKilled,
                    DivertReason::Fuel => {
                        h.status == Status::Operational && fuel.is_some_and(|f| f.low())
                    }
                    DivertReason::Weather => true,
                    DivertReason::Staging => h.status == Status::Operational && m.complete,
                }
        })
        .map(|(e, u, p, ..)| (u.id, e, u.side, p.point, u.draft_m))
        .collect();
    candidates.sort_by_key(|c| c.0);
    for (id, e, side, at, draft) in candidates {
        if world.resource::<PlanningLayer>().diverted.contains_key(&id) {
            continue;
        }
        if reason == DivertReason::Weather && !threatened(world, e, sea) {
            continue;
        }
        let Some((point, to)) = points
            .iter()
            .filter(|(f, _)| f.side == side && f.unit != Some(id) && fits(f, draft))
            .min_by(|(a, pa), (b, pb)| {
                let da = sim_geo::inverse(at, *pa).distance_m;
                let db = sim_geo::inverse(at, *pb).distance_m;
                da.total_cmp(&db).then(a.id.cmp(&b.id))
            })
            .cloned()
        else {
            continue;
        };
        // A completed mission stays where it is (so the unit can stage); a live one is set aside.
        let mission = world.get_mut::<MissionState>(e).and_then(|mut m| {
            if m.complete {
                None
            } else {
                m.mission.take()
            }
        });
        world.resource_mut::<PlanningLayer>().diverted.insert(
            id,
            Diversion {
                facility: point.id,
                mission,
                reason,
            },
        );
        let already_there = inside(&point, to, at);
        if !already_there {
            set_route(world, e, id, to, tick);
            ensure_moving(world, e, 0.7);
        }
        let facility = point.id;
        let kind = match reason {
            DivertReason::Repair => SimEventKind::DivertedToPort { unit: id, facility },
            DivertReason::Fuel => SimEventKind::DivertedForFuel { unit: id, facility },
            DivertReason::Weather => SimEventKind::ShelteringFromWeather { unit: id, facility },
            DivertReason::Staging => SimEventKind::HoldingAtAnchorage { unit: id, facility },
        };
        world.resource_mut::<EventLog>().push(tick, kind);
    }
}

/// Diverted units that have arrived stop alongside a fixed facility, or keep company with an
/// oiler: its speed, steering for a point 2 km ahead of it. Those not there yet steer for it.
fn hold_alongside(world: &mut World, tick: Tick) {
    for (id, e, f, at) in diverted_now(world) {
        let Some(here) = world.get::<Position>(e).map(|p| p.point) else {
            continue;
        };
        let near = inside(&f, at, here);
        let oiler = f
            .unit
            .and_then(|u| world.resource::<UnitIndex>().0.get(&u).copied())
            .and_then(|o| world.get::<Kinematics>(o).copied());
        match (near, oiler) {
            (true, None) => crate::ai::hold(world, e),
            (true, Some(ok)) if ok.speed_mps <= ALONGSIDE_SPEED_MPS => crate::ai::hold(world, e),
            (true, Some(ok)) => {
                let ahead = sim_geo::direct(at, ok.heading_deg, RAS_LEAD_M).point;
                if let Some(mut r) = world.get_mut::<Route>(e) {
                    r.waypoints = vec![ahead];
                    r.next = 0;
                }
                if let Some(mut k) = world.get_mut::<Kinematics>(e) {
                    k.ordered_speed_mps = ok.speed_mps.min(k.max_speed_mps);
                }
            }
            (false, Some(_)) => {
                set_route(world, e, id, at, tick);
                ensure_moving(world, e, 0.7);
            }
            (false, None) => {
                let idle = world
                    .get::<Route>(e)
                    .is_none_or(|r| r.next >= r.waypoints.len());
                if idle {
                    set_route(world, e, id, at, tick);
                    ensure_moving(world, e, 0.7);
                }
            }
        }
    }
}

/// Units alongside each facility this period, and those waiting for a berth.
fn alongside(
    world: &mut World,
    facilities: &[(Facility, GeoPoint, Option<f64>)],
) -> (BTreeMap<UnitId, (u32, Entity)>, BTreeMap<UnitId, u32>) {
    let mut units: Vec<(UnitId, Entity, SideId, GeoPoint, f64, Option<f64>)> = world
        .query::<(Entity, &Unit, &Position, &Kinematics)>()
        .iter(world)
        .map(|(e, u, p, k)| (u.id, e, u.side, p.point, k.speed_mps, u.draft_m))
        .collect();
    units.sort_by_key(|u| u.0);
    let mut served: BTreeMap<UnitId, (u32, Entity)> = BTreeMap::new();
    let mut waiting: BTreeMap<UnitId, u32> = BTreeMap::new();
    for (f, at, carrier_speed) in facilities {
        let mut n = 0u32;
        for &(id, e, side, here, speed, draft) in &units {
            // At a fixed facility a unit must be nearly stopped; alongside an oiler it must keep
            // within 1 m/s of the oiler's speed.
            let slow = match carrier_speed {
                None => speed <= ALONGSIDE_SPEED_MPS,
                Some(v) => (speed - v).abs() <= 1.0,
            };
            if side != f.side
                || f.unit == Some(id)
                || !slow
                || served.contains_key(&id)
                || !fits(f, draft)
                || !inside(f, *at, here)
            {
                continue;
            }
            if f.berths > 0 && n >= f.berths {
                waiting.entry(id).or_insert(f.id);
                continue;
            }
            n += 1;
            served.insert(id, (f.id, e));
            waiting.remove(&id);
        }
    }
    (served, waiting)
}

fn serve(world: &mut World, tick: Tick) {
    let facilities: Vec<(Facility, GeoPoint, Option<f64>)> = world
        .resource::<PlanningLayer>()
        .facilities
        .iter()
        .filter(|f| f.services.repair || f.services.rearm || f.services.refuel)
        .filter_map(|f| {
            let at = post_position(world, f)?;
            let carrier = f
                .unit
                .and_then(|u| world.resource::<UnitIndex>().0.get(&u).copied())
                .and_then(|o| world.get::<Kinematics>(o))
                .map(|k| k.speed_mps);
            Some((f.clone(), at, carrier))
        })
        .collect();
    let (alongside, waiting) = alongside(world, &facilities);
    // Units that left (or whose facility changed) start again; newly waiting units are logged.
    let newly_waiting: Vec<(UnitId, u32)> = {
        let mut layer = world.resource_mut::<PlanningLayer>();
        layer
            .service
            .retain(|id, s| alongside.get(id).is_some_and(|a| a.0 == s.facility));
        for (id, (fac, _)) in &alongside {
            layer.service.entry(*id).or_insert(ServiceState {
                facility: *fac,
                since: tick,
                rearmed: false,
                refuelled: false,
            });
        }
        let new: Vec<(UnitId, u32)> = waiting
            .iter()
            .filter(|(id, f)| layer.waiting.get(id) != Some(f))
            .map(|(id, f)| (*id, *f))
            .collect();
        layer.waiting = waiting;
        new
    };
    for (unit, facility) in newly_waiting {
        world
            .resource_mut::<EventLog>()
            .push(tick, SimEventKind::WaitingForBerth { unit, facility });
    }
    for (id, (fac, e)) in alongside {
        let Some((f, _, _)) = facilities.iter().find(|(f, _, _)| f.id == fac) else {
            continue;
        };
        let services = f.services;
        if services.repair {
            repair(world, id, e, fac, tick);
        }
        if services.refuel {
            refuel(world, id, e, f, tick);
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

/// One service period of fuel transfer: the facility's rate, up to the unit's capacity and the
/// facility's stock. A unit filled up is logged and, if it was sent here for fuel, resumes.
#[allow(clippy::cast_precision_loss)]
fn refuel(world: &mut World, id: UnitId, e: Entity, f: &Facility, tick: Tick) {
    let Some(fuel) = world.get::<crate::fuel::Fuel>(e).copied() else {
        return;
    };
    let room = fuel.capacity_t - fuel.fuel_t;
    if room <= 0.0 {
        return;
    }
    let rate = f.transfer_t_per_h.unwrap_or(DEFAULT_TRANSFER_T_PER_H) / 3_600.0;
    let left = world.resource::<PlanningLayer>().fuel_left(f);
    let amount = room
        .min(rate * SERVICE_PERIOD_TICKS as f64)
        .min(left.unwrap_or(f64::INFINITY));
    if amount <= 0.0 {
        return;
    }
    if let Some(mut fl) = world.get_mut::<crate::fuel::Fuel>(e) {
        fl.fuel_t = if amount >= room {
            fl.capacity_t
        } else {
            fl.fuel_t + amount
        };
    }
    *world
        .resource_mut::<PlanningLayer>()
        .issued
        .entry(f.id)
        .or_insert(0.0) += amount;
    if amount < room {
        return;
    }
    let first = world
        .resource_mut::<PlanningLayer>()
        .service
        .get_mut(&id)
        .is_some_and(|s| !std::mem::replace(&mut s.refuelled, true));
    if !first {
        return;
    }
    world.resource_mut::<EventLog>().push(
        tick,
        SimEventKind::UnitRefuelled {
            unit: id,
            facility: f.id,
        },
    );
    resume(world, id, e, DivertReason::Fuel);
}

/// A unit sent to a facility for `reason` takes up its mission again: its route and speed are
/// cleared, so the mission AI plans afresh.
fn resume(world: &mut World, id: UnitId, e: Entity, reason: DivertReason) {
    let mut layer = world.resource_mut::<PlanningLayer>();
    if layer.diverted.get(&id).is_none_or(|d| d.reason != reason) {
        return;
    }
    let resumed = layer.diverted.remove(&id);
    if let Some(mission) = resumed.and_then(|d| d.mission) {
        if let Some(mut m) = world.get_mut::<MissionState>(e) {
            m.mission = Some(mission);
            m.complete = false;
        }
        if let Some(mut r) = world.get_mut::<Route>(e) {
            r.waypoints.clear();
            r.next = 0;
        }
        if let Some(mut k) = world.get_mut::<Kinematics>(e) {
            k.ordered_speed_mps = 0.0;
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
    h.status = Status::assess(&h.profile, h.damage, &h.criticals);
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
    resume(world, id, e, DivertReason::Repair);
}
