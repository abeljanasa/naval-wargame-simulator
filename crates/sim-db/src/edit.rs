//! Editing platform records in the JSON source (ADR-0004: the JSON is the source of truth; a GUI
//! editor edits it, not the built database).
//!
//! A record is found by id and replaced as text, so the rest of its file keeps its layout and a
//! data review sees a one-record diff. The edited source set is checked by building it in memory
//! and running the full validation suite, the same checks as `sim-db validate`.

use crate::error::DbError;
use crate::source::{build_set, SourceSet};
use crate::validate::{validate, Issue, Policy};
use rusqlite::Connection;
use serde_json::Value;

/// A platform record as authored: its file and its exact text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordText {
    /// The file's name in the source tree, e.g. `platforms/surface-combatants.json`.
    pub file: String,
    pub text: String,
}

/// Why an edit could not be applied.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EditError {
    #[error("no platform record with id {0:?}")]
    NotFound(String),
    #[error("the record is not valid JSON: {0}")]
    Parse(String),
    #[error("the record must be a JSON object with a text \"id\"")]
    NotARecord,
}

/// Byte spans `[start, end)` of every `{ … }` object in `text`, ordered by start (an enclosing
/// object comes before the objects inside it). Strings, and braces inside them, are skipped.
fn object_spans(text: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut open = Vec::new();
    let mut in_string = false;
    let mut escaped = false;
    for (i, b) in text.bytes().enumerate() {
        if in_string {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_string = false;
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'{' => open.push(i),
            b'}' => {
                if let Some(start) = open.pop() {
                    spans.push((start, i + 1));
                }
            }
            _ => {}
        }
    }
    spans.sort_unstable();
    spans
}

/// The span of the platform record `id` in a platform file's text: the outermost object whose
/// `id` is `id` and that has a `category` (the platform fields; nested objects have no id).
fn record_span(text: &str, id: &str) -> Option<(usize, usize)> {
    let needle = format!("\"{id}\"");
    object_spans(text).into_iter().find(|&(s, e)| {
        let slice = &text[s..e];
        slice.contains(&needle)
            && serde_json::from_str::<Value>(slice).is_ok_and(|v| {
                v.get("id").and_then(Value::as_str) == Some(id) && v.get("category").is_some()
            })
    })
}

/// The authored text of platform record `id`, or `None` if no platform file has it.
#[must_use]
pub fn platform_record(set: &SourceSet, id: &str) -> Option<RecordText> {
    set.platforms.iter().find_map(|f| {
        record_span(&f.text, id).map(|(s, e)| RecordText {
            file: f.name.clone(),
            text: f.text[s..e].to_owned(),
        })
    })
}

/// Replaces platform record `id` with `new_text` (which may change its id) and returns the new
/// source set and the name of the file that changed. Only that record's text changes. The result
/// is not validated here; call [`check`].
pub fn replace_platform(
    set: &SourceSet,
    id: &str,
    new_text: &str,
) -> Result<(SourceSet, String), EditError> {
    let value: Value =
        serde_json::from_str(new_text).map_err(|e| EditError::Parse(e.to_string()))?;
    if !value.is_object() || value.get("id").and_then(Value::as_str).is_none() {
        return Err(EditError::NotARecord);
    }
    let found = set
        .platforms
        .iter()
        .enumerate()
        .find_map(|(i, f)| record_span(&f.text, id).map(|span| (i, span)));
    let Some((i, (start, end))) = found else {
        return Err(EditError::NotFound(id.to_owned()));
    };
    let mut out = set.clone();
    out.platforms[i]
        .text
        .replace_range(start..end, new_text.trim());
    let file = out.platforms[i].name.clone();
    Ok((out, file))
}

