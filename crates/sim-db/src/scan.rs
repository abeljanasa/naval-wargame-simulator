//! Release scan (master prompt §3): fails a release whose artefacts contain
//!
//! * a database file not on the open-data allowlist, or
//! * any record or JSON field whose classification is not `UNCLASSIFIED`.
//!
//! Database files are recognised by the SQLite header bytes, not just by extension, so a renamed
//! file is still caught. JSON files are parsed, and every `"classification"` key at any depth is
//! checked.

use crate::error::DbError;
use crate::{OPEN_CLASSIFICATION, PROVENANCE_TABLES};
use rusqlite::{Connection, OpenFlags};
use std::collections::BTreeSet;
use std::io::Read;
use std::path::{Path, PathBuf};

const SQLITE_HEADER: &[u8; 16] = b"SQLite format 3\0";

/// One reason the scanned tree must not be released.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Finding {
    pub path: PathBuf,
    pub problem: String,
}

impl std::fmt::Display for Finding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.problem)
    }
}

/// Reads an allowlist: one file name per line; blank lines and `#` comments are ignored.
pub fn read_allowlist(path: &Path) -> Result<BTreeSet<String>, DbError> {
    let text = std::fs::read_to_string(path).map_err(|source| DbError::Io {
        path: path.to_owned(),
        source,
    })?;
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_owned)
        .collect())
}

fn is_sqlite(path: &Path) -> bool {
    let mut buf = [0u8; 16];
    std::fs::File::open(path)
        .and_then(|mut f| f.read_exact(&mut buf))
        .is_ok_and(|()| &buf == SQLITE_HEADER)
}

fn files_under(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), DbError> {
    let entries = std::fs::read_dir(dir).map_err(|source| DbError::Io {
        path: dir.to_owned(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| DbError::Io {
            path: dir.to_owned(),
            source,
        })?;
        let path = entry.path();
        if path.is_dir() {
            files_under(&path, out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}

fn scan_database(path: &Path, findings: &mut Vec<Finding>) {
    let mut report = |problem: String| {
        findings.push(Finding {
            path: path.to_owned(),
            problem,
        });
    };
    let conn = match Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY) {
        Ok(c) => c,
        Err(e) => return report(format!("cannot open database: {e}")),
    };
    for table in PROVENANCE_TABLES {
        let sql = format!(
            "SELECT count(*), group_concat(DISTINCT classification) FROM {table} WHERE classification != ?1"
        );
        match conn.query_row(&sql, [OPEN_CLASSIFICATION], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?))
        }) {
            Ok((0, _)) => {}
            Ok((n, marks)) => report(format!(
                "{n} record(s) in `{table}` marked {}",
                marks.unwrap_or_default()
            )),
            Err(e) => report(format!(
                "cannot check `{table}` ({e}); not an open platform database"
            )),
        }
    }
}

fn walk_json(v: &serde_json::Value, at: &str, path: &Path, findings: &mut Vec<Finding>) {
    match v {
        serde_json::Value::Object(map) => {
            for (k, child) in map {
                let here = format!("{at}/{k}");
                if k == "classification" && child.as_str() != Some(OPEN_CLASSIFICATION) {
                    findings.push(Finding {
                        path: path.to_owned(),
                        problem: format!("{here} = {child}"),
                    });
                }
                walk_json(child, &here, path, findings);
            }
        }
        serde_json::Value::Array(items) => {
            for (i, child) in items.iter().enumerate() {
                walk_json(child, &format!("{at}[{i}]"), path, findings);
            }
        }
        _ => {}
    }
}

/// Scans every file under `dir`. An empty result means the tree is clear to release.
pub fn scan(dir: &Path, allowlist: &BTreeSet<String>) -> Result<Vec<Finding>, DbError> {
    let mut files = Vec::new();
    files_under(dir, &mut files)?;
    files.sort();
    let mut findings = Vec::new();
    for path in &files {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if is_sqlite(path) {
            if !allowlist.contains(&name) {
                findings.push(Finding {
                    path: path.clone(),
                    problem: "database file is not on the open-data allowlist".into(),
                });
            }
            scan_database(path, &mut findings);
        } else if path.extension().is_some_and(|e| e == "json") {
            // Unparsable JSON is not data we ship as a database; skip it.
            if let Ok(text) = std::fs::read_to_string(path) {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                    walk_json(&v, "", path, &mut findings);
                }
            }
        }
    }
    findings.sort();
    Ok(findings)
}
