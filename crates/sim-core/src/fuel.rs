//! Fuel for ships and submarines (Phase 3 slice 11; model doc `docs/models/fuel.md`).
//!
//! Burn rate at speed v, tonnes per second:
//! r(v) = C · (v_c / E) · [h + (1 − h)·(v / v_c)³],
//! where C is the capacity (t), E the endurance (m) at cruise speed v_c, and h the hotel load
//! (generators, auxiliaries) as a fraction of the cruise burn. At cruise r = C·v_c/E, so a full
//! load lasts E/v_c seconds and covers exactly E metres. The cube law is propulsion power
//! against speed for a displacement hull.
//!
//! A unit out of fuel has no propulsion: its ordered speed is treated as zero and it coasts to a
//! stop at its deceleration. Land units carry no fuel model.
//!
//! Aircraft (Phase 4 slice 11, `docs/models/air-operations.md`) burn **aviation** fuel by the same
//! law with an idle share of 0.6: an airframe's fuel flow falls far less with speed than a hull's
//! (it must keep flying), and rises steeply above cruise (dash, afterburner). Takeoff and climb
//! cost a fixed allowance at launch; an aircraft out of fuel is lost.

use bevy_ecs::prelude::Component;
use serde::{Deserialize, Serialize};

/// Hotel load as a fraction of the cruise burn rate.
pub const HOTEL_FRACTION: f64 = 0.1;
/// Default bingo: below this fraction of capacity, a unit on a mission goes to refuel.
pub const DEFAULT_BINGO: f64 = 0.3;
/// Fuel capacity as a fraction of full-load displacement, when the scenario gives none (generic).
pub const FUEL_FRACTION_OF_DISPLACEMENT: f64 = 0.1;

fn one() -> f64 {
    1.0
}

fn hotel() -> f64 {
    HOTEL_FRACTION
}

/// Idle share of the cruise burn for aircraft.
pub const AIRCRAFT_IDLE_FRACTION: f64 = 0.6;

/// What a unit burns, and what a facility holds (Phase 4 slice 11).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum FuelKind {
    /// Ship fuel (diesel, F-76).
    #[default]
    Marine,
    /// Aviation fuel (JP-5, F-44).
    Aviation,
}

/// A unit's fuel as given at spawn.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct FuelSpec {
    /// Tonnes when full.
    pub capacity_t: f64,
    /// Range on a full load at cruise speed, metres.
    pub endurance_m: f64,
    pub cruise_mps: f64,
    /// Fraction of capacity at the start (default full).
    #[serde(default = "one")]
    pub level: f64,
    /// Fraction below which a unit on a mission goes to refuel (default 0.3).
    #[serde(default)]
    pub bingo: Option<f64>,
    /// What it burns (default ship fuel).
    #[serde(default)]
    pub kind: FuelKind,
    /// Idle share of the cruise burn (default 0.1, a hull's hotel load; aircraft 0.6).
    #[serde(default)]
    pub idle_fraction: Option<f64>,
}

impl FuelSpec {
    pub(crate) fn validate(&self) -> Result<(), String> {
        let pos = |v: f64| v.is_finite() && v > 0.0;
        let frac = |v: f64| v.is_finite() && (0.0..=1.0).contains(&v);
        if !(pos(self.capacity_t) && pos(self.endurance_m) && pos(self.cruise_mps)) {
            return Err("fuel needs a capacity, an endurance and a cruise speed > 0".into());
        }
        if !frac(self.level)
            || self.bingo.is_some_and(|b| !frac(b))
            || self.idle_fraction.is_some_and(|h| !frac(h))
        {
            return Err("fuel level and bingo must be fractions from 0 to 1".into());
        }
        Ok(())
    }
}

/// A unit's fuel state.
#[derive(Debug, Clone, Copy, PartialEq, Component, Serialize, Deserialize)]
pub struct Fuel {
    pub capacity_t: f64,
    pub endurance_m: f64,
    pub cruise_mps: f64,
    /// Tonnes left.
    pub fuel_t: f64,
    pub bingo: f64,
    #[serde(default)]
    pub kind: FuelKind,
    #[serde(default = "hotel")]
    pub idle_fraction: f64,
}

