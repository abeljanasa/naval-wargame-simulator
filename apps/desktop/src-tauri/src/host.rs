//! Owns the running simulation and paces it against wall-clock time.
//!
//! Real-time pacing is the *only* place wall-clock time is used, and it decides only how many
//! fixed ticks to run. It never changes what a tick does (ADR-0003 §1).

use serde::{Deserialize, Serialize};
use sim_core::{
    Command, CommandPayload, ContactView, Doctrine, DoctrineScope, GeoPoint, Issuer, SimError,
    Simulation, Snapshot, Tick, UnitId, UnitView, Viewer,
};
use sim_scenario::Scenario;
use std::time::{Duration, Instant};

/// Time compression settings offered in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum TimeCompression {
    X1,
    X2,
    X5,
    X15,
    X30,
    X60,
    /// As fast as the machine allows.
    Max,
}

impl TimeCompression {
    /// Game seconds per wall second, or `None` for unbounded.
    #[must_use]
    pub fn factor(self) -> Option<f64> {
        match self {
            Self::X1 => Some(1.0),
            Self::X2 => Some(2.0),
            Self::X5 => Some(5.0),
            Self::X15 => Some(15.0),
            Self::X30 => Some(30.0),
            Self::X60 => Some(60.0),
            Self::Max => None,
        }
    }
}

/// One frame's change to the viewer's picture. `full` = replace everything; otherwise `units`
/// are added or replaced by id and `removed` are dropped. Contacts are always sent whole.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct SnapshotUpdate {
    pub full: bool,
    pub tick: Tick,
    pub game_time_s: f64,
    pub viewer: Viewer,
    pub units: Vec<UnitView>,
    pub removed: Vec<UnitId>,
    pub contacts: Vec<ContactView>,
}

/// Clock status sent with every frame.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct ClockStatus {
    pub tick: Tick,
    pub game_time_s: f64,
    pub paused: bool,
    pub compression: TimeCompression,
    /// Game seconds per wall second actually achieved over the last frame.
    pub achieved_ratio: f64,
    pub scenario_duration_s: f64,
}

/// Longest wall-clock gap credited in one frame. This stops a stall (debugger, sleep) from
/// turning into a burst of thousands of ticks.
const MAX_FRAME_CREDIT: Duration = Duration::from_millis(250);

pub struct Host {
    pub scenario: Scenario,
    platforms: sim_db::PlatformDb,
    sim: Simulation,
    /// The picture last streamed to the UI, for diffing; `None` forces a full update.
    last_sent: Option<Snapshot>,
    paused: bool,
    compression: TimeCompression,
    viewer: Viewer,
    /// Fractional ticks owed from previous frames.
    carry: f64,
    achieved_ratio: f64,
}

impl Host {
    pub fn new(
        scenario: Scenario,
        platforms: sim_db::PlatformDb,
    ) -> Result<Self, sim_scenario::ScenarioError> {
        let sim = scenario.build(None, &platforms)?;
        let first_side = scenario.sides.first().map(|s| s.id);
        Ok(Self {
            scenario,
            platforms,
            sim,
            last_sent: None,
            paused: true,
            compression: TimeCompression::X1,
            viewer: first_side.map_or(Viewer::WhiteCell, |side| Viewer::Side { side }),
            carry: 0.0,
            achieved_ratio: 0.0,
        })
    }

    /// Replaces the running scenario. The new one is fully built first, so a scenario that fails
    /// to load leaves the current session untouched. The new session starts paused, at 1×,
    /// in the first side's seat.
    pub fn load(&mut self, scenario: Scenario) -> Result<(), sim_scenario::ScenarioError> {
        let sim = scenario.build(None, &self.platforms)?;
        self.viewer = scenario
            .sides
            .first()
            .map_or(Viewer::WhiteCell, |s| Viewer::Side { side: s.id });
        self.scenario = scenario;
        self.sim = sim;
        self.last_sent = None;
        self.paused = true;
        self.compression = TimeCompression::X1;
        self.carry = 0.0;
        self.achieved_ratio = 0.0;
        Ok(())
    }

    /// The platform database (validated when it was opened).
    #[must_use]
    pub fn platforms(&self) -> &sim_db::PlatformDb {
        &self.platforms
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
        self.carry = 0.0;
    }

    pub fn set_compression(&mut self, c: TimeCompression) {
        self.compression = c;
        self.carry = 0.0;
    }

    pub fn set_viewer(&mut self, viewer: Viewer) {
        self.viewer = viewer;
        self.last_sent = None;
    }

    #[must_use]
    pub fn viewer(&self) -> Viewer {
        self.viewer
    }

