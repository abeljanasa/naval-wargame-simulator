//! One seeded random stream per subsystem (ADR-0003 §2).

use bevy_ecs::prelude::Resource;
use rand_chacha::rand_core::{RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Fixed stream identifiers. **Never renumber**: changing a value changes every result for that subsystem.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(u64)]
pub enum StreamId {
    Movement = 1,
    Sensors = 2,
    Weapons = 3,
    Damage = 4,
    Ai = 5,
    Events = 6,
}

impl StreamId {
    pub const ALL: [StreamId; 6] = [
        StreamId::Movement,
        StreamId::Sensors,
        StreamId::Weapons,
        StreamId::Damage,
        StreamId::Ai,
        StreamId::Events,
    ];
}

/// The world's random streams. Each is ChaCha8 keyed by the scenario seed, on its own ChaCha
/// stream number, so the subsystems' sequences are independent.
#[derive(Debug, Clone, Resource, Serialize, Deserialize)]
pub struct RngStreams {
    seed: u64,
    streams: BTreeMap<StreamId, ChaCha8Rng>,
}

impl RngStreams {
    #[must_use]
    pub fn new(seed: u64) -> Self {
        let streams = StreamId::ALL
            .iter()
            .map(|&id| {
                let mut rng = ChaCha8Rng::seed_from_u64(seed);
                rng.set_stream(id as u64);
                (id, rng)
            })
            .collect();
        Self { seed, streams }
    }

    #[must_use]
    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// Mutable access to one subsystem's stream.
    pub fn stream(&mut self, id: StreamId) -> &mut ChaCha8Rng {
        // Every StreamId is inserted in `new`, and deserialised saves are checked in
        // `is_complete`, so the entry always exists. `or_insert_with` keeps this panic-free anyway.
        let seed = self.seed;
        self.streams.entry(id).or_insert_with(|| {
            let mut rng = ChaCha8Rng::seed_from_u64(seed);
            rng.set_stream(id as u64);
            rng
        })
    }

    /// Uniform draw in [0, 1) with 53 bits of precision.
    #[allow(clippy::cast_precision_loss)] // exact: value < 2^53
    pub fn uniform(&mut self, id: StreamId) -> f64 {
        (self.stream(id).next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    /// Position of every stream (in 32-bit words), for the state hash.
    #[must_use]
    pub fn positions(&self) -> Vec<(StreamId, u128)> {
        self.streams
            .iter()
            .map(|(&id, rng)| (id, rng.get_word_pos()))
            .collect()
    }

    /// True when every known stream is present (used to validate loaded saves).
    #[must_use]
    pub fn is_complete(&self) -> bool {
        StreamId::ALL.iter().all(|id| self.streams.contains_key(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_sequence() {
        let mut a = RngStreams::new(42);
        let mut b = RngStreams::new(42);
        for _ in 0..100 {
            assert_eq!(
                a.stream(StreamId::Sensors).next_u64(),
                b.stream(StreamId::Sensors).next_u64()
            );
        }
    }

    #[test]
    fn streams_are_independent() {
        let mut a = RngStreams::new(7);
        let mut b = RngStreams::new(7);
        // Draw heavily from one stream in `a` only.
        for _ in 0..1000 {
            a.stream(StreamId::Weapons).next_u64();
        }
        assert_eq!(
            a.stream(StreamId::Sensors).next_u64(),
            b.stream(StreamId::Sensors).next_u64()
        );
    }

    #[test]
    fn different_streams_differ() {
        let mut r = RngStreams::new(7);
        let x = r.clone().stream(StreamId::Sensors).next_u64();
        let y = r.stream(StreamId::Weapons).next_u64();
        assert_ne!(x, y);
    }

    #[test]
    fn uniform_in_unit_interval() {
        let mut r = RngStreams::new(1);
        for _ in 0..10_000 {
            let u = r.uniform(StreamId::Ai);
            assert!((0.0..1.0).contains(&u));
        }
    }

    #[test]
    fn serde_round_trip_preserves_position() {
        let mut r = RngStreams::new(99);
        for _ in 0..37 {
            r.stream(StreamId::Damage).next_u64();
        }
        let bytes = rmp_serde::to_vec_named(&r).unwrap();
        let mut back: RngStreams = rmp_serde::from_slice(&bytes).unwrap();
        assert!(back.is_complete());
        assert_eq!(
            r.stream(StreamId::Damage).next_u64(),
            back.stream(StreamId::Damage).next_u64()
        );
    }
}
