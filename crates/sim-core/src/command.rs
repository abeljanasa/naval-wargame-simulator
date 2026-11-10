//! Commands: the only way anything outside the core changes simulation state.

use crate::components::Domain;
use crate::detection::SensorSpec;
use crate::ids::{SideId, UnitId};
use crate::time::Tick;
use serde::{Deserialize, Serialize};
use sim_c2::{Doctrine, Posture};
use sim_geo::GeoPoint;
use sim_sensors::{Environment, JammerSpec};

/// Version of the command and snapshot wire protocol. Bump it on any breaking change to
/// [`Command`], [`CommandPayload`] or [`crate::Snapshot`], and add a migration.
pub const PROTOCOL_VERSION: u16 = 1;

/// Who issued a command. Recorded in the command log for audit and AAR.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Issuer {
    /// Scenario loader (initial state).
    Scenario,
    /// A player at a seat.
    Player { seat: u32 },
    /// White cell / umpire. Allowed to command any side.
    WhiteCell { umpire: String },
    /// Side AI.
    Ai,
    /// Event engine / Lua script.
    Script,
}

impl Issuer {
    /// Issuers allowed to act on units of any side.
    #[must_use]
    pub fn is_privileged(&self) -> bool {
        matches!(self, Issuer::Scenario | Issuer::WhiteCell { .. })
    }
}

/// An input to the simulation, applied at the start of `tick`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Command {
    pub tick: Tick,
    pub issuer: Issuer,
    /// The side the issuer is acting for. A non-privileged issuer can only command units of this side.
    pub side: Option<SideId>,
    pub payload: CommandPayload,
}

/// Parameters for creating a unit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct UnitSpawn {
    pub id: UnitId,
    pub name: String,
    pub side: SideId,
    pub domain: Domain,
    pub position: GeoPoint,
    #[serde(default)]
    pub altitude_m: f64,
    #[serde(default)]
    pub heading_deg: f64,
    #[serde(default)]
    pub speed_mps: f64,
    pub max_speed_mps: f64,
    pub accel_mps2: f64,
    #[serde(default)]
    pub route: Vec<GeoPoint>,
    /// Platform database id this unit was resolved from (e.g. `GEN-FFG`), if any.
    #[serde(default)]
    pub platform: Option<String>,
    /// Fitted sensors, copied from the platform database when the scenario is built. Carrying the
    /// data in the command (instead of a database reference) keeps replays independent of later
    /// database edits.
    #[serde(default)]
    pub sensors: Vec<SensorSpec>,
    /// Radar cross-section, m². `None` falls back to a domain default (see `detection`).
    #[serde(default)]
    pub rcs_m2: Option<f64>,
    /// Turn-rate cap, deg/s. `None` = instant heading changes.
    #[serde(default)]
    pub max_turn_rate_dps: Option<f64>,
    /// Minimum turn radius, metres (0 or absent = no radius limit).
    #[serde(default)]
    pub min_turn_radius_m: Option<f64>,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub doctrine: Doctrine,
    #[serde(default)]
    pub jammer: Option<JammerSpec>,
}

/// Where a doctrine setting applies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "level", rename_all = "snake_case")]
pub enum DoctrineScope {
    Side { side: SideId },
    Group { side: SideId, group: String },
    Unit { unit: UnitId },
}

/// What a command does.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "type", rename_all = "snake_case")]
// Spawns are much larger than orders, but commands are few (thousands per run, not millions) and
// boxing would change every constructor and the serialised form's Rust API for no measurable gain.
#[allow(clippy::large_enum_variant)]
pub enum CommandPayload {
    SpawnUnit(UnitSpawn),
    /// Replace the unit's route. An empty list clears it, and the unit holds its current course.
    SetRoute {
        unit: UnitId,
        waypoints: Vec<GeoPoint>,
    },
    /// Set the ordered speed (m/s). Values above the unit's maximum are clamped by the movement model.
    SetSpeed {
        unit: UnitId,
        speed_mps: f64,
    },
    /// How `side` regards `toward`. Scenario and white cell only.
    SetPosture {
        side: SideId,
        toward: SideId,
        posture: Posture,
    },
    /// Replaces the doctrine set at `scope` (unset fields inherit). A player may set doctrine
    /// for their own side, its groups and its units.
    SetDoctrine {
        scope: DoctrineScope,
        doctrine: Doctrine,
    },
    /// Sea state and weather loss for every radar. Scenario and white cell only.
    SetEnvironment {
        environment: Environment,
    },
}

