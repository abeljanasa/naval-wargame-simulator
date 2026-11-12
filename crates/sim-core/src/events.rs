//! Event log: what happened and why. Detection and engagement events record their model inputs and
//! random draws here, so every outcome can be explained (master prompt §8).

use crate::ids::{SideId, UnitId};
use crate::time::Tick;
use bevy_ecs::prelude::Resource;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SimEvent {
    pub tick: Tick,
    pub kind: SimEventKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SimEventKind {
    UnitSpawned {
        unit: UnitId,
    },
    CommandApplied {
        seq: u64,
    },
    CommandRejected {
        seq: u64,
        reason: String,
    },
    WaypointReached {
        unit: UnitId,
        index: u32,
    },
    RouteCompleted {
        unit: UnitId,
    },
    /// One radar detection draw, with every model input, so "why was / wasn't it seen?" can be
    /// answered and checked by hand (master prompt §8).
    DetectionAttempt {
        observer: UnitId,
        sensor: String,
        target: UnitId,
        range_m: f64,
        horizon_m: f64,
        r50_m: f64,
        pd: f64,
        draw: f64,
        detected: bool,
        /// Clutter loss applied, dB (0 in saves before format 5).
        #[serde(default)]
        clutter_db: f64,
        /// Weather loss applied, dB.
        #[serde(default)]
        weather_db: f64,
        /// Total linear jamming-to-noise ratio at the radar.
        #[serde(default)]
        jn: f64,
        /// Measurement errors drawn for a detection (none when not detected, or in v2 saves).
        #[serde(default)]
        range_error_m: Option<f64>,
        #[serde(default)]
        bearing_error_deg: Option<f64>,
    },
    ContactGained {
        side: SideId,
        number: u32,
        target: UnitId,
    },
    /// A route crossing land was planned round it: `legs` legs replanned, `waypoints` in the result.
    RouteReplanned {
        unit: UnitId,
        legs: u32,
        waypoints: u32,
    },
    /// A scenario route could not be planned round land and was dropped.
    RouteBlocked {
        unit: UnitId,
        reason: String,
    },
    /// A ship or submarine stopped rather than run onto land.
    LandAhead {
        unit: UnitId,
    },
    /// One ESM intercept draw against one emitter, with every model input.
    EsmIntercept {
        observer: UnitId,
        sensor: String,
        emitter_unit: UnitId,
        /// Emitter type (database sensor id or `JAMMER`).
        emitter: String,
        range_m: f64,
        horizon_m: f64,
        intercept_range_m: f64,
        pd: f64,
        draw: f64,
        detected: bool,
        bearing_error_deg: Option<f64>,
    },
    /// One sonar detection draw, with every term of the sonar equation.
    SonarAttempt {
        observer: UnitId,
        sensor: String,
        target: UnitId,
        /// Active transmission (else passive listening).
        active: bool,
        range_m: f64,
        sl_db: f64,
        tl_db: f64,
        nl_db: f64,
        di_db: f64,
        dt_db: f64,
        se_db: f64,
        pd: f64,
        draw: f64,
        detected: bool,
        range_error_m: Option<f64>,
        bearing_error_deg: Option<f64>,
    },
    /// One visual, EO or IR detection draw.
    OpticalAttempt {
        observer: UnitId,
        sensor: String,
        target: UnitId,
        range_m: f64,
        horizon_m: f64,
        r50_m: f64,
        /// 0 day, 1 twilight, 2 night (at the observer).
        light: u8,
        pd: f64,
        draw: f64,
        detected: bool,
        /// Identification level reached by a detection.
        identified: Option<sim_sensors::Classification>,
        range_error_m: Option<f64>,
        bearing_error_deg: Option<f64>,
    },
    /// A missile or torpedo left its launcher.
    WeaponLaunched {
        weapon_id: u64,
        weapon: String,
        shooter: UnitId,
        side: SideId,
        contact: u32,
    },
    /// A seeker's one acquisition draw.
    SeekerAcquisition {
        weapon_id: u64,
        range_m: f64,
        pd: f64,
        draw: f64,
        acquired: bool,
    },
    /// Probability-of-hit roll with its modifiers (`weapon_id` is `None` for guns and other
    /// weapons resolved at once).
    HitRoll {
        weapon_id: Option<u64>,
        weapon: String,
        /// The firing unit (none for mines). Absent before save format 16.
        #[serde(default)]
        shooter: Option<UnitId>,
        /// The firing side (the minefield's side for mines). Absent before save format 16.
        #[serde(default)]
        side: Option<SideId>,
        target: UnitId,
        base_ph: f64,
        size_modifier: f64,
        speed_modifier: f64,
        sea_modifier: f64,
        /// Soft kill (decoy, target jamming); 1 when none. Absent before save format 10.
        #[serde(default = "one")]
        countermeasure_modifier: f64,
        ph: f64,
        draw: f64,
        hit: bool,
    },
    /// A scenario event fired (its actions follow).
    ScenarioEventFired {
        event: String,
    },
    /// A scenario event's trigger occurred but condition `condition` (0-based) did not hold.
    ScenarioEventHeld {
        event: String,
        condition: u32,
    },
    /// Condition `condition` (0-based, a Lua script) of a triggered event could not be
    /// evaluated; the event is held.
    ScenarioScriptFailed {
        event: String,
        condition: u32,
        reason: String,
    },
    /// Action `action` (0-based) of a fired event could not be carried out.
    ScenarioActionFailed {
        event: String,
        action: u32,
        reason: String,
    },
    /// A white-cell facilitation action (inject, message, event fired or switched).
    Facilitated {
        umpire: String,
        action: crate::scenario_events::Facilitation,
    },
    /// A white-cell override, with who made it and why.
    Adjudicated {
        umpire: String,
        reason: String,
        action: crate::adjudication::Adjudication,
    },
    /// One transit-risk draw for a ship moving through another side's minefield.
    MineEncounter {
        unit: UnitId,
        field: u32,
        distance_m: f64,
        p: f64,
        draw: f64,
        encountered: bool,
    },
    /// A patrolling unit picked its next point (two `Ai` draws).
    MissionWaypoint {
        unit: UnitId,
        point: sim_geo::GeoPoint,
        draws: [f64; 2],
    },
    MissionComplete {
        unit: UnitId,
    },
    /// The AI engaged a contact on the unit's behalf (the launch or hit roll follows).
    MissionEngage {
        unit: UnitId,
        contact: u32,
        weapon: String,
    },
    /// A defender fired an interceptor (SAM, gun or CIWS) at an incoming weapon.
    PointDefenceShot {
        defender: UnitId,
        interceptor: String,
        threat: u64,
        range_m: f64,
        resolve_at: crate::time::Tick,
    },
    /// The Ph draw of an interceptor reaching its threat.
    InterceptRoll {
        defender: UnitId,
        interceptor: String,
        threat: u64,
        ph: f64,
        draw: f64,
        killed: bool,
    },
    /// A ship under attack fired a decoy at an incoming weapon.
    DecoyLaunched {
        unit: UnitId,
        decoy: String,
        threat: u64,
        effect: f64,
    },
    WeaponMissed {
        weapon_id: u64,
        reason: String,
    },
    /// A hit's damage and its critical, fire and flooding draws.
    DamageTaken {
        unit: UnitId,
        damage_dp: f64,
        total_dp: f64,
        critical_draw: f64,
        critical: Option<sim_weapons::System>,
        fire_draw: f64,
        fire_started: bool,
        flood_draw: f64,
        flooding_started: bool,
        status: sim_weapons::Status,
    },
    /// A minute of fire, flooding and damage control.
    DamageControl {
        unit: UnitId,
        fire_dp: f64,
        flood_dp: f64,
        total_dp: f64,
        /// One draw per fire, then one for flooding.
        draws: Vec<f64>,
        fires_out: u32,
        flooding_stopped: bool,
        status: sim_weapons::Status,
    },
    UnitDestroyed {
        unit: UnitId,
    },
    /// One MAD detection draw.
    MadAttempt {
        observer: UnitId,
        target: UnitId,
        slant_range_m: f64,
        r50_m: f64,
        pd: f64,
        draw: f64,
        detected: bool,
    },
    /// Bearings (ESM and passive sonar) from several observers of a side were combined into a
    /// position fix.
    EsmFix {
        side: SideId,
        target: UnitId,
        bearings: u32,
    },
    ContactLost {
        side: SideId,
        number: u32,
        target: UnitId,
    },
    /// `side` gained contact `contact` inside its sense zone `zone` (planning layer).
    ZoneAlert {
        side: SideId,
        zone: u32,
        contact: u32,
    },
    /// A unit alongside facility `facility` is fully repaired; its systems are restored.
    UnitRepaired {
        unit: UnitId,
        facility: u32,
    },
    /// A unit alongside facility `facility` has been rearmed to its loadout.
    UnitRearmed {
        unit: UnitId,
        facility: u32,
    },
    /// A command post was lost with the unit carrying it.
    CommandPostDown {
        facility: u32,
    },
    /// A command post went down while jammed or with its comms knocked out.
    CommandPostDisrupted {
        facility: u32,
        status: sim_c2::comms::LinkStatus,
    },
    /// A disrupted command post is back up.
    CommandPostRestored {
        facility: u32,
    },
    /// A unit ran out of fuel: no propulsion until refuelled.
    FuelExhausted {
        unit: UnitId,
    },
    /// A unit on a mission below bingo fuel was sent to a refuelling point.
    DivertedForFuel {
        unit: UnitId,
        facility: u32,
    },
    /// A unit was filled up at a refuelling point.
    UnitRefuelled {
        unit: UnitId,
        facility: u32,
    },
    /// A unit is waiting for a free berth at a facility.
    WaitingForBerth {
        unit: UnitId,
        facility: u32,
    },
    /// A unit on a mining mission laid its field.
    MissionMinesLaid {
        unit: UnitId,
        field: u32,
        mines: u32,
    },
    /// A mission could not be carried out (it is marked complete).
    MissionFailed {
        unit: UnitId,
        reason: String,
    },
    /// A unit clearing mines tried to sweep one mine of a field: P = 1 − exp(−ρ·W·d).
    MineSwept {
        unit: UnitId,
        field: u32,
        distance_m: f64,
        p: f64,
        draw: f64,
        swept: bool,
    },
    CargoLoaded {
        unit: UnitId,
        tonnes: f64,
    },
    CargoDelivered {
        unit: UnitId,
        tonnes: f64,
    },
    /// A surface unit at or above its weather limit was sent to a sheltering port.
    ShelteringFromWeather {
        unit: UnitId,
        facility: u32,
    },
    /// The weather eased: a sheltering unit resumes its mission.
    WeatherEased {
        unit: UnitId,
    },
    /// A unit whose mission is complete was sent to hold at a staging anchorage.
    HoldingAtAnchorage {
        unit: UnitId,
        facility: u32,
    },
    /// An order held while its command post was down was carried out.
    QueuedOrderApplied {
        seq: u64,
    },
    /// An order held while its command post was down could not be carried out.
    QueuedOrderDropped {
        seq: u64,
        reason: String,
    },
    /// An engage zone's assigned shooter engaged a contact inside it (the launch or hit roll
    /// follows).
    ZoneEngage {
        zone: u32,
        unit: UnitId,
        contact: u32,
        weapon: String,
    },
    /// Another side's unit entered a choke point.
    ChokePointTransit {
        choke_point: String,
        unit: UnitId,
    },
    /// A mission-killed unit was sent to a port for repair.
    DivertedToPort {
        unit: UnitId,
        facility: u32,
    },
}

#[derive(Debug, Clone, Default, Resource, Serialize, Deserialize)]
pub struct EventLog {
    pub events: Vec<SimEvent>,
}

impl EventLog {
    pub fn push(&mut self, tick: Tick, kind: SimEventKind) {
        self.events.push(SimEvent { tick, kind });
    }
}

fn one() -> f64 {
    1.0
}
