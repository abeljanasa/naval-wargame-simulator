//! Owns the running simulation and paces it against wall-clock time.
//!
//! Real-time pacing is the *only* place wall-clock time is used, and it decides only how many
//! fixed ticks to run. It never changes what a tick does (ADR-0003 §1).

use serde::{Deserialize, Serialize};
use sim_core::{
    Command, CommandPayload, GeoPoint, Issuer, SimError, Simulation, Snapshot, Tick, UnitId, Viewer,
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
    sim: Simulation,
    paused: bool,
    compression: TimeCompression,
    viewer: Viewer,
    /// Fractional ticks owed from previous frames.
    carry: f64,
    achieved_ratio: f64,
}

impl Host {
    pub fn new(scenario: Scenario) -> Result<Self, sim_scenario::ScenarioError> {
        let sim = scenario.build(None)?;
        let first_side = scenario.sides.first().map(|s| s.id);
        Ok(Self {
            scenario,
            sim,
            paused: true,
            compression: TimeCompression::X1,
            viewer: first_side.map_or(Viewer::WhiteCell, |side| Viewer::Side { side }),
            carry: 0.0,
            achieved_ratio: 0.0,
        })
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

    pub fn snapshot(&mut self) -> Snapshot {
        self.sim.snapshot(self.viewer)
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host() -> Host {
        let s = Scenario::from_json(include_str!(
            "../../../../data/scenarios/phase1-strait-transit.json"
        ))
        .unwrap();
        Host::new(s).unwrap()
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
        let snap = h.snapshot();
        let merchant = snap.units.iter().find(|u| u.id == UnitId(2)).unwrap();
        assert!(
            (merchant.ordered_speed_mps - 6.5).abs() < 1e-9,
            "order from wrong side must be rejected (seq {seq})"
        );
        h.order_speed(UnitId(2), 1.0).unwrap();
        h.step(1);
        let snap = h.snapshot();
        let merchant = snap.units.iter().find(|u| u.id == UnitId(2)).unwrap();
        assert!((merchant.ordered_speed_mps - 1.0).abs() < 1e-9);
    }
}
