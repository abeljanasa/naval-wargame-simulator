//! Loading a built database for the simulation: the `DbSource` extension point (master prompt §3).

use crate::error::DbError;
use crate::migrate::{schema_version, LATEST_VERSION};
use crate::validate::{validate, Policy, Severity};
use rusqlite::{Connection, OpenFlags, OptionalExtension, Row};
use std::path::PathBuf;

/// Where a platform database comes from.
#[derive(Debug, Clone)]
pub enum DbSource {
    /// The open, unclassified database shipped with this repository's build.
    Open(PathBuf),
    /// A separately accredited database file, loaded only on approved systems. The open build
    /// refuses it unless compiled with the `accredited` feature, which only the separate
    /// accredited pipeline enables.
    Accredited(PathBuf),
}

/// The handful of platform fields the Phase 1 movement model needs.
#[derive(Debug, Clone, PartialEq)]
pub struct PlatformSummary {
    pub id: String,
    pub name: String,
    pub category: String,
    pub domain: String,
    pub max_speed_mps: f64,
    pub cruise_speed_mps: f64,
    pub accel_mps2: f64,
    pub max_altitude_m: Option<f64>,
    pub max_depth_m: Option<f64>,
    pub length_m: Option<f64>,
}

impl PlatformSummary {
    fn from_row(r: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: r.get(0)?,
            name: r.get(1)?,
            category: r.get(2)?,
            domain: r.get(3)?,
            max_speed_mps: r.get(4)?,
            cruise_speed_mps: r.get(5)?,
            accel_mps2: r.get(6)?,
            max_altitude_m: r.get(7)?,
            max_depth_m: r.get(8)?,
            length_m: r.get(9)?,
        })
    }
}

/// A sensor as fitted to a platform.
#[derive(Debug, Clone, PartialEq)]
pub struct MountedSensor {
    pub sensor_id: String,
    /// Database sensor kind, e.g. `radar`, `esm`, `sonar_hull`.
    pub kind: String,
    pub mount_height_m: f64,
    pub min_range_m: f64,
    pub max_range_m: f64,
    pub ref_detect_range_m: Option<f64>,
    pub ref_target_rcs_m2: Option<f64>,
    pub max_target_altitude_m: f64,
    pub clutter_db_per_sea_state: Option<f64>,
    pub beamwidth_deg: Option<f64>,
}

const SUMMARY_COLS: &str = "id, name, category, domain, max_speed_mps, cruise_speed_mps, accel_mps2, max_altitude_m, max_depth_m, length_m";

/// A validated, read-only platform database.
pub struct PlatformDb {
    conn: Connection,
    policy: Policy,
}

impl std::fmt::Debug for PlatformDb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlatformDb")
            .field("policy", &self.policy)
            .finish_non_exhaustive()
    }
}

impl PlatformDb {
    /// Opens a database file read-only. The schema version must match this build exactly, and
    /// the database must pass validation with no errors under the source's policy.
    pub fn open(source: &DbSource) -> Result<Self, DbError> {
        let (path, policy) = match source {
            DbSource::Open(p) => (p, Policy::Open),
            #[cfg(feature = "accredited")]
            DbSource::Accredited(p) => (p, Policy::Accredited),
            #[cfg(not(feature = "accredited"))]
            DbSource::Accredited(_) => return Err(DbError::AccreditedUnavailable),
        };
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        Self::from_connection(conn, policy)
    }

    /// Wraps an existing connection (e.g. an in-memory database just built from source),
    /// applying the same version and validation checks as [`PlatformDb::open`].
    pub fn from_connection(conn: Connection, policy: Policy) -> Result<Self, DbError> {
        let found = schema_version(&conn)?;
        if found > LATEST_VERSION {
            return Err(DbError::FutureSchema {
                found,
                supported: LATEST_VERSION,
            });
        }
        if found < LATEST_VERSION {
            return Err(DbError::OutdatedSchema {
                found,
                supported: LATEST_VERSION,
            });
        }
        let issues = validate(&conn, policy)?;
        let errors: Vec<_> = issues
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .collect();
        if let Some(first) = errors.first() {
            return Err(DbError::Invalid {
                errors: errors.len(),
                first: first.to_string(),
            });
        }
        Ok(Self { conn, policy })
    }

    /// Re-runs the validation suite. An open database has no errors (opening would have failed),
    /// so this returns its warnings: the feedback a data reviewer acts on.
    pub fn issues(&self) -> Result<Vec<crate::validate::Issue>, DbError> {
        validate(&self.conn, self.policy)
    }

    #[must_use]
    pub fn policy(&self) -> Policy {
        self.policy
    }

    pub fn platform(&self, id: &str) -> Result<Option<PlatformSummary>, DbError> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {SUMMARY_COLS} FROM platforms WHERE id = ?1"),
                [id],
                PlatformSummary::from_row,
            )
            .optional()?)
    }

    /// Sensors fitted to a platform, in mount order.
    pub fn sensors_of(&self, platform_id: &str) -> Result<Vec<MountedSensor>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT s.id, s.kind, m.height_m, s.min_range_m, s.max_range_m, s.ref_detect_range_m,
                    s.ref_target_rcs_m2, s.max_target_altitude_m, s.clutter_db_per_sea_state,
                    s.beamwidth_deg
               FROM mounts m JOIN sensors s ON s.id = m.sensor_id
              WHERE m.platform_id = ?1 AND m.kind = 'sensor'
              ORDER BY m.id",
        )?;
        let v = stmt
            .query_map([platform_id], |r| {
                Ok(MountedSensor {
                    sensor_id: r.get(0)?,
                    kind: r.get(1)?,
                    mount_height_m: r.get(2)?,
                    min_range_m: r.get(3)?,
                    max_range_m: r.get(4)?,
                    ref_detect_range_m: r.get(5)?,
                    ref_target_rcs_m2: r.get(6)?,
                    max_target_altitude_m: r.get(7)?,
                    clutter_db_per_sea_state: r.get(8)?,
                    beamwidth_deg: r.get(9)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(v)
    }

    /// The platform's all-aspect, any-speed radar cross-section (m²), falling back to the
    /// most general RCS entry available.
    pub fn rcs_m2(&self, platform_id: &str) -> Result<Option<f64>, DbError> {
        Ok(self
            .conn
            .query_row(
                "SELECT value FROM platform_signatures
                  WHERE platform_id = ?1 AND kind = 'rcs'
                  ORDER BY (aspect = 'all') DESC, (speed_band = 'any') DESC, aspect, speed_band
                  LIMIT 1",
                [platform_id],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// (source citation, confidence, classification) of a platform record.
    pub fn platform_provenance(
        &self,
        id: &str,
    ) -> Result<Option<(String, String, String)>, DbError> {
        Ok(self
            .conn
            .query_row(
                "SELECT source_citation, confidence, classification FROM platforms WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?)
    }

    /// Every platform, sorted by id.
    pub fn platforms(&self) -> Result<Vec<PlatformSummary>, DbError> {
        let mut stmt = self
            .conn
            .prepare(&format!("SELECT {SUMMARY_COLS} FROM platforms ORDER BY id"))?;
        let v = stmt
            .query_map([], PlatformSummary::from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(v)
    }
}