    /// Runs the ticks owed for `wall_elapsed` of real time. In `Max` mode it runs until
    /// `max_budget` of wall time has been spent. Returns the number of ticks run.
    pub fn advance(&mut self, wall_elapsed: Duration, max_budget: Duration) -> u64 {
        if self.paused {
            self.achieved_ratio = 0.0;
            return 0;
        }
        let credited = wall_elapsed.min(MAX_FRAME_CREDIT);
        let ran = if let Some(factor) = self.compression.factor() {
            let owed = credited.as_secs_f64() * factor + self.carry;
            let whole = owed.floor();
            self.carry = owed - whole;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            // `whole` is non-negative and bounded by 60 × 0.25 s
            let n = whole as u64;
            self.sim.run_ticks(n);
            n
        } else {
            // Wall-clock pacing lives outside the deterministic core (ADR-0003 §1).
            #[allow(clippy::disallowed_methods)]
            let start = Instant::now();
            let mut n = 0;
            while start.elapsed() < max_budget {
                self.sim.run_ticks(32);
                n += 32;
            }
            n
        };
        let secs = wall_elapsed.as_secs_f64();
        #[allow(clippy::cast_precision_loss)]
        if secs > 0.0 {
            self.achieved_ratio = ran as f64 / secs;
        }
        ran
    }

    pub fn step(&mut self, ticks: u64) {
        self.sim.run_ticks(ticks);
    }

    pub fn clock(&self) -> ClockStatus {
        let tick = self.sim.tick();
        #[allow(clippy::cast_precision_loss)]
        let duration = self.scenario.duration_s as f64;
        ClockStatus {
            tick,
            game_time_s: tick.seconds(),
            paused: self.paused,
            compression: self.compression,
            achieved_ratio: self.achieved_ratio,
            scenario_duration_s: duration,
        }
    }

    /// The next UI update: a full picture the first time (and after any seat or scenario change),
    /// then only the units that changed and the ids of those that left the picture (§6.2).
    pub fn update(&mut self) -> SnapshotUpdate {
        let snap = self.sim.snapshot(self.viewer);
        let update = match &self.last_sent {
            Some(prev) if prev.viewer == snap.viewer => {
                let old: std::collections::BTreeMap<UnitId, &UnitView> =
                    prev.units.iter().map(|u| (u.id, u)).collect();
                let new_ids: std::collections::BTreeSet<UnitId> =
                    snap.units.iter().map(|u| u.id).collect();
                SnapshotUpdate {
                    full: false,
                    tick: snap.tick,
                    game_time_s: snap.game_time_s,
                    viewer: snap.viewer,
                    units: snap
                        .units
                        .iter()
                        .filter(|u| old.get(&u.id).is_none_or(|o| *o != *u))
                        .cloned()
                        .collect(),
                    removed: old
                        .keys()
                        .filter(|id| !new_ids.contains(id))
                        .copied()
                        .collect(),
                    contacts: snap.contacts.clone(),
                }
            }
            _ => SnapshotUpdate {
                full: true,
                tick: snap.tick,
                game_time_s: snap.game_time_s,
                viewer: snap.viewer,
                units: snap.units.clone(),
                removed: vec![],
                contacts: snap.contacts.clone(),
            },
        };
        self.last_sent = Some(snap);
        update
    }

    /// Issues an order from the current seat. The white-cell seat may order any unit; a side
    /// seat only its own (the core enforces this).
    pub fn order(&mut self, payload: CommandPayload) -> Result<u64, SimError> {
        let (issuer, side) = match self.viewer {
            Viewer::WhiteCell => (
                Issuer::WhiteCell {
                    umpire: "local".into(),
                },
                None,
            ),
            Viewer::Side { side } => (Issuer::Player { seat: 1 }, Some(side)),
        };
        self.sim.submit(Command {
            tick: self.sim.tick(),
            issuer,
            side,
            payload,
        })
    }

    pub fn order_route(&mut self, unit: UnitId, waypoints: Vec<GeoPoint>) -> Result<u64, SimError> {
        self.order(CommandPayload::SetRoute { unit, waypoints })
    }

    pub fn order_speed(&mut self, unit: UnitId, speed_mps: f64) -> Result<u64, SimError> {
        self.order(CommandPayload::SetSpeed { unit, speed_mps })
    }

