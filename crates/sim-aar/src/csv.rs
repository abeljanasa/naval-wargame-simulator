//! CSV export of the AAR tables (RFC 4180: comma-separated, CRLF line ends, fields quoted when
//! they hold a comma, quote or line break).

use crate::report::{AarReport, TimelineKind};
use serde::{Deserialize, Serialize};
use sim_core::{SideId, Tick};

/// The tables of an [`AarReport`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum AarTable {
    Sides,
    Losses,
    Weapons,
    Detections,
    Transits,
    Sorties,
    Timeline,
}

impl AarTable {
    pub const ALL: [AarTable; 7] = [
        AarTable::Sides,
        AarTable::Losses,
        AarTable::Weapons,
        AarTable::Detections,
        AarTable::Transits,
        AarTable::Sorties,
        AarTable::Timeline,
    ];

    /// File stem for an export (`losses` → `losses.csv`).
    #[must_use]
    pub fn file_stem(self) -> &'static str {
        match self {
            AarTable::Sides => "sides",
            AarTable::Losses => "losses",
            AarTable::Weapons => "weapons",
            AarTable::Detections => "detections",
            AarTable::Transits => "transits",
            AarTable::Sorties => "sorties",
            AarTable::Timeline => "timeline",
        }
    }
}

fn field(s: &str) -> String {
    // A leading = + - @ would be run as a formula by spreadsheet programs; prefix a quote.
    let s = if s.starts_with(['=', '+', '-', '@']) && s.parse::<f64>().is_err() {
        format!("'{s}")
    } else {
        s.to_owned()
    };
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s
    }
}

fn line(out: &mut String, fields: &[String]) {
    let row: Vec<String> = fields.iter().map(|f| field(f)).collect();
    out.push_str(&row.join(","));
    out.push_str("\r\n");
}

/// `hh:mm:ss` from the start.
#[must_use]
pub fn clock(t: Tick) -> String {
    let s = t.0;
    format!("{:02}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
}

fn side(s: Option<SideId>) -> String {
    s.map_or_else(String::new, |s| s.0.to_string())
}

fn kind(k: TimelineKind) -> &'static str {
    match k {
        TimelineKind::Order => "order",
        TimelineKind::Refused => "refused",
        TimelineKind::Contact => "contact",
        TimelineKind::Launch => "launch",
        TimelineKind::Hit => "hit",
        TimelineKind::Miss => "miss",
        TimelineKind::Intercept => "intercept",
        TimelineKind::Damage => "damage",
        TimelineKind::Loss => "loss",
        TimelineKind::Mine => "mine",
        TimelineKind::Air => "air",
        TimelineKind::Tasking => "tasking",
        TimelineKind::Override => "override",
        TimelineKind::Facilitation => "facilitation",
        TimelineKind::Event => "event",
        TimelineKind::ScriptError => "script_error",
    }
}

