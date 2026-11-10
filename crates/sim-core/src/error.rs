use crate::time::Tick;

#[derive(Debug, thiserror::Error)]
pub enum SimError {
    #[error("command scheduled for {scheduled} but the simulation is already at {now}")]
    CommandInPast { scheduled: Tick, now: Tick },
    #[error("invalid command: {0}")]
    InvalidCommand(String),
    #[error("not a save file (bad magic)")]
    NotASave,
    #[error("save format version {found} is newer than this build supports ({supported})")]
    UnknownFutureVersion { found: u32, supported: u32 },
    #[error("save format version {found} is no longer supported")]
    UnsupportedVersion { found: u32 },
    #[error("corrupt save: {0}")]
    CorruptSave(String),
    #[error("save encode failed: {0}")]
    Encode(#[from] rmp_serde::encode::Error),
    #[error("save decode failed: {0}")]
    Decode(#[from] rmp_serde::decode::Error),
}
