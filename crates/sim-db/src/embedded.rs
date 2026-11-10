//! The open database sources compiled into the binary, so the desktop app and the batch runner work
//! with no data files and no network (air-gapped requirement, master prompt §4.8).
//!
//! A test checks that this list matches `data/db/open/` exactly. When you add a platform file, add
//! it here as well.

use crate::db::PlatformDb;
use crate::error::DbError;
use crate::source::{build_set, SourceFile, SourceSet};
use crate::validate::Policy;
use rusqlite::Connection;

macro_rules! open_file {
    ($rel:literal) => {
        ($rel, include_str!(concat!("../../../data/db/open/", $rel)))
    };
}

/// (relative path, contents) of every platform file in `data/db/open/platforms/`.
pub const PLATFORM_FILES: &[(&str, &str)] = &[
    open_file!("platforms/aircraft.json"),
    open_file!("platforms/auxiliaries-and-merchants.json"),
    open_file!("platforms/land-facilities.json"),
    open_file!("platforms/submarines.json"),
    open_file!("platforms/surface-combatants.json"),
];

const REFERENCE: (&str, &str) = open_file!("reference.json");
const SENSORS: (&str, &str) = open_file!("sensors.json");
const WEAPONS: (&str, &str) = open_file!("weapons.json");

fn file((name, text): (&str, &str)) -> SourceFile {
    SourceFile {
        name: name.to_owned(),
        text: text.to_owned(),
    }
}

/// The embedded open source set.
#[must_use]
pub fn source_set() -> SourceSet {
    SourceSet {
        reference: file(REFERENCE),
        sensors: file(SENSORS),
        weapons: file(WEAPONS),
        platforms: PLATFORM_FILES.iter().copied().map(file).collect(),
    }
}

/// Builds the embedded open database in memory, validates it, and returns it ready to query.
pub fn open_platform_db() -> Result<PlatformDb, DbError> {
    let mut conn = Connection::open_in_memory()?;
    build_set(&mut conn, source_set())?;
    PlatformDb::from_connection(conn, Policy::Open)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::path::Path;

    #[test]
    fn embedded_list_matches_the_source_directory() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/db/open/platforms");
        let on_disk: BTreeSet<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| format!("platforms/{}", e.unwrap().file_name().to_string_lossy()))
            .filter(|n| n.ends_with(".json"))
            .collect();
        let embedded: BTreeSet<String> = PLATFORM_FILES
            .iter()
            .map(|(n, _)| (*n).to_owned())
            .collect();
        assert_eq!(on_disk, embedded, "update PLATFORM_FILES in embedded.rs");
    }

    #[test]
    fn embedded_database_opens_clean() {
        let db = open_platform_db().unwrap();
        assert_eq!(db.platforms().unwrap().len(), 50);
    }

    #[test]
    fn frigate_sensors_and_rcs_come_through() {
        let db = open_platform_db().unwrap();
        let sensors = db.sensors_of("GEN-FFG").unwrap();
        assert!(sensors
            .iter()
            .any(|s| s.sensor_id == "SEN-RDR-3D-MR" && (s.mount_height_m - 25.0).abs() < 1e-9));
        assert_eq!(db.rcs_m2("GEN-FFG").unwrap(), Some(5000.0));
        assert_eq!(db.rcs_m2("GEN-PORT-NAVAL").unwrap(), None);
    }
}
