//! Damage: damage points, critical hits, fire and flooding with damage control, mission kill
//! (master prompt §8 "Ph and damage"; `docs/models/weapons-damage.md`).

use serde::{Deserialize, Serialize};

/// Chance that a hit also causes a critical hit.
pub const CRITICAL_CHANCE: f64 = 0.25;
/// Damage points a burning fire costs per minute.
pub const FIRE_DP_PER_MINUTE: f64 = 2.0;
/// Damage points flooding costs per minute.
pub const FLOOD_DP_PER_MINUTE: f64 = 3.0;
/// Chance per minute that damage control puts a fire out.
pub const DC_FIRE_PER_MINUTE: f64 = 0.25;
/// Chance per minute that damage control stops flooding.
pub const DC_FLOOD_PER_MINUTE: f64 = 0.2;

/// A platform's resistance to damage (platform database `damage_profiles`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct DamageProfile {
    pub damage_points: f64,
    /// Fraction of damage points lost at which the unit is mission-killed.
    pub mission_kill_fraction: f64,
    pub fire_susceptibility: f64,
    pub flooding_susceptibility: f64,
}

impl DamageProfile {
    pub fn validate(&self) -> Result<(), String> {
        let unit = |v: f64| (0.0..=1.0).contains(&v);
        if self.damage_points.is_finite()
            && self.damage_points > 0.0
            && unit(self.mission_kill_fraction)
            && unit(self.fire_susceptibility)
            && unit(self.flooding_susceptibility)
        {
            Ok(())
        } else {
            Err("invalid damage profile".into())
        }
    }
}

/// Systems a critical hit can knock out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum System {
    Sensors,
    Propulsion,
    Weapons,
    FlightDeck,
    Comms,
}

impl System {
    pub const ALL: [System; 5] = [
        System::Sensors,
        System::Propulsion,
        System::Weapons,
        System::FlightDeck,
        System::Comms,
    ];

    /// Stable code for the state hash.
    #[must_use]
    pub fn code(self) -> u8 {
        self as u8
    }
}

/// What a unit can still do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Operational,
    /// Afloat or flying but unable to fight (damage beyond the mission-kill fraction, or both
    /// propulsion and weapons knocked out).
    MissionKilled,
    /// Sunk, shot down or destroyed.
    Destroyed,
}

impl Status {
    /// Status from damage taken and critical hits.
    #[must_use]
    pub fn assess(profile: &DamageProfile, damage: f64, crits: &[System]) -> Self {
        if damage >= profile.damage_points {
            Self::Destroyed
        } else if damage >= profile.mission_kill_fraction * profile.damage_points
            || (crits.contains(&System::Propulsion) && crits.contains(&System::Weapons))
        {
            Self::MissionKilled
        } else {
            Self::Operational
        }
    }
}

/// Damage points from one hit: 1 per kilogram of warhead (original design).
#[must_use]
pub fn hit_damage_dp(warhead_kg: f64) -> f64 {
    warhead_kg.max(0.0)
}

/// Which system a critical hit strikes, from a uniform draw in [0, 1): equal fifths.
#[must_use]
pub fn critical_system(u: f64) -> System {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let i = ((u.clamp(0.0, 0.999_999_999) * 5.0) as usize).min(4);
    System::ALL[i]
}

/// Chance a hit starts a fire: 0.6 × the profile's fire susceptibility.
#[must_use]
pub fn fire_starts(profile: &DamageProfile) -> f64 {
    0.6 * profile.fire_susceptibility
}

/// Chance a hit starts flooding: the full flooding susceptibility for a torpedo (below the
/// waterline), a fifth of it otherwise.
#[must_use]
pub fn flooding_starts(profile: &DamageProfile, torpedo: bool) -> f64 {
    if torpedo {
        profile.flooding_susceptibility
    } else {
        0.2 * profile.flooding_susceptibility
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRIGATE: DamageProfile = DamageProfile {
        damage_points: 450.0,
        mission_kill_fraction: 0.45,
        fire_susceptibility: 0.5,
        flooding_susceptibility: 0.5,
    };

    #[test]
    fn frigate_hand_values() {
        // A 220 kg subsonic ASM does 220 DP: 48.9 % of 450, past the 45 % mission-kill line.
        let one = hit_damage_dp(220.0);
        assert_eq!(Status::assess(&FRIGATE, one, &[]), Status::MissionKilled);
        // A 40 kg SAM-sized hit leaves it operational; a second ASM sinks it (440 + fire).
        assert_eq!(Status::assess(&FRIGATE, 40.0, &[]), Status::Operational);
        assert_eq!(Status::assess(&FRIGATE, 450.0, &[]), Status::Destroyed);
        // Losing propulsion and weapons is a mission kill whatever the damage.
        assert_eq!(
            Status::assess(&FRIGATE, 10.0, &[System::Propulsion, System::Weapons]),
            Status::MissionKilled
        );
        // Fire 0.6 × 0.5 = 0.3; flooding 0.5 from a torpedo, 0.1 otherwise.
        assert!((fire_starts(&FRIGATE) - 0.3).abs() < 1e-12);
        assert!((flooding_starts(&FRIGATE, true) - 0.5).abs() < 1e-12);
        assert!((flooding_starts(&FRIGATE, false) - 0.1).abs() < 1e-12);
    }

    #[test]
    fn critical_system_fifths() {
        assert_eq!(critical_system(0.0), System::Sensors);
        assert_eq!(critical_system(0.25), System::Propulsion);
        assert_eq!(critical_system(0.5), System::Weapons);
        assert_eq!(critical_system(0.7), System::FlightDeck);
        assert_eq!(critical_system(0.999), System::Comms);
    }
}
