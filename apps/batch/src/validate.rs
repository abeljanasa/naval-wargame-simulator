//! Model validation against the historical record (Phase 4 slice 14, `docs/models/validation.md`).
//!
//! A **validation spec** names a scenario of a historical engagement, how many Monte Carlo runs to
//! make, and the measures to compare, each with what the record says happened:
//!
//! - `unit_lost`, `unit_survives`, `unit_disabled` (mission-killed or lost): yes or no;
//! - `losses` of a side, `weapons_fired` by a side (optionally of given weapons), `hits` by a side:
//!   counts;
//! - `first_hit_s` by a side: seconds from the start.
//!
//! For a yes/no measure the report gives the model's probability of the recorded outcome with a
//! 95 % Wilson interval; the record is **consistent** with the model when that probability is at
//! least 10 %, **unlikely** from 2 % to 10 %, and **inconsistent** below 2 %. For a count or a
//! time it gives the model's mean and 5–95 % range; the record is **consistent** inside the range
//! and **outside** it otherwise. These thresholds are original design and say how surprising the
//! record would be if the model were right; every difference is for the SMEs to explain (model
//! limitation, data gap or historical uncertainty).

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sim_core::{SideId, SimEventKind, Status, UnitId, Viewer};
use sim_scenario::Scenario;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

fn default_runs() -> u64 {
    200
}

/// A validation spec (`data/validation/*/validation.json`).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spec {
    pub format: String,
    pub name: String,
    /// The scenario, relative to the spec.
    pub scenario: PathBuf,
    #[serde(default = "default_runs")]
    pub runs: u64,
    /// "draft for SME review", "confirmed by SMEs", …
    pub status: String,
    #[serde(default)]
    pub summary: String,
    /// Where the record comes from.
    #[serde(default)]
    pub sources: Vec<String>,
    /// What the scenario assumes where the record is silent.
    #[serde(default)]
    pub assumptions: Vec<String>,
    pub measures: Vec<Measure>,
}

/// One measure and the record. (Unknown fields cannot be refused here: `deny_unknown_fields`
/// does not work with the flattened kind.)
#[derive(Debug, Clone, Deserialize)]
pub struct Measure {
    pub id: String,
    pub name: String,
    #[serde(flatten)]
    pub kind: Kind,
    pub record: Record,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Kind {
    UnitLost {
        unit: UnitId,
    },
    UnitSurvives {
        unit: UnitId,
    },
    UnitDisabled {
        unit: UnitId,
    },
    Losses {
        side: SideId,
    },
    WeaponsFired {
        side: SideId,
        #[serde(default)]
        weapons: Vec<String>,
    },
    Hits {
        side: SideId,
    },
    FirstHitS {
        side: SideId,
    },
}

/// What happened, per the record.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Record {
    Yes(bool),
    Value(f64),
}

impl Kind {
    fn yes_no(&self) -> bool {
        matches!(
            self,
            Self::UnitLost { .. } | Self::UnitSurvives { .. } | Self::UnitDisabled { .. }
        )
    }
}

/// One run's value of each measure (`None`: did not occur).
pub type RunValues = BTreeMap<String, Option<f64>>;

