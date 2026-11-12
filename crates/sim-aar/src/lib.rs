//! Replay and after-action review (`docs/models/replay-aar.md`).
//!
//! - [`ExerciseRecord`]: the seed, the coastline id, the command log and the final state hash of
//!   one exercise, plus the scenario it started from. It is everything needed to rebuild the run
//!   exactly (ADR-0003).
//! - [`Replay`]: rebuilds the run once, checks the final hash, keeps keyframes, and then seeks to
//!   any tick for the scrubber.
//! - [`AarReport`]: tables built from the command and event logs (losses, weapons expended
//!   against effective, detections, an event timeline) with CSV export.

#![forbid(unsafe_code)]

mod csv;
mod record;
mod replay;
mod report;

pub use csv::AarTable;
pub use record::{ExerciseRecord, RECORD_FORMAT_VERSION, RECORD_MAGIC};
pub use replay::{Replay, MAX_KEYFRAMES, MIN_KEYFRAME_INTERVAL};
pub use report::{
    AarReport, DetectionRow, LossRow, SideSummary, TimelineEntry, TimelineKind, WeaponRow,
};

/// Errors from records and replays.
#[derive(Debug, thiserror::Error)]
pub enum AarError {
    #[error("not an exercise record")]
    NotARecord,
    #[error("exercise record format {found} is newer than this build supports ({supported})")]
    FutureVersion { found: u32, supported: u32 },
    #[error("record encoding: {0}")]
    Encode(#[from] rmp_serde::encode::Error),
    #[error("record decoding: {0}")]
    Decode(#[from] rmp_serde::decode::Error),
    #[error("the record was made with coastline {0:?}, which this build does not have")]
    UnknownLandMask(String),
    #[error(transparent)]
    Sim(#[from] sim_core::SimError),
}
