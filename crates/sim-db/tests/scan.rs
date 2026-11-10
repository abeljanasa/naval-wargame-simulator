//! Release scan tests (master prompt §3). Synthetic markings only; no real classified data.

#![allow(clippy::unwrap_used)] // test harness

use rusqlite::Connection;
use sim_db::scan::{read_allowlist, scan};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn fresh_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nws-scan-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("nested")).unwrap();
    dir
}

fn write_db(path: &Path, tamper: Option<&str>) {
    let mut c = Connection::open_in_memory().unwrap();
    sim_db::source::build_set(&mut c, sim_db::embedded::source_set()).unwrap();
    if let Some(sql) = tamper {
        c.execute_batch(sql).unwrap();
    }
    c.execute("VACUUM INTO ?1", [path.to_string_lossy()])
        .unwrap();
}

fn allow() -> BTreeSet<String> {
    ["open-platforms.sqlite".to_owned()].into()
}

#[test]
fn clean_allowlisted_database_passes() {
    let dir = fresh_dir("clean");
    write_db(&dir.join("nested/open-platforms.sqlite"), None);
    std::fs::write(
        dir.join("notes.json"),
        r#"{"classification": "UNCLASSIFIED"}"#,
    )
    .unwrap();
    assert!(scan(&dir, &allow()).unwrap().is_empty());
}

#[test]
fn unlisted_database_fails_even_when_renamed() {
    let dir = fresh_dir("renamed");
    // A database disguised with an innocent extension is still recognised by its header.
    write_db(&dir.join("nested/readme.txt"), None);
    let f = scan(&dir, &allow()).unwrap();
    assert_eq!(f.len(), 1, "{f:?}");
    assert!(f[0].problem.contains("allowlist"));
}

#[test]
fn non_unclassified_records_fail_even_when_allowlisted() {
    let dir = fresh_dir("marked");
    write_db(
        &dir.join("open-platforms.sqlite"),
        Some("UPDATE sensors SET classification = 'SYNTHETIC-RESTRICTED' WHERE id = 'SEN-VISUAL'"),
    );
    let f = scan(&dir, &allow()).unwrap();
    assert_eq!(f.len(), 1, "{f:?}");
    assert!(f[0].problem.contains("SYNTHETIC-RESTRICTED"));
}

#[test]
fn json_classification_fields_are_checked_at_any_depth() {
    let dir = fresh_dir("json");
    std::fs::write(
        dir.join("nested/data.json"),
        r#"{"items": [{"x": 1}, {"meta": {"classification": "SYNTHETIC-RESTRICTED"}}]}"#,
    )
    .unwrap();
    let f = scan(&dir, &allow()).unwrap();
    assert_eq!(f.len(), 1, "{f:?}");
    assert!(f[0].problem.contains("/items[1]/meta/classification"));
}

#[test]
fn foreign_sqlite_file_is_reported() {
    let dir = fresh_dir("foreign");
    let c = Connection::open(dir.join("open-platforms.sqlite")).unwrap();
    c.execute_batch("CREATE TABLE t (x INTEGER); INSERT INTO t VALUES (1);")
        .unwrap();
    drop(c);
    let f = scan(&dir, &allow()).unwrap();
    assert!(!f.is_empty());
    assert!(f
        .iter()
        .all(|x| x.problem.contains("not an open platform database")));
}

#[test]
fn shipped_data_tree_and_allowlist_are_clean() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
    let allow = read_allowlist(&root.join("db/release-allowlist.txt")).unwrap();
    assert!(allow.contains("open-platforms.sqlite"));
    assert!(scan(&root, &allow).unwrap().is_empty());
}