impl AarReport {
    /// One table as CSV text, with a header row.
    #[must_use]
    pub fn csv(&self, table: AarTable) -> String {
        let mut out = String::new();
        let h = |cols: &[&str]| cols.iter().map(|c| (*c).to_owned()).collect::<Vec<_>>();
        match table {
            AarTable::Sides => {
                line(
                    &mut out,
                    &h(&[
                        "side",
                        "name",
                        "units_start",
                        "reinforcements",
                        "destroyed",
                        "removed",
                        "remaining",
                        "weapons_fired",
                        "hits",
                        "kills",
                    ]),
                );
                for s in &self.sides {
                    line(
                        &mut out,
                        &[
                            s.side.0.to_string(),
                            s.name.clone(),
                            s.units_start.to_string(),
                            s.reinforcements.to_string(),
                            s.destroyed.to_string(),
                            s.removed.to_string(),
                            s.remaining.to_string(),
                            s.weapons_fired.to_string(),
                            s.hits.to_string(),
                            s.kills.to_string(),
                        ],
                    );
                }
            }
            AarTable::Losses => {
                line(
                    &mut out,
                    &h(&[
                        "time",
                        "tick",
                        "unit",
                        "name",
                        "side",
                        "side_name",
                        "platform",
                        "removed",
                        "cause",
                        "by_side",
                    ]),
                );
                for l in &self.losses {
                    line(
                        &mut out,
                        &[
                            clock(l.tick),
                            l.tick.0.to_string(),
                            l.unit.0.to_string(),
                            l.name.clone(),
                            l.side.0.to_string(),
                            l.side_name.clone(),
                            l.platform.clone().unwrap_or_default(),
                            l.removed.to_string(),
                            l.cause.clone(),
                            side(l.by_side),
                        ],
                    );
                }
            }
            AarTable::Weapons => {
                line(
                    &mut out,
                    &h(&[
                        "side",
                        "side_name",
                        "weapon",
                        "defensive",
                        "fired",
                        "hits",
                        "missed",
                        "intercepted",
                        "kills",
                    ]),
                );
                for w in &self.weapons {
                    line(
                        &mut out,
                        &[
                            w.side.0.to_string(),
                            w.side_name.clone(),
                            w.weapon.clone(),
                            w.defensive.to_string(),
                            w.fired.to_string(),
                            w.hits.to_string(),
                            w.missed.to_string(),
                            w.intercepted.to_string(),
                            w.kills.to_string(),
                        ],
                    );
                }
            }
            AarTable::Detections => {
                line(
                    &mut out,
                    &h(&[
                        "side",
                        "side_name",
                        "contact",
                        "target",
                        "target_name",
                        "target_side",
                        "gained",
                        "lost",
                        "held_s",
                    ]),
                );
                for d in &self.detections {
                    line(
                        &mut out,
                        &[
                            d.side.0.to_string(),
                            d.side_name.clone(),
                            d.contact.to_string(),
                            d.target.0.to_string(),
                            d.target_name.clone(),
                            side(d.target_side),
                            clock(d.gained),
                            d.lost.map(clock).unwrap_or_default(),
                            format!("{:.0}", d.held_s),
                        ],
                    );
                }
            }
            AarTable::Sorties => {
                line(
                    &mut out,
                    &h(&[
                        "launched",
                        "unit",
                        "unit_name",
                        "side",
                        "base",
                        "mission",
                        "loadout",
                        "ended",
                        "flight_min",
                        "outcome",
                        "tanker_fuel_t",
                    ]),
                );
                for r in &self.sorties {
                    let until = r.ended.unwrap_or(self.end_tick);
                    line(
                        &mut out,
                        &[
                            clock(r.launched),
                            r.unit.0.to_string(),
                            r.unit_name.clone(),
                            side(r.side),
                            r.base.clone(),
                            r.mission.clone(),
                            r.loadout.clone().unwrap_or_default(),
                            r.ended.map(clock).unwrap_or_default(),
                            format!("{:.0}", (until.seconds() - r.launched.seconds()) / 60.0),
                            r.outcome.clone(),
                            format!("{:.1}", r.tanker_fuel_t),
                        ],
                    );
                }
            }
            AarTable::Transits => {
                line(
                    &mut out,
                    &h(&[
                        "time",
                        "choke_point",
                        "unit",
                        "unit_name",
                        "side",
                        "lane",
                        "traffic_per_day",
                        "depth_m",
                        "draft_m",
                        "risk",
                    ]),
                );
                for r in &self.transits {
                    let num = |v: Option<f64>, p: usize| {
                        v.map_or_else(String::new, |v| format!("{v:.p$}"))
                    };
                    line(
                        &mut out,
                        &[
                            clock(r.tick),
                            r.choke_point.clone(),
                            r.unit.0.to_string(),
                            r.unit_name.clone(),
                            side(r.side),
                            r.lane.clone().unwrap_or_default(),
                            format!("{:.1}", r.traffic_per_day),
                            num(r.depth_m, 1),
                            num(r.draft_m, 1),
                            match r.risk {
                                sim_core::TransitRisk::Low => "low",
                                sim_core::TransitRisk::Moderate => "moderate",
                                sim_core::TransitRisk::High => "high",
                            }
                            .to_owned(),
                        ],
                    );
                }
            }
            AarTable::Timeline => {
                line(&mut out, &h(&["time", "tick", "kind", "side", "text"]));
                for e in &self.timeline {
                    line(
                        &mut out,
                        &[
                            clock(e.tick),
                            e.tick.0.to_string(),
                            kind(e.kind).to_owned(),
                            side(e.side),
                            e.text.clone(),
                        ],
                    );
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_are_quoted_and_formulas_defused() {
        assert_eq!(field("plain"), "plain");
        assert_eq!(field("a,b"), "\"a,b\"");
        assert_eq!(field("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(field("=CMD()"), "'=CMD()");
        assert_eq!(field("-3.5"), "-3.5");
        assert_eq!(clock(Tick(3_725)), "01:02:05");
    }
}
