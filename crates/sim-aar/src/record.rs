//! Exercise records: what is needed to rebuild a run, MessagePack-encoded and versioned.

use crate::AarError;
use serde::{Deserialize, Serialize};
use sim_core::{LoggedCommand, Simulation, Tick};

pub const RECORD_MAGIC: &str = "NWS-RECORD";
/// Increment on any change to [`ExerciseRecord`] and keep older versions loading.
///
/// History:
/// * 1: Phase 3 slice 6.
pub const RECORD_FORMAT_VERSION: u32 = 1;

/// One exercise, as recorded at its end (or at any point during it).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExerciseRecord {
    pub magic: String,
    pub format_version: u32,
    pub scenario_name: String,
    /// The scenario file the exercise started from, so the record opens on its own (sides, seats
    /// and names). The run itself is rebuilt from `commands` alone.
    pub scenario_json: String,
    pub seed: u64,
    /// The coastline the run navigated round (`None`: open ocean).
    pub land_mask: Option<String>,
    /// The terrain and bathymetry packs the run used (`None`: none). Records made before
    /// Phase 4 slice 2 have none.
    #[serde(default)]
    pub terrain: Option<String>,
    /// The tick the run had reached: the replay ends here.
    pub end_tick: Tick,
    /// [`Simulation::state_hash`] at `end_tick`; a replay that does not reach it is reported.
    pub final_hash: u64,
    /// Every accepted command in sequence order (scenario set-up, orders, overrides).
    pub commands: Vec<LoggedCommand>,
}

impl ExerciseRecord {
    /// Records a run as it stands now.
    pub fn capture(
        sim: &mut Simulation,
        scenario_name: impl Into<String>,
        scenario_json: impl Into<String>,
    ) -> Self {
        Self {
            magic: RECORD_MAGIC.to_owned(),
            format_version: RECORD_FORMAT_VERSION,
            scenario_name: scenario_name.into(),
            scenario_json: scenario_json.into(),
            seed: sim.seed(),
            land_mask: sim.land_mask_id(),
            terrain: sim.terrain_id(),
            end_tick: sim.tick(),
            final_hash: sim.state_hash(),
            commands: sim.command_log(),
        }
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, AarError> {
        Ok(rmp_serde::to_vec_named(self)?)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, AarError> {
        #[derive(Deserialize)]
        struct Header {
            magic: String,
            format_version: u32,
        }
        let header: Header = rmp_serde::from_slice(bytes).map_err(|_| AarError::NotARecord)?;
        if header.magic != RECORD_MAGIC {
            return Err(AarError::NotARecord);
        }
        if header.format_version > RECORD_FORMAT_VERSION {
            return Err(AarError::FutureVersion {
                found: header.format_version,
                supported: RECORD_FORMAT_VERSION,
            });
        }
        Ok(rmp_serde::from_slice(bytes)?)
    }
}