impl Fuel {
    #[must_use]
    pub fn from_spec(s: &FuelSpec) -> Self {
        Self {
            capacity_t: s.capacity_t,
            endurance_m: s.endurance_m,
            cruise_mps: s.cruise_mps,
            fuel_t: s.capacity_t * s.level,
            bingo: s.bingo.unwrap_or(DEFAULT_BINGO),
            kind: s.kind,
            idle_fraction: s.idle_fraction.unwrap_or(HOTEL_FRACTION),
        }
    }

    /// Fraction of capacity left.
    #[must_use]
    pub fn fraction(&self) -> f64 {
        self.fuel_t / self.capacity_t
    }

    #[must_use]
    pub fn empty(&self) -> bool {
        self.fuel_t <= 0.0
    }

    /// Below bingo.
    #[must_use]
    pub fn low(&self) -> bool {
        self.fraction() < self.bingo
    }
}

/// Burn rate at `speed_mps`, tonnes per second (see the module docs).
#[must_use]
pub fn burn_t_per_s(f: &Fuel, speed_mps: f64) -> f64 {
    let x = speed_mps.max(0.0) / f.cruise_mps;
    f.capacity_t
        * (f.cruise_mps / f.endurance_m)
        * (f.idle_fraction + (1.0 - f.idle_fraction) * x * x * x)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frigate() -> Fuel {
        // 500 t, 4 500 nm at 16 kn.
        Fuel::from_spec(&FuelSpec {
            capacity_t: 500.0,
            endurance_m: 4_500.0 * 1_852.0,
            cruise_mps: 16.0 * 1_852.0 / 3_600.0,
            level: 1.0,
            bingo: None,
            kind: FuelKind::Marine,
            idle_fraction: None,
        })
    }

    #[test]
    fn a_full_load_covers_the_endurance_at_cruise() {
        let f = frigate();
        let t_full = f.capacity_t / burn_t_per_s(&f, f.cruise_mps);
        // 8 334 000 m at 8.2311 m/s = 1 012 500 s (11.7 days).
        assert!((t_full * f.cruise_mps - f.endurance_m).abs() < 1e-6);
        assert!((t_full - 1_012_500.0).abs() < 1e-6);
    }

    #[test]
    fn hand_values_at_rest_and_at_twice_cruise() {
        let f = frigate();
        let cruise = burn_t_per_s(&f, f.cruise_mps);
        // Stopped: the hotel load, 10 % of the cruise rate.
        assert!((burn_t_per_s(&f, 0.0) - 0.1 * cruise).abs() < 1e-15);
        // Twice cruise: 0.1 + 0.9 × 8 = 7.3 × the cruise rate, so the range on a full load is
        // E × 2 / 7.3 = 27.4 % of the endurance.
        let r2 = burn_t_per_s(&f, 2.0 * f.cruise_mps);
        assert!((r2 / cruise - 7.3).abs() < 1e-12);
        let range = f.capacity_t / r2 * 2.0 * f.cruise_mps;
        assert!((range / f.endurance_m - 2.0 / 7.3).abs() < 1e-12);
    }

    #[test]
    fn bingo_and_validation() {
        let mut f = frigate();
        assert!(!f.low());
        f.fuel_t = 140.0;
        assert!(f.low(), "28 % < 30 %");
        let bad = FuelSpec {
            capacity_t: 0.0,
            endurance_m: 1.0,
            cruise_mps: 1.0,
            level: 1.0,
            bingo: None,
            kind: FuelKind::Marine,
            idle_fraction: None,
        };
        assert!(bad.validate().is_err());
        let bad_level = FuelSpec {
            capacity_t: 1.0,
            level: 1.5,
            ..bad
        };
        assert!(bad_level.validate().is_err());
    }
}
