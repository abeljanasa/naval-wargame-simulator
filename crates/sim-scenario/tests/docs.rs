//! The JSON examples in the scripting reference (`docs/manuals/scripting-reference.md`) are
//! valid: each parses as the engine type it shows, so the manual cannot drift from the format.

#![allow(clippy::unwrap_used, clippy::expect_used)] // test harness

use serde_json::Value;
use std::path::Path;

fn examples() -> Vec<Value> {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/manuals/scripting-reference.md");
    let text = std::fs::read_to_string(path).unwrap();
    let mut out = Vec::new();
    let mut block: Option<String> = None;
    for line in text.lines() {
        match (&mut block, line.trim_start()) {
            (None, "```json") => block = Some(String::new()),
            (Some(b), "```") => {
                out.push(serde_json::from_str(b).unwrap_or_else(|e| panic!("{e}: {b}")));
                block = None;
            }
            (Some(b), _) => {
                b.push_str(line);
                b.push('\n');
            }
            (None, _) => {}
        }
    }
    out
}

#[test]
fn every_json_example_in_the_scripting_reference_parses() {
    let all = examples();
    assert!(all.len() >= 6, "found {} examples", all.len());
    let mut kinds = Vec::new();
    for v in all {
        let has = |k: &str| v.get(k).is_some();
        let kind = if has("trigger") {
            serde_json::from_value::<sim_core::ScenarioEvent>(v.clone()).map(|_| "event")
        } else if has("leader") {
            serde_json::from_value::<sim_scenario::ActionGroup>(v.clone()).map(|_| "action group")
        } else if has("area") {
            serde_json::from_value::<sim_core::Zone>(v.clone()).map(|_| "zone")
        } else if has("kind") && has("radius_m") {
            serde_json::from_value::<sim_core::Facility>(v.clone()).map(|_| "facility")
        } else if has("title") {
            serde_json::from_value::<sim_core::Inject>(v.clone()).map(|_| "inject")
        } else if has("platform") {
            serde_json::from_value::<sim_scenario::ScenarioUnit>(v.clone()).map(|_| "unit")
        } else {
            panic!("an example of no known type: {v}");
        };
        kinds.push(kind.unwrap_or_else(|e| panic!("{e}: {v}")));
    }
    for k in [
        "event",
        "action group",
        "zone",
        "facility",
        "inject",
        "unit",
    ] {
        assert!(kinds.contains(&k), "no {k} example: {kinds:?}");
    }
}
