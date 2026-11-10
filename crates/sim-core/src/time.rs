use bevy_ecs::prelude::Resource;
use serde::{Deserialize, Serialize};

/// Game seconds per base tick (ADR-0003). Time compression runs more ticks; it never lengthens one.
pub const TICK_SECONDS: f64 = 1.0;

/// A simulation tick number. Tick `n` covers game time `[n, n+1)` × [`TICK_SECONDS`].
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(transparent)]
// Exported to TypeScript as `number`: exact below 2^53 ticks (≈ 285 million years).
pub struct Tick(#[cfg_attr(feature = "specta", specta(type = u32))] pub u64);

impl Tick {
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0 + 1)
    }

    /// Game time at the start of this tick, seconds.
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // exact below 2^53 ticks (≈ 285 million years)
    pub fn seconds(self) -> f64 {
        self.0 as f64 * TICK_SECONDS
    }

    /// True on ticks where a subsystem with the given period (in ticks) should run.
    #[must_use]
    pub const fn is_multiple_of(self, period: u64) -> bool {
        period != 0 && self.0.is_multiple_of(period)
    }
}

impl std::fmt::Display for Tick {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "T{}", self.0)
    }
}

/// The world's clock: the tick that will run on the next `step`.
#[derive(Debug, Clone, Copy, Default, Resource, Serialize, Deserialize)]
pub struct SimClock {
    pub tick: Tick,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn periods() {
        assert!(Tick(0).is_multiple_of(5));
        assert!(Tick(10).is_multiple_of(5));
        assert!(!Tick(11).is_multiple_of(5));
        assert!(!Tick(10).is_multiple_of(0));
    }

    #[test]
    fn seconds() {
        assert_eq!(Tick(3600).seconds(), 3600.0);
    }
}
