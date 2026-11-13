//! sim-db integration tests: the shipped open data, every validation rule, the loader API and
//! the accredited extension point (synthetic data only).

#![allow(clippy::unwrap_used, clippy::expect_used)] // test harness

use rusqlite::Connection;
use sim_db::validate::validate;
use sim_db::{source, DbError, DbSource, Issue, PlatformDb, Policy, Severity};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn open_source_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/db/open")
}

fn built() -> Connection {
    let mut c = Connection::open_in_memory().unwrap();
    source::build(&mut c, &open_source_dir()).unwrap();
    c
}

fn rules(issues: &[Issue]) -> BTreeSet<&'static str> {
    issues.iter().map(|i| i.rule).collect()
}

/// A unique scratch directory under the OS temp dir (tests run in parallel).
fn temp_path(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nws-simdb-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

// ---------- the shipped open database ----------

#[test]
fn open_database_builds_and_validates_clean() {
    let c = built();
    let issues = validate(&c, Policy::Open).unwrap();
    let listing: Vec<String> = issues.iter().map(ToString::to_string).collect();
    assert!(
        issues.is_empty(),
        "unexpected issues:\n{}",
        listing.join("\n")
    );
}

#[test]
fn open_database_has_fifty_generic_types_across_required_categories() {
    let c = built();
    let n: i64 = c
        .query_row("SELECT count(*) FROM platforms", [], |r| r.get(0))
        .unwrap();
    assert!(n >= 50, "only {n} platforms");
    let mut stmt = c
        .prepare("SELECT DISTINCT category FROM platforms")
        .unwrap();
    let cats: BTreeSet<String> = stmt
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    // Master prompt §9: surface combatants, submarines, MPA, fighters, helicopters,
    // auxiliaries, merchant shipping, shore SAM/AShM batteries, airbases, ports.
    for required in [
        "surface_combatant",
        "submarine",
        "mpa",
        "fighter",
        "helicopter",
        "auxiliary",
        "merchant",
        "shore_battery",
        "airbase",
        "port",
    ] {
        assert!(cats.contains(required), "missing category {required}");
    }
}

/// The open database is unclassified throughout. Generic classes are generic; specific records
/// (the regional database, Phase 4 slice 13, and the Gate 4 historical records) each cite the
/// open source they come from.
#[test]
fn open_database_is_unclassified_and_specific_records_cite_their_sources() {
    let c = built();
    for table in sim_db::PROVENANCE_TABLES {
        let bad: i64 = c
            .query_row(
                &format!("SELECT count(*) FROM {table} WHERE classification != 'UNCLASSIFIED' OR (is_generic = 0 AND source_citation NOT LIKE '%http%')"),
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(bad, 0, "{table}");
    }
    let generic_ffg: i64 = c
        .query_row(
            "SELECT is_generic FROM platforms WHERE id = 'GEN-FFG'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(generic_ffg, 1);
}

#[test]
fn build_is_reproducible() {
    let dump = |c: &Connection| -> Vec<String> {
        let mut stmt = c
            .prepare(
                "SELECT p.id || ':' || p.max_speed_mps || ':' ||
                        coalesce((SELECT group_concat(m.id || m.name, ',') FROM mounts m WHERE m.platform_id = p.id), '')
                   FROM platforms p ORDER BY p.id",
            )
            .unwrap();
        stmt.query_map([], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    assert_eq!(dump(&built()), dump(&built()));
}

// ---------- every validation rule fires on a seeded fault ----------

/// (fault description, SQL that injects exactly that fault, rule that must report it)
const FAULTS: &[(&str, &str, &str)] = &[
    ("empty citation", "UPDATE sensors SET source_citation = '' WHERE id = 'SEN-VISUAL'", "provenance.citation_missing"),
    ("empty author", "UPDATE weapons SET created_by = ' ' WHERE id = 'WPN-CIWS'", "provenance.created_by_missing"),
    ("non-ISO date", "UPDATE platforms SET updated_at = '10/11/2026' WHERE id = 'GEN-FFG'", "provenance.updated_at_format"),
    ("classified record", "UPDATE weapons SET classification = 'SYNTHETIC-RESTRICTED' WHERE id = 'WPN-CIWS'", "classification.open_build"),
    ("non-generic record", "UPDATE platforms SET is_generic = 0 WHERE id = 'GEN-FFG'", "provenance.not_generic"),
    ("dangling reference", "INSERT INTO platform_comms (platform_id, comms_id) VALUES ('GEN-FFG', 'COM-DOES-NOT-EXIST')", "fk.dangling"),
    ("range beyond horizon", "UPDATE sensors SET max_range_m = 900000, ref_detect_range_m = 15000 WHERE id = 'SEN-RDR-NAV'", "sensor.beyond_horizon"),
    ("sensor range order", "UPDATE sensors SET min_range_m = max_range_m WHERE id = 'SEN-VISUAL'", "sensor.range_order"),
    ("half a radar reference", "UPDATE sensors SET ref_target_rcs_m2 = NULL WHERE id = 'SEN-RDR-NAV'", "sensor.reference_range"),
    ("radar field on a non-radar", "UPDATE sensors SET beamwidth_deg = 2 WHERE id = 'SEN-VISUAL'", "sensor.radar_fields"),
    ("sonar field on a non-sonar", "UPDATE sensors SET sonar_di_db = 10 WHERE id = 'SEN-VISUAL'", "sensor.sonar_fields"),
    ("ECM out of range", "UPDATE ecm_systems SET jn_db_at_1km = 500 WHERE id = 'ECM-GEN-SHIP'", "ecm.values"),
    ("radar without clutter figure", "UPDATE sensors SET clutter_db_per_sea_state = NULL WHERE id = 'SEN-RDR-NAV'", "sensor.radar_defaults"),
    ("radar without reference","UPDATE sensors SET ref_target_rcs_m2 = NULL, ref_detect_range_m = NULL WHERE id = 'SEN-RDR-NAV'", "sensor.radar_reference"),
    ("weapon range against a domain beyond its maximum", "UPDATE weapon_target_ranges SET max_range_m = 999999 WHERE weapon_id = 'WPN-GUN-127'", "weapon.target_ranges"),
    ("weapon range order", "UPDATE weapons SET min_range_m = max_range_m WHERE id = 'WPN-SAM-MR'", "weapon.range_order"),
    ("Ph above one", "UPDATE weapons SET base_ph = 1.5 WHERE id = 'WPN-SAM-MR'", "weapon.base_ph"),
    ("bad target domain", "UPDATE weapons SET target_domains = 'air,space' WHERE id = 'WPN-SAM-MR'", "weapon.target_domains"),
    ("negative weapon speed", "UPDATE weapons SET speed_mps = -1 WHERE id = 'WPN-SAM-MR'", "weapon.values"),
    ("weapon with no launcher", "DELETE FROM launcher_weapons WHERE weapon_id = 'WPN-ASM-SUB' AND mount_id IN (SELECT id FROM mounts WHERE platform_id = 'GEN-FFG')", "weapon.no_launcher"),
    ("unused weapon", "INSERT INTO weapons SELECT 'WPN-ORPHAN', name, kind, guidance, min_range_m, max_range_m, speed_mps, warhead_kg, base_ph, target_domains, source_citation, confidence, classification, is_generic, created_by, updated_at FROM weapons WHERE id = 'WPN-CIWS'", "weapon.unused"),
    ("empty launcher", "DELETE FROM launcher_weapons WHERE mount_id IN (SELECT id FROM mounts WHERE platform_id = 'GEN-OPV')", "launcher.empty"),
    ("launcher without ammunition", "DELETE FROM magazine_contents WHERE magazine_id IN (SELECT id FROM magazines WHERE platform_id = 'GEN-OPV')", "launcher.no_ammunition"),
    ("magazine over capacity", "UPDATE magazines SET capacity = 10 WHERE platform_id = 'GEN-FFG' AND name = 'VLS cells'", "magazine.over_capacity"),
    ("zero capacity", "UPDATE magazines SET capacity = 0 WHERE platform_id = 'GEN-OPV'", "magazine.capacity"),
    ("zero quantity", "UPDATE magazine_contents SET quantity = 0 WHERE weapon_id = 'WPN-GUN-30' AND magazine_id IN (SELECT id FROM magazines WHERE platform_id = 'GEN-OPV')", "magazine.quantity"),
    ("loadout over hardpoints", "UPDATE platforms SET hardpoints = 2 WHERE id = 'GEN-FTR-MR'", "loadout.over_hardpoints"),
    ("loadout without hardpoints", "UPDATE platforms SET hardpoints = NULL WHERE id = 'GEN-FTR-MR'", "loadout.no_hardpoints"),
    ("zero loadout quantity", "UPDATE loadout_items SET quantity = 0 WHERE loadout_id = 'GEN-FTR-AS/CAP' AND weapon_id = 'WPN-AAM-SR'", "loadout.quantity"),
    ("cruise above max", "UPDATE platforms SET cruise_speed_mps = max_speed_mps + 1 WHERE id = 'GEN-FFG'", "platform.speed"),
    ("no acceleration", "UPDATE platforms SET accel_mps2 = 0 WHERE id = 'GEN-FFG'", "platform.accel"),
    ("negative length", "UPDATE platforms SET length_m = -1 WHERE id = 'GEN-FFG'", "platform.dimensions"),
    ("submarine without depth", "UPDATE platforms SET max_depth_m = NULL WHERE id = 'GEN-SSK'", "platform.depth"),
    ("aircraft without ceiling", "UPDATE platforms SET max_altitude_m = NULL WHERE id = 'GEN-AEW'", "platform.altitude"),
    ("category in wrong domain", "UPDATE platforms SET domain = 'land' WHERE id = 'GEN-OPV'", "platform.category_domain"),
    ("damage fraction", "UPDATE damage_profiles SET mission_kill_fraction = 2 WHERE id = 'DMG-FRIGATE'", "damage.fractions"),
    ("negative signature", "UPDATE platform_signatures SET value = -1 WHERE platform_id = 'GEN-FFG' AND kind = 'rcs'", "signature.value"),
    ("missing RCS", "DELETE FROM platform_signatures WHERE platform_id = 'GEN-FFG' AND kind = 'rcs'", "platform.no_rcs"),
    ("missing acoustic", "DELETE FROM platform_signatures WHERE platform_id = 'GEN-SSK' AND kind = 'acoustic'", "platform.no_acoustic"),
];

#[test]
fn every_rule_detects_its_seeded_fault() {
    let mut failures = vec![];
    for (what, sql, rule) in FAULTS {
        let c = built();
        c.execute_batch(sql)
            .unwrap_or_else(|e| panic!("{what}: {e}"));
        let found = rules(&validate(&c, Policy::Open).unwrap());
        if !found.contains(rule) {
            failures.push(format!("{what}: expected {rule}, got {found:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn fault_table_covers_every_rule() {
    let source = include_str!("../src/validate.rs");
    let covered: BTreeSet<&str> = FAULTS.iter().map(|f| f.2).collect();
    // Rule ids are the second argument of `rows(...)` / `out.err(...)`: quoted, dotted, lowercase.
    let mut seen = BTreeSet::new();
    for chunk in source.split('"').skip(1).step_by(2) {
        let looks_like_rule = chunk.contains('.')
            && !chunk.starts_with('.')
            && !chunk.ends_with('.')
            && chunk
                .chars()
                .all(|c| c.is_ascii_lowercase() || c == '.' || c == '_');
        if looks_like_rule {
            assert!(
                covered.contains(chunk),
                "rule {chunk} has no seeded-fault test"
            );
            seen.insert(chunk);
        }
    }
    // Guard against the scan silently matching nothing: every tested rule must exist in the source.
    assert_eq!(
        seen, covered,
        "fault table names rules that validate.rs does not define"
    );
}

#[test]
fn accredited_policy_allows_classification_markings() {
    let c = built();
    c.execute_batch(
        "UPDATE weapons SET classification = 'SYNTHETIC-RESTRICTED' WHERE id = 'WPN-CIWS'",
    )
    .unwrap();
    assert!(rules(&validate(&c, Policy::Open).unwrap()).contains("classification.open_build"));
    assert!(validate(&c, Policy::Accredited).unwrap().is_empty());
}

#[test]
fn issues_are_sorted_errors_first() {
    let c = built();
    c.execute_batch(
        "DELETE FROM platform_signatures WHERE platform_id = 'GEN-FFG' AND kind = 'rcs';
         UPDATE weapons SET base_ph = 2 WHERE id = 'WPN-CIWS';",
    )
    .unwrap();
    let issues = validate(&c, Policy::Open).unwrap();
    assert_eq!(issues.first().map(|i| i.severity), Some(Severity::Error));
    assert_eq!(issues.last().map(|i| i.severity), Some(Severity::Warning));
}

// ---------- source format errors ----------

fn copy_reference_files(to: &Path) {
    for f in ["reference.json", "sensors.json", "weapons.json"] {
        std::fs::copy(open_source_dir().join(f), to.join(f)).unwrap();
    }
    std::fs::create_dir_all(to.join("platforms")).unwrap();
}

#[test]
fn unknown_fields_in_source_are_rejected() {
    let dir = temp_path("unknown-field");
    copy_reference_files(&dir);
    std::fs::write(
        dir.join("platforms/bad.json"),
        r#"{"defaults": {}, "platforms": [{"id": "X", "nme": "typo"}]}"#,
    )
    .unwrap();
    let mut c = Connection::open_in_memory().unwrap();
    assert!(matches!(
        source::build(&mut c, &dir),
        Err(DbError::Json { .. })
    ));
}

#[test]
fn missing_provenance_defaults_are_rejected() {
    let dir = temp_path("no-defaults");
    copy_reference_files(&dir);
    std::fs::write(
        dir.join("platforms/bad.json"),
        r#"{"defaults": {}, "platforms": [{"id": "X", "name": "x", "category": "merchant", "domain": "surface",
            "service": "GEN-CIVIL", "propulsion": "PROP-DIESEL", "damage_profile": "DMG-MERCHANT-SMALL",
            "max_speed_kn": 10, "cruise_speed_kn": 8, "accel_mps2": 0.05}]}"#,
    )
    .unwrap();
    let mut c = Connection::open_in_memory().unwrap();
    let err = source::build(&mut c, &dir).unwrap_err();
    assert!(matches!(err, DbError::Source { .. }), "{err}");
}

// ---------- loading through DbSource ----------

fn write_db(c: &Connection, name: &str) -> PathBuf {
    let path = temp_path(name).join("db.sqlite");
    let _ = std::fs::remove_file(&path);
    c.execute("VACUUM INTO ?1", [path.to_string_lossy()])
        .unwrap();
    path
}

#[test]
fn open_source_loads_and_answers_queries() {
    let path = write_db(&built(), "load");
    let db = PlatformDb::open(&DbSource::Open(path)).unwrap();
    let ffg = db.platform("GEN-FFG").unwrap().unwrap();
    // 28 kn = 28 × 1852 / 3600 m/s
    assert!((ffg.max_speed_mps - 28.0 * 1852.0 / 3600.0).abs() < 1e-9);
    assert_eq!(ffg.domain, "surface");
    assert!(db.platform("NOPE").unwrap().is_none());
    let all = db.platforms().unwrap();
    assert_eq!(all.len(), 113, "50 generic, 55 regional, 8 historical");
    assert!(all.windows(2).all(|w| w[0].id < w[1].id), "sorted by id");
}

#[test]
fn loading_refuses_future_and_outdated_schemas() {
    let c = built();
    c.pragma_update(None, "user_version", 99).unwrap();
    let future = write_db(&c, "future");
    assert!(matches!(
        PlatformDb::open(&DbSource::Open(future)),
        Err(DbError::FutureSchema { found: 99, .. })
    ));
    c.pragma_update(None, "user_version", 0).unwrap();
    let old = write_db(&c, "outdated");
    assert!(matches!(
        PlatformDb::open(&DbSource::Open(old)),
        Err(DbError::OutdatedSchema { found: 0, .. })
    ));
}

#[test]
fn open_build_refuses_a_database_with_classified_records() {
    let c = built();
    c.execute_batch(
        "UPDATE platforms SET classification = 'SYNTHETIC-RESTRICTED' WHERE id = 'GEN-FFG'",
    )
    .unwrap();
    let path = write_db(&c, "classified");
    assert!(matches!(
        PlatformDb::open(&DbSource::Open(path)),
        Err(DbError::Invalid { .. })
    ));
}

#[cfg(not(feature = "accredited"))]
#[test]
fn accredited_source_is_unavailable_in_the_open_build() {
    let path = write_db(&built(), "accredited-off");
    assert!(matches!(
        PlatformDb::open(&DbSource::Accredited(path)),
        Err(DbError::AccreditedUnavailable)
    ));
}

#[cfg(feature = "accredited")]
#[test]
fn accredited_source_accepts_synthetic_marked_records() {
    // Synthetic marking only: no real classified data exists in this repository.
    let c = built();
    c.execute_batch(
        "UPDATE platforms SET classification = 'SYNTHETIC-RESTRICTED', is_generic = 0 WHERE id = 'GEN-FFG'",
    )
    .unwrap();
    let path = write_db(&c, "accredited-on");
    let db = PlatformDb::open(&DbSource::Accredited(path)).unwrap();
    assert_eq!(db.policy(), Policy::Accredited);
    assert!(db.platform("GEN-FFG").unwrap().is_some());
}