    /// Replaces the doctrine set on `unit` (unset fields inherit from its group and side).
    pub fn order_doctrine(&mut self, unit: UnitId, doctrine: Doctrine) -> Result<u64, SimError> {
        self.order(CommandPayload::SetDoctrine {
            scope: DoctrineScope::Unit { unit },
            doctrine,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host() -> Host {
        let s = Scenario::from_json(include_str!(
            "../../../../data/scenarios/phase1-strait-transit.json"
        ))
        .unwrap();
        Host::new(s, sim_db::embedded::open_platform_db().unwrap()).unwrap()
    }

    #[test]
    fn a_player_sets_doctrine_on_its_own_unit() {
        let mut h = host();
        h.set_viewer(Viewer::Side {
            side: sim_core::SideId(1),
        });
        h.step(1);
        let silent = Doctrine {
            emcon: Some(sim_core::Emcon::Silent),
            ..Doctrine::default()
        };
        h.order_doctrine(UnitId(1), silent).unwrap();
        h.step(1);
        let snap = h.sim.snapshot(Viewer::Side {
            side: sim_core::SideId(1),
        });
        assert_eq!(snap.units[0].doctrine.emcon, sim_core::Emcon::Silent);
        // The demo's Blue side doctrine (WCS hold) is inherited.
        assert_eq!(snap.units[0].doctrine.wcs, sim_core::Wcs::Hold);
    }

    #[test]
    fn platform_database_is_browsable_with_provenance() {
        let h = host();
        let db = h.platforms();
        let all = db.platforms().unwrap();
        assert!(all.len() >= 50);
        let (citation, _confidence, classification) =
            db.platform_provenance("GEN-FFG").unwrap().unwrap();
        assert!(!citation.is_empty());
        assert_eq!(classification, "UNCLASSIFIED");
        assert!(db.platform_provenance("NO-SUCH").unwrap().is_none());
        assert!(db
            .issues()
            .unwrap()
            .iter()
            .all(|i| i.severity == sim_db::Severity::Warning));
    }

    #[test]
    fn starts_paused_and_does_not_advance() {
        let mut h = host();
        assert_eq!(h.advance(Duration::from_secs(1), Duration::ZERO), 0);
        assert_eq!(h.clock().tick, Tick(0));
    }

    #[test]
    fn compression_runs_whole_ticks_and_carries_fractions() {
        let mut h = host();
        h.set_paused(false);
        h.set_compression(TimeCompression::X15);
        // 33 ms at 15× = 0.495 ticks → 0, carry 0.495; second frame → 0.99 → 0; third → 1.485 → 1.
        let budget = Duration::ZERO;
        let f = Duration::from_millis(33);
        assert_eq!(h.advance(f, budget), 0);
        assert_eq!(h.advance(f, budget), 0);
        assert_eq!(h.advance(f, budget), 1);
    }

    #[test]
    fn long_stall_is_capped() {
        let mut h = host();
        h.set_paused(false);
        h.set_compression(TimeCompression::X60);
        // A 10 s stall is credited as 0.25 s → 15 ticks.
        assert_eq!(h.advance(Duration::from_secs(10), Duration::ZERO), 15);
    }

    #[test]
    fn side_seat_cannot_order_other_side_but_white_cell_can() {
        let mut h = host();
        h.set_viewer(Viewer::Side {
            side: sim_core::SideId(1),
        });
        let seq = h.order_speed(UnitId(2), 1.0).unwrap();
        h.step(1);
        h.set_viewer(Viewer::WhiteCell);
        let snap = h.sim.snapshot(Viewer::WhiteCell);
        let merchant = snap.units.iter().find(|u| u.id == UnitId(2)).unwrap();
        assert!(
            (merchant.ordered_speed_mps - 6.5).abs() < 1e-9,
            "order from wrong side must be rejected (seq {seq})"
        );
        h.order_speed(UnitId(2), 1.0).unwrap();
        h.step(1);
        let snap = h.sim.snapshot(Viewer::WhiteCell);
        let merchant = snap.units.iter().find(|u| u.id == UnitId(2)).unwrap();
        assert!((merchant.ordered_speed_mps - 1.0).abs() < 1e-9);
    }

    #[test]
    fn loading_a_scenario_replaces_the_session_and_a_bad_one_changes_nothing() {
        let mut h = host();
        h.set_paused(false);
        h.step(100);
        let inline = r#"{"format":"nws-scenario","format_version":2,"name":"Loaded","seed":3,"duration_s":60,
            "sides":[{"id":7,"name":"Red"}],
            "units":[{"id":1,"name":"A","side":7,"platform":"GEN-OPV","position":{"lat_deg":-39.3,"lon_deg":146.0}}]}"#;
        h.load(Scenario::from_json(inline).unwrap()).unwrap();
        assert_eq!(h.scenario.name, "Loaded");
        assert_eq!(h.clock().tick, Tick(0));
        assert!(h.clock().paused);
        assert_eq!(
            h.viewer(),
            Viewer::Side {
                side: sim_core::SideId(7)
            }
        );

        let bad = inline.replace("GEN-OPV", "GEN-NOPE");
        assert!(h.load(Scenario::from_json(&bad).unwrap()).is_err());
        assert_eq!(
            h.scenario.name, "Loaded",
            "failed load must not change the session"
        );
    }

    #[test]
    fn updates_are_full_first_then_only_changes() {
        let mut h = host();
        h.set_viewer(Viewer::WhiteCell);
        h.step(1); // units spawn on the first tick
        let first = h.update();
        assert!(first.full);
        assert_eq!(first.units.len(), 2);
        // Paused and nothing ordered: nothing changed, so nothing is resent.
        let idle = h.update();
        assert!(!idle.full);
        assert!(idle.units.is_empty() && idle.removed.is_empty());
        // One tick of movement: both ships moved, so both are resent (and only once).
        h.step(1);
        let moved = h.update();
        assert!(!moved.full);
        assert_eq!(moved.units.len(), 2);
        // A seat change forces a full picture for the new viewer.
        h.set_viewer(Viewer::Side {
            side: sim_core::SideId(1),
        });
        let seat = h.update();
        assert!(seat.full);
        assert_eq!(seat.units.len(), 1);
    }
}