/// Measures a finished run.
pub fn measure(spec: &Spec, scenario: &Scenario, sim: &mut sim_core::Simulation) -> RunValues {
    let side_of: BTreeMap<UnitId, SideId> = scenario.units.iter().map(|u| (u.id, u.side)).collect();
    let events = sim.events().to_vec();
    let lost = |u: UnitId| {
        events
            .iter()
            .any(|e| matches!(e.kind, SimEventKind::UnitDestroyed { unit } if unit == u))
    };
    let snap = sim.snapshot(Viewer::WhiteCell);
    let disabled = |u: UnitId| {
        lost(u)
            || snap
                .units
                .iter()
                .any(|v| v.id == u && v.status != Status::Operational)
    };
    let hits_by = |side: SideId| {
        events.iter().filter_map(move |e| match &e.kind {
            SimEventKind::HitRoll {
                side: Some(s),
                hit: true,
                ..
            } if *s == side => Some(e.tick),
            _ => None,
        })
    };
    let flag = |b: bool| Some(if b { 1.0 } else { 0.0 });
    #[allow(clippy::cast_precision_loss)]
    let count = |n: usize| Some(n as f64);
    spec.measures
        .iter()
        .map(|m| {
            let v = match &m.kind {
                Kind::UnitLost { unit } => flag(lost(*unit)),
                Kind::UnitSurvives { unit } => flag(!lost(*unit)),
                Kind::UnitDisabled { unit } => flag(disabled(*unit)),
                Kind::Losses { side } => count(
                    events
                        .iter()
                        .filter(|e| match e.kind {
                            SimEventKind::UnitDestroyed { unit } => {
                                side_of.get(&unit) == Some(side)
                            }
                            _ => false,
                        })
                        .count(),
                ),
                Kind::WeaponsFired { side, weapons } => count(
                    events
                        .iter()
                        .filter(|e| match &e.kind {
                            SimEventKind::WeaponLaunched {
                                side: s, weapon, ..
                            } => s == side && (weapons.is_empty() || weapons.contains(weapon)),
                            _ => false,
                        })
                        .count(),
                ),
                Kind::Hits { side } => count(hits_by(*side).count()),
                #[allow(clippy::cast_precision_loss)]
                Kind::FirstHitS { side } => hits_by(*side).min().map(|t| t.0 as f64),
            };
            (m.id.clone(), v)
        })
        .collect()
}

/// How the record compares with the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Consistent,
    Unlikely,
    Inconsistent,
    Outside,
    NeverInModel,
}

impl Verdict {
    fn text(self) -> &'static str {
        match self {
            Self::Consistent => "consistent",
            Self::Unlikely => "unlikely",
            Self::Inconsistent => "inconsistent",
            Self::Outside => "outside the model's 90 % range",
            Self::NeverInModel => "never happened in the model",
        }
    }
}

/// One measure's result.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Comparison {
    pub id: String,
    pub name: String,
    pub record: Record,
    /// Runs in which the measure occurred.
    pub n: usize,
    /// Yes/no: the model's probability of the recorded outcome, with a 95 % Wilson interval.
    pub p_record: Option<f64>,
    pub p_ci95: Option<[f64; 2]>,
    /// Counts and times: mean and 5–95 % range.
    pub mean: Option<f64>,
    pub p5_p95: Option<[f64; 2]>,
    pub verdict: Verdict,
    pub note: String,
}

/// Wilson score interval for `k` successes in `n` trials at 95 %.
#[must_use]
pub fn wilson(k: usize, n: usize) -> [f64; 2] {
    if n == 0 {
        return [0.0, 1.0];
    }
    #[allow(clippy::cast_precision_loss)]
    let (k, n) = (k as f64, n as f64);
    let z = 1.96_f64;
    let p = k / n;
    let denom = 1.0 + z * z / n;
    let centre = (p + z * z / (2.0 * n)) / denom;
    let half = z * (p * (1.0 - p) / n + z * z / (4.0 * n * n)).sqrt() / denom;
    [(centre - half).max(0.0), (centre + half).min(1.0)]
}

/// The `q` quantile (0–1) of sorted values, nearest rank.
fn quantile(sorted: &[f64], q: f64) -> f64 {
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    let i = ((q * sorted.len() as f64).ceil() as usize).clamp(1, sorted.len()) - 1;
    sorted[i]
}