impl CommandPayload {
    /// The unit this command acts on, if it is an existing unit.
    #[must_use]
    pub fn target_unit(&self) -> Option<UnitId> {
        match self {
            CommandPayload::SetRoute { unit, .. }
            | CommandPayload::SetSpeed { unit, .. }
            | CommandPayload::SetDoctrine {
                scope: DoctrineScope::Unit { unit },
                ..
            } => Some(*unit),
            CommandPayload::SpawnUnit(_)
            | CommandPayload::SetPosture { .. }
            | CommandPayload::SetEnvironment { .. }
            | CommandPayload::SetDoctrine { .. } => None,
        }
    }

    /// Checks the payload on its own, without looking at world state.
    pub(crate) fn validate(&self) -> Result<(), String> {
        let check_points = |pts: &[GeoPoint]| {
            if pts.iter().all(GeoPoint::is_valid) {
                Ok(())
            } else {
                Err("waypoint has invalid coordinates".to_owned())
            }
        };
        match self {
            CommandPayload::SpawnUnit(s) => {
                if !s.position.is_valid() {
                    return Err("spawn position invalid".into());
                }
                if !(s.max_speed_mps.is_finite() && s.max_speed_mps >= 0.0) {
                    return Err("max_speed_mps must be finite and >= 0".into());
                }
                // Fixed sites (max speed 0) may have zero acceleration; anything mobile may not.
                if !(s.accel_mps2.is_finite()
                    && (s.accel_mps2 > 0.0 || (s.max_speed_mps == 0.0 && s.accel_mps2 == 0.0)))
                {
                    return Err("accel_mps2 must be finite and > 0 for a mobile unit".into());
                }
                let bad = |v: Option<f64>| v.is_some_and(|x| !(x.is_finite() && x >= 0.0));
                if bad(s.max_turn_rate_dps) || bad(s.min_turn_radius_m) {
                    return Err("turn rate and turn radius must be finite and >= 0".into());
                }
                if s.rcs_m2.is_some_and(|r| !(r.is_finite() && r >= 0.0)) {
                    return Err("rcs_m2 must be finite and >= 0".into());
                }
                for sensor in &s.sensors {
                    sensor.validate()?;
                }
                if s.jammer.is_some_and(|j| !j.jn_db_at_1km.is_finite()) {
                    return Err("jammer jn_db_at_1km must be finite".into());
                }
                if !(s.speed_mps.is_finite()
                    && s.heading_deg.is_finite()
                    && s.altitude_m.is_finite())
                {
                    return Err("spawn speed, heading and altitude must be finite".into());
                }
                check_points(&s.route)
            }
            CommandPayload::SetRoute { waypoints, .. } => check_points(waypoints),
            CommandPayload::SetSpeed { speed_mps, .. } => {
                if speed_mps.is_finite() && *speed_mps >= 0.0 {
                    Ok(())
                } else {
                    Err("speed must be finite and >= 0".into())
                }
            }
            CommandPayload::SetPosture { side, toward, .. } => {
                if side == toward {
                    Err("a side is always friendly to itself".into())
                } else {
                    Ok(())
                }
            }
            CommandPayload::SetDoctrine {
                scope: DoctrineScope::Group { group, .. },
                ..
            } if group.trim().is_empty() => Err("group name is empty".into()),
            CommandPayload::SetDoctrine { .. } => Ok(()),
            CommandPayload::SetEnvironment { environment } => environment.validate(),
        }
    }
}

/// A command as accepted into the log, with its sequence number (unique and increasing).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct LoggedCommand {
    pub seq: u64,
    pub command: Command,
}
