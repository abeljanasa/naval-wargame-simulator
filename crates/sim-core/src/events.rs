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
        target: UnitId,
        base_ph: f64,
        size_modifier: f64,
        speed_modifier: f64,
        sea_modifier: f64,
        ph: f64,
        draw: f64,
        hit: bool,
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