/// Compares the runs with the record, measure by measure.
#[must_use]
pub fn compare(spec: &Spec, runs: &[RunValues]) -> Vec<Comparison> {
    spec.measures
        .iter()
        .map(|m| {
            let mut vals: Vec<f64> = runs
                .iter()
                .filter_map(|r| r.get(&m.id).copied().flatten())
                .collect();
            vals.sort_by(f64::total_cmp);
            let n = vals.len();
            let base = Comparison {
                id: m.id.clone(),
                name: m.name.clone(),
                record: m.record,
                n,
                p_record: None,
                p_ci95: None,
                mean: None,
                p5_p95: None,
                verdict: Verdict::NeverInModel,
                note: m.note.clone(),
            };
            match (m.kind.yes_no(), m.record) {
                (true, Record::Yes(want)) => {
                    let target = if want { 1.0 } else { 0.0 };
                    let k = vals.iter().filter(|v| (**v - target).abs() < 0.5).count();
                    #[allow(clippy::cast_precision_loss)]
                    let p = if n == 0 { 0.0 } else { k as f64 / n as f64 };
                    let verdict = if p >= 0.10 {
                        Verdict::Consistent
                    } else if p >= 0.02 {
                        Verdict::Unlikely
                    } else {
                        Verdict::Inconsistent
                    };
                    Comparison {
                        p_record: Some(p),
                        p_ci95: Some(wilson(k, n)),
                        verdict,
                        ..base
                    }
                }
                (_, Record::Value(want)) if n > 0 => {
                    #[allow(clippy::cast_precision_loss)]
                    let mean = vals.iter().sum::<f64>() / n as f64;
                    let range = [quantile(&vals, 0.05), quantile(&vals, 0.95)];
                    let verdict = if want >= range[0] && want <= range[1] {
                        Verdict::Consistent
                    } else {
                        Verdict::Outside
                    };
                    Comparison {
                        mean: Some(mean),
                        p5_p95: Some(range),
                        verdict,
                        ..base
                    }
                }
                _ => base,
            }
        })
        .collect()
}

/// Loads a spec and its scenario.
pub fn load(path: &Path) -> Result<(Spec, Scenario)> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let spec: Spec =
        serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    if spec.format != "nws-validation" {
        bail!(
            "{}: not a validation spec (format {:?})",
            path.display(),
            spec.format
        );
    }
    for m in &spec.measures {
        match (m.kind.yes_no(), m.record) {
            (true, Record::Yes(_)) | (false, Record::Value(_)) => {}
            _ => bail!(
                "measure {}: its record is the wrong type for its kind",
                m.id
            ),
        }
    }
    let scen_path = path.parent().unwrap_or(Path::new(".")).join(&spec.scenario);
    let scenario =
        Scenario::load(&scen_path).with_context(|| format!("loading {}", scen_path.display()))?;
    Ok((spec, scenario))
}

fn record_text(r: Record) -> String {
    match r {
        Record::Yes(true) => "yes".into(),
        Record::Yes(false) => "no".into(),
        Record::Value(v) => format!("{v}"),
    }
}

