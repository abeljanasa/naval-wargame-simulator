//! Save games: full world state + RNG state + command log, MessagePack-encoded and versioned.

use crate::ai::MissionState;
use crate::c2::C2State;
use crate::combat::{Health, Magazine, WeaponCatalog, WeaponsInFlight};
use crate::command::LoggedCommand;
use crate::components::{Kinematics, Position, Route, Unit};
use crate::detection::{ContactRecord, SensorSuite, Signature};
use crate::error::SimError;
use crate::esm::{BearingTable, EmitterLibrary};
use crate::events::SimEvent;
use crate::ids::SideId;
use crate::point_defence::PointDefence;
use crate::rng::RngStreams;
use crate::time::Tick;
use serde::{Deserialize, Serialize};
use sim_sensors::{Environment, JammerSpec};

pub const SAVE_MAGIC: &str = "NWS-SAVE";
/// Increment on any change to [`SaveGame`] and add a migration in [`SaveGame::from_bytes`].
///
/// History:
/// * 1: Phase 1 movement.
/// * 2: adds sensors, signatures and contacts. A v1 save loads as "no sensors, default
///   signature, no contacts" through the serde defaults below.
/// * 3: contacts become tracks with a covariance, classification and domain, and detection events
///   carry measurement errors. A v2 save loads with zero covariance, `unknown` classification and
///   no measurement errors (serde defaults).
/// * 4: adds postures and side/group doctrine (`c2`), and unit group and doctrine. Older saves
///   load with no postures set (all neutral) and no doctrine (defaults).
/// * 5: adds the environment (sea state, weather), unit jammers, radar clutter and beamwidth
///   figures, and clutter/weather/jamming on detection events. Older saves load with a calm sea,
///   no weather, no jammers and the radar defaults.
/// * 6: adds ESM bearing lines, the emitter library, and contact category and class. Older saves
///   load with none of these.
/// * 7: adds sonar: the sensor `active` flag, radiated noise on signatures, and sonar events.
///   Older saves load with passive-only sensors and the domain noise defaults.
/// * 8: adds visual/EO/IR/MAD: environment start time and visibility, unit category and length,
///   and their detection events. Older saves load with a 1970 start, 40 km visibility and the
///   domain length defaults.
/// * 9: adds weapons and damage: unit magazines and health, the weapon catalogue, and weapons
///   in flight. Older saves load with empty magazines and undamaged units on the domain
///   damage profile.
/// * 10: adds point defence (fire channels, detections, interceptors in the air) and decoys on
///   weapons in flight. Older saves load with none.
/// * 11: adds unit missions. Older saves load with no missions.
pub const SAVE_FORMAT_VERSION: u32 = 11;

/// Every component of one unit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnitRecord {
    pub unit: Unit,
    pub position: Position,
    pub kinematics: Kinematics,
    pub route: Route,
    #[serde(default)]
    pub sensors: SensorSuite,
    /// `None` only in v1 saves; filled with the domain default on load.
    #[serde(default)]
    pub signature: Option<Signature>,
    #[serde(default)]
    pub jammer: Option<JammerSpec>,
    #[serde(default)]
    pub magazine: Magazine,
    /// `None` in saves before format 9: an undamaged unit on the domain profile.
    #[serde(default)]
    pub health: Option<Health>,
    #[serde(default)]
    pub mission: MissionState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveGame {
    pub magic: String,
    pub format_version: u32,
    pub protocol_version: u16,
    pub tick: Tick,
    pub next_seq: u64,
    pub rng: RngStreams,
    /// Sorted by unit id.
    pub units: Vec<UnitRecord>,
    /// Every accepted command, in sequence order, including future-scheduled ones not yet applied.
    pub commands: Vec<LoggedCommand>,
    pub events: Vec<SimEvent>,
    #[serde(default)]
    pub contacts: Vec<ContactRecord>,
    #[serde(default)]
    pub next_contact_numbers: Vec<(SideId, u32)>,
    #[serde(default)]
    pub c2: C2State,
    #[serde(default)]
    pub environment: Environment,
    #[serde(default)]
    pub bearings: BearingTable,
    #[serde(default)]
    pub emitter_library: EmitterLibrary,
    #[serde(default)]
    pub weapon_catalog: WeaponCatalog,
    #[serde(default)]
    pub weapons_in_flight: WeaponsInFlight,
    #[serde(default)]
    pub point_defence: PointDefence,
}

impl SaveGame {
    pub fn to_bytes(&self) -> Result<Vec<u8>, SimError> {
        Ok(rmp_serde::to_vec_named(self)?)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SimError> {
        // Read only the header first, so a newer, unknown format gets a clear error instead of a decode failure.
        #[derive(Deserialize)]
        struct Header {
            magic: String,
            format_version: u32,
        }
        let header: Header = rmp_serde::from_slice(bytes).map_err(|_| SimError::NotASave)?;
        if header.magic != SAVE_MAGIC {
            return Err(SimError::NotASave);
        }
        match header.format_version {
            v if v > SAVE_FORMAT_VERSION => Err(SimError::UnknownFutureVersion {
                found: v,
                supported: SAVE_FORMAT_VERSION,
            }),
            // v1 and v2: the newer fields take their serde defaults (see the history above).
            1..=10 | SAVE_FORMAT_VERSION => {
                let mut save: SaveGame = rmp_serde::from_slice(bytes)?;
                if !save.rng.is_complete() {
                    return Err(SimError::CorruptSave("missing RNG stream".into()));
                }
                save.format_version = SAVE_FORMAT_VERSION;
                Ok(save)
            }
            v => Err(SimError::UnsupportedVersion { found: v }),
        }
    }
}