/// Builds `set` in memory and runs the open build's validation suite. A file that does not load
/// (bad JSON, an unknown field, a wrong type) is an `Err` naming the file and the problem.
pub fn check(set: SourceSet) -> Result<Vec<Issue>, DbError> {
    let mut conn = Connection::open_in_memory()?;
    build_set(&mut conn, set)?;
    validate(&conn, Policy::Open)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embedded::source_set;
    use crate::validate::Severity;

    #[test]
    fn spans_skip_braces_in_strings_and_order_outer_first() {
        let t = r#"{"a": "}{", "b": {"c": 1}}"#;
        assert_eq!(object_spans(t), vec![(0, t.len()), (17, 25)]);
    }

    #[test]
    fn finds_a_record_as_authored() {
        let r = platform_record(&source_set(), "GEN-FFG").unwrap();
        assert_eq!(r.file, "platforms/surface-combatants.json");
        let v: Value = serde_json::from_str(&r.text).unwrap();
        assert_eq!(v["id"], "GEN-FFG");
        // The anti-submarine frigate is a different record, not a match on a prefix.
        let asw = platform_record(&source_set(), "GEN-FFG-ASW").unwrap();
        assert_ne!(asw.text, r.text);
        assert!(platform_record(&source_set(), "GEN-NONE").is_none());
    }

    #[test]
    fn an_edit_changes_only_its_record_and_checks_clean() {
        let set = source_set();
        let old = platform_record(&set, "GEN-OPV").unwrap();
        let new = old
            .text
            .replacen("\"max_speed_kn\": 21", "\"max_speed_kn\": 22", 1);
        assert_ne!(new, old.text, "the test edits a real field");
        let (edited, file) = replace_platform(&set, "GEN-OPV", &new).unwrap();
        assert_eq!(file, old.file);
        let before = &set.platforms.iter().find(|f| f.name == file).unwrap().text;
        let after = &edited
            .platforms
            .iter()
            .find(|f| f.name == file)
            .unwrap()
            .text;
        let at = before.find(&old.text).unwrap();
        assert_eq!(
            &after[..at],
            &before[..at],
            "text before the record is unchanged"
        );
        assert_eq!(
            &after[at + new.len()..],
            &before[at + old.text.len()..],
            "text after the record is unchanged"
        );
        let issues = check(edited).unwrap();
        assert!(
            issues.iter().all(|i| i.severity != Severity::Error),
            "{issues:?}"
        );
    }

    #[test]
    fn bad_edits_are_refused_or_reported() {
        let set = source_set();
        assert!(matches!(
            replace_platform(&set, "GEN-OPV", "{ not json"),
            Err(EditError::Parse(_))
        ));
        assert!(matches!(
            replace_platform(&set, "GEN-OPV", "[1, 2]"),
            Err(EditError::NotARecord)
        ));
        assert!(matches!(
            replace_platform(&set, "GEN-NONE", r#"{"id": "X"}"#),
            Err(EditError::NotFound(_))
        ));
        // An unknown field does not load: the check names the problem.
        let old = platform_record(&set, "GEN-OPV").unwrap().text;
        let typo = old.replacen("\"max_speed_kn\"", "\"max_sped_kn\"", 1);
        let (edited, _) = replace_platform(&set, "GEN-OPV", &typo).unwrap();
        let err = check(edited).unwrap_err().to_string();
        assert!(err.contains("max_sped_kn"), "{err}");
        // A dangling reference loads but fails validation: a missing row, and the edited
        // record's magazine left with nothing to fire it.
        let dangling = old.replacen("\"WPN-GUN-30\"", "\"WPN-NONE\"", 1);
        assert_ne!(dangling, old);
        let (edited, _) = replace_platform(&set, "GEN-OPV", &dangling).unwrap();
        let issues = check(edited).unwrap();
        assert!(issues.iter().any(|i| i.rule == "fk.dangling"), "{issues:?}");
        assert!(
            issues
                .iter()
                .any(|i| i.severity == Severity::Error && i.location.contains("GEN-OPV")),
            "{issues:?}"
        );
    }
}