/// The report, in Markdown.
#[must_use]
pub fn report_md(spec: &Spec, scenario: &Scenario, seeds: &[u64], rows: &[Comparison]) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "# Validation: {}\n", spec.name);
    let _ = writeln!(s, "> **Status: {}.**\n", spec.status);
    if !spec.summary.is_empty() {
        let _ = writeln!(s, "{}\n", spec.summary);
    }
    let _ = writeln!(
        s,
        "- **Scenario:** {} (`{}`)\n- **Runs:** {} (seeds {}–{})\n- **Method:** [docs/models/validation.md](../../../models/validation.md)\n",
        scenario.name,
        spec.scenario.display(),
        seeds.len(),
        seeds.first().copied().unwrap_or(0),
        seeds.last().copied().unwrap_or(0)
    );
    let _ = writeln!(s, "## Model against the record\n");
    let _ = writeln!(
        s,
        "| Measure | Record | Model | Verdict |\n|---|---|---|---|"
    );
    for r in rows {
        let model = match (r.p_record, r.mean) {
            (Some(p), _) => {
                let ci = r.p_ci95.unwrap_or([0.0, 1.0]);
                format!(
                    "P(record) {:.0} % (95 % CI {:.0}–{:.0} %)",
                    p * 100.0,
                    ci[0] * 100.0,
                    ci[1] * 100.0
                )
            }
            (_, Some(mean)) => {
                let rg = r.p5_p95.unwrap_or([mean, mean]);
                format!(
                    "mean {mean:.1} (5–95 %: {:.0}–{:.0}; n {})",
                    rg[0], rg[1], r.n
                )
            }
            _ => format!("did not occur (n {})", r.n),
        };
        let _ = writeln!(
            s,
            "| {} | {} | {} | {} |",
            r.name,
            record_text(r.record),
            model,
            r.verdict.text()
        );
    }
    let open: Vec<&Comparison> = rows
        .iter()
        .filter(|r| r.verdict != Verdict::Consistent)
        .collect();
    let _ = writeln!(s, "\n## Differences to explain\n");
    if open.is_empty() {
        let _ = writeln!(s, "None: every measure is consistent with the record.");
    } else {
        let _ = writeln!(
            s,
            "Each difference is for the SMEs to explain as a **model limitation**, a **data gap** or **historical uncertainty**.\n"
        );
        for r in open {
            let _ = writeln!(s, "- **{}** ({}): _to explain._", r.name, r.verdict.text());
            if !r.note.is_empty() {
                let _ = writeln!(s, "  - Note: {}", r.note);
            }
        }
    }
    if !spec.assumptions.is_empty() {
        let _ = writeln!(s, "\n## What the scenario assumes\n");
        for a in &spec.assumptions {
            let _ = writeln!(s, "- {a}");
        }
    }
    if !spec.sources.is_empty() {
        let _ = writeln!(s, "\n## Sources\n");
        for a in &spec.sources {
            let _ = writeln!(s, "- {a}");
        }
    }
    s
}

