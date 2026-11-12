//! Editing platform records in the JSON source (ADR-0004: the JSON is the source of truth; a GUI
//! editor edits it, not the built database).
//!
//! A record is found by id and replaced as text, so the rest of its file keeps its layout and a
//! data review sees a one-record diff. The edited source set is checked by building it in memory
//! and running the full validation suite, the same checks as `sim-db validate`.

use crate::error::DbError;
use crate::source::{build_set, SourceFile, SourceSet};
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

/// Which kind of record, and so which source file(s) hold it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordKind {
    /// `platforms/*.json`
    Platform,
    /// `sensors.json`
    Sensor,
    /// `weapons.json`
    Weapon,
}

impl RecordKind {
    /// The kind from its name: `platform`, `sensor` or `weapon`.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "platform" => Some(Self::Platform),
            "sensor" => Some(Self::Sensor),
            "weapon" => Some(Self::Weapon),
            _ => None,
        }
    }

    /// The array that holds this kind's records in its files.
    fn key(self) -> &'static str {
        match self {
            Self::Platform => "platforms",
            Self::Sensor => "sensors",
            Self::Weapon => "weapons",
        }
    }
}

/// The files that hold `kind`, with the platform file's index (`None` for sensors and weapons).
fn files(set: &SourceSet, kind: RecordKind) -> Vec<(Option<usize>, &SourceFile)> {
    match kind {
        RecordKind::Platform => set
            .platforms
            .iter()
            .enumerate()
            .map(|(i, f)| (Some(i), f))
            .collect(),
        RecordKind::Sensor => vec![(None, &set.sensors)],
        RecordKind::Weapon => vec![(None, &set.weapons)],
    }
}

/// The span of record `id` in a source file's text: the outermost object whose `id` is `id`.
/// Objects nested in a record (an aircraft's loadouts) start after it, so the record wins.
fn record_span(text: &str, id: &str) -> Option<(usize, usize)> {
    let needle = format!("\"{id}\"");
    object_spans(text).into_iter().find(|&(s, e)| {
        let slice = &text[s..e];
        slice.contains(&needle)
            && serde_json::from_str::<Value>(slice)
                .is_ok_and(|v| v.get("id").and_then(Value::as_str) == Some(id))
    })
}

/// The authored text of record `id` of `kind`, or `None` if its files do not have it.
#[must_use]
pub fn record(set: &SourceSet, kind: RecordKind, id: &str) -> Option<RecordText> {
    files(set, kind).into_iter().find_map(|(_, f)| {
        record_span(&f.text, id).map(|(s, e)| RecordText {
            file: f.name.clone(),
            text: f.text[s..e].to_owned(),
        })
    })
}

/// The authored text of platform record `id`.
#[must_use]
pub fn platform_record(set: &SourceSet, id: &str) -> Option<RecordText> {
    record(set, RecordKind::Platform, id)
}

/// Every record of `kind` as (id, name), in file order. A file that does not parse is skipped:
/// the editor lists what loads, and the check reports the rest.
#[must_use]
pub fn record_ids(set: &SourceSet, kind: RecordKind) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (_, f) in files(set, kind) {
        let Ok(v) = serde_json::from_str::<Value>(&f.text) else {
            continue;
        };
        for r in v
            .get(kind.key())
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(id) = r.get("id").and_then(Value::as_str) {
                let name = r.get("name").and_then(Value::as_str).unwrap_or_default();
                out.push((id.to_owned(), name.to_owned()));
            }
        }
    }
    out
}

/// Replaces record `id` of `kind` with `new_text` (which may change its id) and returns the new
/// source set and the name of the file that changed. Only that record's text changes. The result
/// is not validated here; call [`check`].
pub fn replace_record(
    set: &SourceSet,
    kind: RecordKind,
    id: &str,
    new_text: &str,
) -> Result<(SourceSet, String), EditError> {
    let value: Value =
        serde_json::from_str(new_text).map_err(|e| EditError::Parse(e.to_string()))?;
    if !value.is_object() || value.get("id").and_then(Value::as_str).is_none() {
        return Err(EditError::NotARecord);
    }
    let found = files(set, kind)
        .into_iter()
        .find_map(|(i, f)| record_span(&f.text, id).map(|span| (i, span)));
    let Some((index, (start, end))) = found else {
        return Err(EditError::NotFound(id.to_owned()));
    };
    let mut out = set.clone();
    let name = {
        let file = match (kind, index) {
            (RecordKind::Platform, Some(i)) => &mut out.platforms[i],
            (RecordKind::Sensor, _) => &mut out.sensors,
            _ => &mut out.weapons,
        };
        file.text.replace_range(start..end, new_text.trim());
        file.name.clone()
    };
    Ok((out, name))
}

/// [`replace_record`] for a platform.
pub fn replace_platform(
    set: &SourceSet,
    id: &str,
    new_text: &str,
) -> Result<(SourceSet, String), EditError> {
    replace_record(set, RecordKind::Platform, id, new_text)
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

    #[test]
    fn sensors_and_weapons_are_edited_in_their_own_files() {
        let set = source_set();
        let sensors = record_ids(&set, RecordKind::Sensor);
        let weapons = record_ids(&set, RecordKind::Weapon);
        assert!(sensors.len() > 20 && weapons.len() > 20);
        assert!(sensors
            .iter()
            .any(|(id, name)| id == "SEN-RDR-NAV" && !name.is_empty()));
        // The platform listing holds platforms only, not an aircraft's loadouts.
        let platforms = record_ids(&set, RecordKind::Platform);
        assert!(platforms.iter().all(|(id, _)| id.starts_with("GEN-")));

        let w = record(&set, RecordKind::Weapon, "WPN-GUN-30").unwrap();
        assert_eq!(w.file, "weapons.json");
        let heavier = w
            .text
            .replacen("\"warhead_kg\": 0.4", "\"warhead_kg\": 0.5", 1);
        assert_ne!(heavier, w.text);
        let (edited, file) =
            replace_record(&set, RecordKind::Weapon, "WPN-GUN-30", &heavier).unwrap();
        assert_eq!(file, "weapons.json");
        assert!(check(edited)
            .unwrap()
            .iter()
            .all(|i| i.severity != Severity::Error));

        let r = record(&set, RecordKind::Sensor, "SEN-RDR-NAV").unwrap();
        assert_eq!(r.file, "sensors.json");
        let typo = r.text.replacen("\"max_range_km\"", "\"max_rnge_km\"", 1);
        let (edited, _) = replace_record(&set, RecordKind::Sensor, "SEN-RDR-NAV", &typo).unwrap();
        assert!(check(edited)
            .unwrap_err()
            .to_string()
            .contains("max_rnge_km"));
        // A sensor id is not a weapon.
        assert!(record(&set, RecordKind::Weapon, "SEN-RDR-NAV").is_none());
        assert_eq!(RecordKind::parse("weapon"), Some(RecordKind::Weapon));
        assert_eq!(RecordKind::parse("ship"), None);
    }
}
