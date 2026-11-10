//! Platform database (master prompt §9).
//!
//! * **Source of truth** is diffable JSON under `data/db/open/` ([`source`]), compiled into a
//!   SQLite file that is a build artefact (ADR-0004).
//! * **Schema** is defined by forward-only, numbered migrations ([`migrate`]).
//! * **Validation** ([`validate`]) checks provenance, classification, referential integrity and
//!   physical plausibility. It runs in CI (`cargo run -p sim-db -- validate`), and again whenever
//!   a database is opened.
//! * **Loading** goes through [`DbSource`]. The open build can only load `DbSource::Open`.
//!   `DbSource::Accredited` needs the `accredited` feature, which only the separate accredited
//!   pipeline enables.

#![forbid(unsafe_code)]

mod db;
pub mod embedded;
mod error;
pub mod migrate;
pub mod source;
pub mod validate;

pub use db::{DbSource, MountedSensor, PlatformDb, PlatformSummary};
pub use error::DbError;
pub use validate::{Issue, Policy, Severity};

/// The only classification marking allowed in the open build.
pub const OPEN_CLASSIFICATION: &str = "UNCLASSIFIED";

/// Every table that carries the provenance columns.
pub const PROVENANCE_TABLES: &[&str] = &[
    "nations",
    "services",
    "propulsion",
    "damage_profiles",
    "comms_systems",
    "sensors",
    "weapons",
    "platforms",
    "platform_signatures",
    "mounts",
    "magazines",
    "loadouts",
];