/// Writes `validation.json`, `report.md` and `runs.csv` to `out`.
pub fn write(
    out: &Path,
    spec: &Spec,
    scenario: &Scenario,
    seeds: &[u64],
    runs: &[RunValues],
    rows: &[Comparison],
) -> Result<()> {
    std::fs::create_dir_all(out).with_context(|| format!("creating {}", out.display()))?;
    let json = serde_json::json!({
        "name": spec.name, "status": spec.status, "scenario": scenario.name,
        "runs": seeds.len(), "seeds": [seeds.first(), seeds.last()], "measures": rows,
    });
    std::fs::write(
        out.join("validation.json"),
        serde_json::to_string_pretty(&json)? + "\n",
    )?;
    std::fs::write(
        out.join("report.md"),
        report_md(spec, scenario, seeds, rows),
    )?;
    let mut csv = String::from("seed");
    for m in &spec.measures {
        csv.push(',');
        csv.push_str(&m.id);
    }
    csv.push_str("\r\n");
    for (seed, r) in seeds.iter().zip(runs) {
        csv.push_str(&seed.to_string());
        for m in &spec.measures {
            csv.push(',');
            if let Some(v) = r.get(&m.id).copied().flatten() {
                csv.push_str(&format!("{v}"));
            }
        }
        csv.push_str("\r\n");
    }
    std::fs::write(out.join("runs.csv"), csv)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim_core::Tick;

    fn spec(measures: serde_json::Value) -> Spec {
        serde_json::from_value(serde_json::json!({
            "format": "nws-validation", "name": "t", "scenario": "s.json", "status": "draft",
            "measures": measures
        }))
        .unwrap()
    }

    #[test]
    fn wilson_interval_by_hand() {
        // 8 of 10: centre (0.8 + 0.19208)/1.38416 = 0.71675; half 1.96·√(0.016 + 0.009604)/1.38416
        // = 0.22658: 0.4902 to 0.9433.
        let [lo, hi] = wilson(8, 10);
        assert!((lo - 0.4902).abs() < 1e-3, "{lo}");
        assert!((hi - 0.9433).abs() < 1e-3, "{hi}");
        assert_eq!(wilson(0, 0), [0.0, 1.0]);
    }

    #[test]
    fn yes_no_verdicts_by_the_probability_of_the_record() {
        let s = spec(serde_json::json!([
            {"id": "a", "name": "A lost", "kind": "unit_lost", "unit": 1, "record": true},
            {"id": "b", "name": "B lost", "kind": "unit_lost", "unit": 2, "record": true},
            {"id": "c", "name": "C lost", "kind": "unit_lost", "unit": 3, "record": true}
        ]));
        // a: lost in 30 of 100 runs; b: 5 of 100; c: 1 of 100.
        let runs: Vec<RunValues> = (0..100)
            .map(|i| {
                [("a", i < 30), ("b", i < 5), ("c", i < 1)]
                    .into_iter()
                    .map(|(k, v)| (k.to_owned(), Some(if v { 1.0 } else { 0.0 })))
                    .collect()
            })
            .collect();
        let r = compare(&s, &runs);
        assert_eq!(r[0].verdict, Verdict::Consistent);
        assert!((r[0].p_record.unwrap() - 0.3).abs() < 1e-12);
        assert_eq!(r[1].verdict, Verdict::Unlikely);
        assert_eq!(r[2].verdict, Verdict::Inconsistent);
    }

    #[test]
    fn counts_compare_with_the_five_to_ninety_five_range() {
        let s = spec(serde_json::json!([
            {"id": "f", "name": "Fired", "kind": "weapons_fired", "side": 1, "record": 3},
            {"id": "g", "name": "Fired", "kind": "weapons_fired", "side": 1, "record": 30},
            {"id": "h", "name": "First hit", "kind": "first_hit_s", "side": 1, "record": 60}
        ]));
        let runs: Vec<RunValues> = (1..=20)
            .map(|i| {
                [
                    ("f".to_owned(), Some(f64::from(i))),
                    ("g".to_owned(), Some(f64::from(i))),
                    ("h".to_owned(), None),
                ]
                .into_iter()
                .collect()
            })
            .collect();
        let r = compare(&s, &runs);
        // 1..20: 5 % is 1 (rank 1), 95 % is 19 (rank 19).
        assert_eq!(r[0].p5_p95, Some([1.0, 19.0]));
        assert!((r[0].mean.unwrap() - 10.5).abs() < 1e-12);
        assert_eq!(r[0].verdict, Verdict::Consistent);
        assert_eq!(r[1].verdict, Verdict::Outside);
        assert_eq!(r[2].verdict, Verdict::NeverInModel);
    }

    #[test]
    fn a_record_of_the_wrong_type_is_refused() {
        let dir = std::env::temp_dir().join("nws-validate-wrong-type");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("validation.json");
        std::fs::write(
            &p,
            serde_json::json!({"format": "nws-validation", "name": "t", "scenario": "s.json",
                "status": "draft", "measures": [
                    {"id": "a", "name": "A", "kind": "unit_lost", "unit": 1, "record": 3}]})
            .to_string(),
        )
        .unwrap();
        let e = load(&p).unwrap_err().to_string();
        assert!(e.contains("wrong type"), "{e}");
    }

    /// The shipped engagements load and run (a handful of seeds; the full runs are in
    /// `docs/gate4/validation/`).
    #[test]
    fn the_shipped_engagements_load_and_run() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/validation");
        let db = sim_db::embedded::open_platform_db().unwrap();
        for name in ["hangor-khukri-1971", "praying-mantis-1988"] {
            let (spec, scenario) = load(&root.join(name).join("validation.json")).unwrap();
            let mut runs = Vec::new();
            for seed in 1..=2 {
                let mut sim = scenario.build(Some(seed), &db).unwrap();
                sim.run_until(Tick(scenario.duration_ticks().0.min(1_800)));
                runs.push(measure(&spec, &scenario, &mut sim));
            }
            let rows = compare(&spec, &runs);
            assert_eq!(rows.len(), spec.measures.len(), "{name}");
            assert!(
                report_md(&spec, &scenario, &[1, 2], &rows).contains("Model against the record")
            );
        }
    }
}
