//! Single-observer target motion analysis (`docs/models/tma.md`): each side's history of passive
//! acoustic bearings on each target from each of its observers, sampled every
//! [`TMA_SAMPLE_TICKS`] over the last [`TMA_WINDOW_TICKS`]. When no cross-fix from two observers
//! exists, the detection pass solves an observer's history with
//! [`sim_sensors::tma_solve`] and records the solution as a position measurement.
//!
//! Saved (format 27, default empty) and hashed only when non-empty, so runs with no passive
//! acoustic bearings keep their hashes.

use crate::ids::{SideId, UnitId};
use crate::time::Tick;
use bevy_ecs::prelude::Resource;
use serde::{Deserialize, Serialize};
use sim_sensors::{BearingLine, TmaBearing};
use std::collections::BTreeMap;

/// One bearing kept every this many ticks (s) per observer and target.
pub const TMA_SAMPLE_TICKS: u64 = 15;
/// Bearings older than this are dropped, ticks (s).
pub const TMA_WINDOW_TICKS: u64 = 900;
/// A history with no new bearing for this long is dropped (the contact was lost), ticks (s).
pub const TMA_GAP_TICKS: u64 = 120;

/// One observer's bearings on one target.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TmaTrack {
    pub samples: Vec<(Tick, BearingLine)>,
    /// A solution has been reported (the `TmaSolution` event is sent once per history).
    pub solved: bool,
}

/// Side → target → observer → bearing history.
#[derive(Debug, Clone, Default, PartialEq, Resource, Serialize, Deserialize)]
pub struct Tma {
    pub by_side: BTreeMap<SideId, BTreeMap<UnitId, BTreeMap<UnitId, TmaTrack>>>,
}

impl Tma {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_side.is_empty()
    }

    /// Records a bearing, at most one per [`TMA_SAMPLE_TICKS`]. A history broken by a gap
    /// longer than [`TMA_GAP_TICKS`] starts again.
    pub fn note(
        &mut self,
        side: SideId,
        target: UnitId,
        observer: UnitId,
        line: BearingLine,
        tick: Tick,
    ) {
        let track = self
            .by_side
            .entry(side)
            .or_default()
            .entry(target)
            .or_default()
            .entry(observer)
            .or_default();
        if let Some((last, _)) = track.samples.last() {
            let since = tick.0.saturating_sub(last.0);
            if since > TMA_GAP_TICKS {
                *track = TmaTrack::default();
            } else if since < TMA_SAMPLE_TICKS {
                return;
            }
        }
        track.samples.push((tick, line));
        track
            .samples
            .retain(|(t, _)| tick.0.saturating_sub(t.0) <= TMA_WINDOW_TICKS);
    }

    /// The history as TMA input, ages measured from `now`.
    #[must_use]
    pub fn bearings(
        &self,
        side: SideId,
        target: UnitId,
        observer: UnitId,
        now: Tick,
    ) -> Vec<TmaBearing> {
        self.track(side, target, observer)
            .map(|t| {
                t.samples
                    .iter()
                    .map(|(tick, line)| TmaBearing {
                        #[allow(clippy::cast_precision_loss)]
                        age_s: now.0.saturating_sub(tick.0) as f64,
                        line: *line,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn track(&self, side: SideId, target: UnitId, observer: UnitId) -> Option<&TmaTrack> {
        self.by_side.get(&side)?.get(&target)?.get(&observer)
    }

    /// Marks a history solved; true the first time.
    pub fn mark_solved(&mut self, side: SideId, target: UnitId, observer: UnitId) -> bool {
        let Some(t) = self
            .by_side
            .get_mut(&side)
            .and_then(|m| m.get_mut(&target))
            .and_then(|m| m.get_mut(&observer))
        else {
            return false;
        };
        !std::mem::replace(&mut t.solved, true)
    }

    /// Drops histories with no bearing for longer than [`TMA_GAP_TICKS`].
    pub fn expire(&mut self, now: Tick) {
        for targets in self.by_side.values_mut() {
            for observers in targets.values_mut() {
                observers.retain(|_, t| {
                    t.samples
                        .last()
                        .is_some_and(|(last, _)| now.0.saturating_sub(last.0) <= TMA_GAP_TICKS)
                });
            }
            targets.retain(|_, o| !o.is_empty());
        }
        self.by_side.retain(|_, t| !t.is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim_geo::GeoPoint;

    fn line(b: f64) -> BearingLine {
        BearingLine {
            from: GeoPoint::new(0.0, 0.0),
            bearing_deg: b,
            sigma_deg: 2.0,
        }
    }

    #[test]
    fn samples_every_fifteen_seconds_and_restarts_after_a_gap() {
        let mut t = Tma::default();
        let (s, tgt, obs) = (SideId(1), UnitId(2), UnitId(3));
        for tick in (0..=60).step_by(5) {
            t.note(s, tgt, obs, line(10.0), Tick(tick));
        }
        // 0, 15, 30, 45, 60.
        assert_eq!(t.bearings(s, tgt, obs, Tick(60)).len(), 5);
        assert!(t.mark_solved(s, tgt, obs));
        assert!(!t.mark_solved(s, tgt, obs));
        // A gap longer than two minutes starts the history again.
        t.note(s, tgt, obs, line(20.0), Tick(60 + TMA_GAP_TICKS + 1));
        assert_eq!(t.bearings(s, tgt, obs, Tick(200)).len(), 1);
        assert!(t.mark_solved(s, tgt, obs));
        t.expire(Tick(60 + 2 * TMA_GAP_TICKS + 2));
        assert!(t.is_empty());
    }

    #[test]
    fn keeps_only_the_window() {
        let mut t = Tma::default();
        let (s, tgt, obs) = (SideId(1), UnitId(2), UnitId(3));
        for tick in (0..=2000).step_by(15) {
            t.note(s, tgt, obs, line(10.0), Tick(tick));
        }
        let b = t.bearings(s, tgt, obs, Tick(1995));
        assert!(b.iter().all(|x| x.age_s <= TMA_WINDOW_TICKS as f64));
        assert_eq!(b.len(), (TMA_WINDOW_TICKS / TMA_SAMPLE_TICKS) as usize + 1);
    }
}
