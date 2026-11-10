use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("cannot read {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{path}: {source}")]
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("{context}: {message}")]
    Source { context: String, message: String },
    #[error("database schema version {found} is newer than this build supports ({supported}); refusing to load")]
    FutureSchema { found: u32, supported: u32 },
    #[error("database schema version {found} is older than {supported}; run `sim-db migrate --db <file>` first")]
    OutdatedSchema { found: u32, supported: u32 },
    #[error("database failed validation with {errors} error(s); first: {first}")]
    Invalid { errors: usize, first: String },
    #[error("accredited databases cannot be loaded by the open build (enable the `accredited` feature in the accredited pipeline)")]
    AccreditedUnavailable,
}
