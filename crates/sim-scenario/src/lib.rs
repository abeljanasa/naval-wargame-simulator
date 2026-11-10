//! Scenario files: JSON, versioned, validated on load.
//!
//! Phase 1 holds the seed, sides and initial units. Missions, events (triggers → conditions →
//! actions), Lua scripts and planning objects arrive in Phase 3.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use sim_core::{Command, CommandPayload, Issuer, SideId, Simulation, Tick, UnitSpawn};
use std::collections::BTreeSet;
use std::path::Path;

pub const SCENARIO_FORMAT: &str = "nws-scenario";
pub const SCENARIO_FORMAT_VERSION: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum ScenarioError {
    #[error("cannot read scenario file: {0}")]
    Io(#[from] std::io::Error),
    #[error("scenario JSON is malformed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("not a scenario file (format = {0:?})")]
    WrongFormat(String),
    #[error("scenario format version {found} is newer than this build supports ({supported})")]
    FutureVersion { found: u32, supported: u32 },
    #[error("scenario is invalid: {0}")]
    Invalid(String),
    #[error(transparent)]
    Sim(#[from] sim_core::SimError),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SideDef {
    pub id: SideId,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scenario {
    pub format: String,
    pub format_version: u32,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Default RNG seed. Batch runs override it per replication.
    pub seed: u64,
    /// Default run length in game seconds.
    pub duration_s: u64,
    pub sides: Vec<SideDef>,
    pub units: Vec<UnitSpawn>,
}

impl Scenario {
    pub fn from_json(text: &str) -> Result<Self, ScenarioError> {
        #[derive(Deserialize)]
        struct Header {
            format: String,
            format_version: u32,
        }
        let header: Header = serde_json::from_str(text)?;
        if header.format != SCENARIO_FORMAT {
            return Err(ScenarioError::WrongFormat(header.format));
        }
        if header.format_version > SCENARIO_FORMAT_VERSION {
            return Err(ScenarioError::FutureVersion {
                found: header.format_version,
                supported: SCENARIO_FORMAT_VERSION,
            });
        }
        let scenario: Scenario = serde_json::from_str(text)?;
        scenario.validate()?;
        Ok(scenario)
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, ScenarioError> {
        Self::from_json(&std::fs::read_to_string(path)?)
    }

    /// Structural checks: unique side and unit ids, and every unit on a declared side.
    /// Per-unit value checks (finite speeds, valid coordinates) run when the spawn command is submitted.
    pub fn validate(&self) -> Result<(), ScenarioError> {
        let mut sides = BTreeSet::new();
        for s in &self.sides {
            if !sides.insert(s.id) {
                return Err(ScenarioError::Invalid(format!(
                    "duplicate side id {}",
                    s.id
                )));
            }
        }
        let mut units = BTreeSet::new();
        for u in &self.units {
            if !units.insert(u.id) {
                return Err(ScenarioError::Invalid(format!(
                    "duplicate unit id {}",
                    u.id
                )));
            }
            if !sides.contains(&u.side) {
                return Err(ScenarioError::Invalid(format!(
                    "unit {} is on undeclared side {}",
                    u.id, u.side
                )));
            }
        }
        if self.duration_s == 0 {
            return Err(ScenarioError::Invalid("duration_s must be > 0".into()));
        }
        Ok(())
    }

    /// Run length in ticks. `TICK_SECONDS` is 1.0, so seconds and ticks are the same number (ADR-0003).
    #[must_use]
    pub fn duration_ticks(&self) -> Tick {
        Tick(self.duration_s)
    }

    /// Builds a simulation at tick 0, with the initial units queued as scenario commands.
    pub fn build(&self, seed: Option<u64>) -> Result<Simulation, ScenarioError> {
        let mut sim = Simulation::new(seed.unwrap_or(self.seed));
        for u in &self.units {
            sim.submit(Command {
                tick: Tick(0),
                issuer: Issuer::Scenario,
                side: None,
                payload: CommandPayload::SpawnUnit(u.clone()),
            })?;
        }
        Ok(sim)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = r#"{
        "format": "nws-scenario", "format_version": 1, "name": "t", "seed": 1, "duration_s": 10,
        "sides": [{"id": 1, "name": "Blue"}],
        "units": [{"id": 1, "name": "A", "side": 1, "domain": "surface",
                   "position": {"lat_deg": 0.0, "lon_deg": 0.0},
                   "max_speed_mps": 10.0, "accel_mps2": 0.1}]
    }"#;

    #[test]
    fn minimal_loads() {
        let s = Scenario::from_json(MINIMAL).unwrap();
        assert_eq!(s.units.len(), 1);
        let mut sim = s.build(None).unwrap();
        sim.step();
        assert_eq!(sim.snapshot(sim_core::Viewer::WhiteCell).units.len(), 1);
    }

    #[test]
    fn rejects_wrong_format_and_future_version() {
        let wrong = MINIMAL.replace("nws-scenario", "other");
        assert!(matches!(
            Scenario::from_json(&wrong),
            Err(ScenarioError::WrongFormat(_))
        ));
        let future = MINIMAL.replace("\"format_version\": 1", "\"format_version\": 99");
        assert!(matches!(
            Scenario::from_json(&future),
            Err(ScenarioError::FutureVersion { .. })
        ));
    }

    #[test]
    fn rejects_unit_on_undeclared_side() {
        let bad = MINIMAL.replace("\"side\": 1", "\"side\": 2");
        assert!(matches!(
            Scenario::from_json(&bad),
            Err(ScenarioError::Invalid(_))
        ));
    }
}
