//! Replay with a scrubber: rebuild once, verify, keep keyframes, seek anywhere.

use crate::record::ExerciseRecord;
use crate::report::AarReport;
use crate::AarError;
use sim_core::{SaveGame, SideId, Simulation, Snapshot, Tick, Viewer};
use sim_geo::land::LandMask;
use std::sync::Arc;

/// Keyframes are at least this many ticks apart.
pub const MIN_KEYFRAME_INTERVAL: u64 = 60;
/// At most this many keyframes are kept (the interval grows with the run length).
pub const MAX_KEYFRAMES: u64 = 240;

/// A recorded exercise opened for review.
pub struct Replay {
    record: ExerciseRecord,
    mask: Option<Arc<LandMask>>,
    interval: u64,
    /// `keyframes[i]` is the state at tick `i * interval`, without the event log.
    keyframes: Vec<SaveGame>,
    sim: Simulation,
    verified: bool,
    report: AarReport,
}

impl std::fmt::Debug for Replay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Replay")
            .field("end_tick", &self.record.end_tick)
            .field("tick", &self.sim.tick())
            .field("verified", &self.verified)
            .finish_non_exhaustive()
    }
}

fn fresh(record: &ExerciseRecord, mask: Option<&Arc<LandMask>>) -> Result<Simulation, AarError> {
    let mut sim = Simulation::new(record.seed);
    if let Some(m) = mask {
        sim.set_land_mask(m.clone());
    }
    let mut ordered: Vec<_> = record.commands.iter().collect();
    ordered.sort_by_key(|c| c.seq);
    for c in ordered {
        sim.submit(c.command.clone())?;
    }
    Ok(sim)
}

impl Replay {
    /// Rebuilds the run to its end, checks the final hash, builds the AAR report from the full
    /// logs, and leaves the cursor at the end. `sides` names the sides for the report.
    pub fn open(record: ExerciseRecord, sides: &[(SideId, String)]) -> Result<Self, AarError> {
        let mask = match record.land_mask.as_deref() {
            None => None,
            Some(id) => {
                let m = sim_geo::land::embedded();
                if m.id() != id {
                    return Err(AarError::UnknownLandMask(id.to_owned()));
                }
                Some(m)
            }
        };
        let end = record.end_tick.0;
        let interval = MIN_KEYFRAME_INTERVAL.max(end.div_ceil(MAX_KEYFRAMES));
        let mut sim = fresh(&record, mask.as_ref())?;
        let mut keyframes = Vec::new();
        loop {
            let t = sim.tick().0;
            if t % interval == 0 {
                keyframes.push(sim.keyframe());
            }
            if t >= end {
                break;
            }
            sim.step();
        }
        let verified = sim.state_hash() == record.final_hash;
        let report = AarReport::build(&sim.command_log(), sim.events(), sides, sim.tick());
        Ok(Self {
            record,
            mask,
            interval,
            keyframes,
            sim,
            verified,
            report,
        })
    }

    /// Did the rebuilt run reach the recorded final state exactly?
    #[must_use]
    pub fn verified(&self) -> bool {
        self.verified
    }

    #[must_use]
    pub fn record(&self) -> &ExerciseRecord {
        &self.record
    }

    #[must_use]
    pub fn report(&self) -> &AarReport {
        &self.report
    }

    #[must_use]
    pub fn end_tick(&self) -> Tick {
        self.record.end_tick
    }

    /// The scrubber position.
    #[must_use]
    pub fn tick(&self) -> Tick {
        self.sim.tick()
    }

    /// Moves the scrubber to `tick` (clamped to the end): forward by running, otherwise from the
    /// nearest keyframe at or before it.
    pub fn seek(&mut self, tick: Tick) -> Result<(), AarError> {
        let target = tick.0.min(self.record.end_tick.0);
        let now = self.sim.tick().0;
        let k = usize::try_from(target / self.interval)
            .unwrap_or(usize::MAX)
            .min(self.keyframes.len().saturating_sub(1));
        let from_keyframe = self.keyframes.get(k);
        let resume_at = from_keyframe.map_or(0, |s| s.tick.0);
        // Restart when going backwards, or when a keyframe is closer than running on.
        if target < now || resume_at > now {
            self.sim = match from_keyframe {
                Some(save) => {
                    let mut sim = Simulation::from_save(save.clone())?;
                    if let Some(m) = &self.mask {
                        sim.set_land_mask(m.clone());
                    }
                    sim
                }
                None => fresh(&self.record, self.mask.as_ref())?,
            };
        }
        self.sim.run_until(Tick(target));
        Ok(())
    }

    /// Advances the scrubber by up to `n` ticks, stopping at the end. Returns the ticks run.
    pub fn advance(&mut self, n: u64) -> u64 {
        let end = self.record.end_tick.0;
        let now = self.sim.tick().0;
        let run = n.min(end.saturating_sub(now));
        self.sim.run_ticks(run);
        run
    }

    #[must_use]
    pub fn at_end(&self) -> bool {
        self.sim.tick() >= self.record.end_tick
    }

    /// The picture at the scrubber position.
    pub fn snapshot(&mut self, viewer: Viewer) -> Snapshot {
        self.sim.snapshot(viewer)
    }

    /// Events logged since the scrubber last restarted from a keyframe.
    #[must_use]
    pub fn events_len(&self) -> usize {
        self.sim.events().len()
    }

    /// The simulation at the scrubber position (for frame building).
    pub fn sim_mut(&mut self) -> &mut Simulation {
        &mut self.sim
    }
}
