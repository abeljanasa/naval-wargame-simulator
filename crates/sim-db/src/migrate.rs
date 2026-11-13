//! Forward-only, numbered schema migrations. The schema version is SQLite's `user_version`.

use crate::error::DbError;
use rusqlite::Connection;

pub struct Migration {
    pub version: u32,
    pub name: &'static str,
    pub sql: &'static str,
}

/// All migrations, in order. Append only: never edit or reorder a merged migration.
pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "0001_initial_schema",
        sql: include_str!("../migrations/0001_initial_schema.sql"),
    },
    Migration {
        version: 2,
        name: "0002_radar_clutter_beamwidth",
        sql: include_str!("../migrations/0002_radar_clutter_beamwidth.sql"),
    },
    Migration {
        version: 3,
        name: "0003_sonar_acoustics",
        sql: include_str!("../migrations/0003_sonar_acoustics.sql"),
    },
];

/// Schema version produced by applying every migration.
pub const LATEST_VERSION: u32 = 3;

pub fn schema_version(conn: &Connection) -> Result<u32, DbError> {
    Ok(conn.pragma_query_value(None, "user_version", |r| r.get(0))?)
}

/// Brings `conn` up to [`LATEST_VERSION`]. Each migration runs in its own transaction.
/// A database from a newer build is refused, never downgraded.
pub fn migrate(conn: &mut Connection) -> Result<u32, DbError> {
    let current = schema_version(conn)?;
    if current > LATEST_VERSION {
        return Err(DbError::FutureSchema {
            found: current,
            supported: LATEST_VERSION,
        });
    }
    for m in MIGRATIONS.iter().filter(|m| m.version > current) {
        let tx = conn.transaction()?;
        tx.execute_batch(m.sql)?;
        tx.pragma_update(None, "user_version", m.version)?;
        tx.commit()?;
    }
    schema_version(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_numbered_contiguously_and_match_latest() {
        for (i, m) in MIGRATIONS.iter().enumerate() {
            let expected = u32::try_from(i + 1).unwrap();
            assert_eq!(m.version, expected, "{}", m.name);
            assert!(m.name.starts_with(&format!("{expected:04}_")), "{}", m.name);
        }
        assert_eq!(MIGRATIONS.last().map(|m| m.version), Some(LATEST_VERSION));
    }

    #[test]
    fn fresh_database_migrates_to_latest_and_is_idempotent() {
        let mut c = Connection::open_in_memory().unwrap();
        assert_eq!(schema_version(&c).unwrap(), 0);
        assert_eq!(migrate(&mut c).unwrap(), LATEST_VERSION);
        assert_eq!(migrate(&mut c).unwrap(), LATEST_VERSION);
    }

    #[test]
    fn future_schema_is_refused() {
        let mut c = Connection::open_in_memory().unwrap();
        c.pragma_update(None, "user_version", LATEST_VERSION + 1)
            .unwrap();
        assert!(matches!(migrate(&mut c), Err(DbError::FutureSchema { .. })));
    }

    #[test]
    fn every_value_table_has_provenance_columns() {
        let mut c = Connection::open_in_memory().unwrap();
        migrate(&mut c).unwrap();
        for table in crate::PROVENANCE_TABLES {
            let mut stmt = c
                .prepare(&format!("SELECT name FROM pragma_table_info('{table}')"))
                .unwrap();
            let cols: Vec<String> = stmt
                .query_map([], |r| r.get(0))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap();
            for col in [
                "source_citation",
                "confidence",
                "classification",
                "is_generic",
                "created_by",
                "updated_at",
            ] {
                assert!(cols.iter().any(|c| c == col), "{table} lacks {col}");
            }
        }
    }
}
