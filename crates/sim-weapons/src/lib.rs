//! Weapon, engagement, hit-probability and damage models (master prompt §8 "Engagements",
//! "Ph and damage"), as plain deterministic functions with no ECS dependency (ADR-0005).
//! `sim-core` runs them. Model doc: `docs/models/weapons-damage.md`.

#![forbid(unsafe_code)]

mod damage;

pub use damage::{
    critical_system, fire_starts, flooding_starts, hit_damage_dp, DamageProfile, Status, System,
    CRITICAL_CHANCE, DC_FIRE_PER_MINUTE, DC_FLOOD_PER_MINUTE, FIRE_DP_PER_MINUTE,
    FLOOD_DP_PER_MINUTE,
};

use serde::{Deserialize, Serialize};

/// Weapon kinds, matching the platform database `weapons.kind` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum WeaponKind {
    Sam,
    Asm,
    Lacm,
    Torpedo,
    Gun,
    Ciws,
    Aam,
    AswRocket,
    Mine,
    DepthCharge,
    Decoy,
    Bomb,
}

impl WeaponKind {
    #[must_use]
    pub fn from_db(kind: &str) -> Option<Self> {
        Some(match kind {
            "sam" => Self::Sam,
            "asm" => Self::Asm,
            "lacm" => Self::Lacm,
            "torpedo" => Self::Torpedo,
            "gun" => Self::Gun,
            "ciws" => Self::Ciws,
            "aam" => Self::Aam,
            "asw_rocket" => Self::AswRocket,
            "mine" => Self::Mine,
            "depth_charge" => Self::DepthCharge,
            "decoy" => Self::Decoy,
            "bomb" => Self::Bomb,
            _ => return None,
        })
    }

    /// Weapons that fly as entities (seeker, time of flight). The rest resolve at once.
    #[must_use]
    pub fn flies(self) -> bool {
        matches!(
            self,
            Self::Sam | Self::Asm | Self::Lacm | Self::Torpedo | Self::Aam
        )
    }

    /// Weapons an `Engage` order can fire. Mines (slice 11) and decoys (slice 9) cannot.
    #[must_use]
    pub fn engageable(self) -> bool {
        !matches!(self, Self::Mine | Self::Decoy)
    }

    /// Stable code for the state hash.
    #[must_use]
    pub fn code(self) -> u8 {
        self as u8
    }
}

/// How a weapon finds its target, grouped from the database `guidance` text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum Guidance {
    /// Own radar seeker (fire and forget).
    ActiveRadar,
    /// Passive IR or RF seeker.
    Passive,
    /// Acoustic homing (torpedoes).
    Acoustic,
    /// Homes on the shooter's illumination: the shooter must hold the track to impact.
    SemiActive,
    /// Steered by the shooter all the way: the shooter must hold the track to impact.
    Command,
    /// Satellite, laser or terrain navigation to a fixed point.
    Navigation,
    /// No guidance after launch (guns, bombs, rockets, depth charges).
    Unguided,
}

impl Guidance {
    /// Groups a database guidance description. Order matters: the terminal seeker decides.
    #[must_use]
    pub fn from_db(guidance: &str) -> Self {
        let g = guidance;
        if g.contains("acoustic") {
            Self::Acoustic
        } else if g.contains("active_radar") && !g.contains("semi_active") {
            Self::ActiveRadar
        } else if g.contains("semi_active") {
            Self::SemiActive
        } else if g.contains("passive") || g.contains("infrared") {
            Self::Passive
        } else if g.contains("command") || g.contains("radar_directed") {
            Self::Command
        } else if g.contains("gnss") || g.contains("laser") || g.contains("terrain") {
            Self::Navigation
        } else {
            Self::Unguided
        }
    }

    /// The worst track (one-sigma major axis, metres) a weapon can be launched on. A seeker
    /// tolerates a loose track because it searches for itself; command, navigation and
    /// unguided weapons need a fire-control-quality track (original design).
    #[must_use]
    pub fn max_track_sigma_m(self) -> f64 {
        match self {
            Self::ActiveRadar => 2_000.0,
            Self::Acoustic => 1_500.0,
            Self::Passive => 1_000.0,
            Self::SemiActive => 500.0,
            Self::Command => 300.0,
            Self::Navigation | Self::Unguided => 100.0,
        }
    }

    /// Range at which a homing seeker can acquire its target, metres (`None`: no seeker).
    #[must_use]
    pub fn seeker_range_m(self) -> Option<f64> {
        match self {
            Self::ActiveRadar => Some(20_000.0),
            Self::Passive => Some(8_000.0),
            Self::Acoustic => Some(2_000.0),
            _ => None,
        }
    }
}

/// Probability that a seeker which comes within range of its target acquires it.
pub const SEEKER_ACQUISITION_PD: f64 = 0.9;

/// A weapon as the engine uses it (from the platform database).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct WeaponSpec {
    pub id: String,
    pub name: String,
    pub kind: WeaponKind,
    pub guidance: Guidance,
    pub min_range_m: f64,
    pub max_range_m: f64,
    pub speed_mps: f64,
    pub warhead_kg: f64,
    /// Single-shot probability of hit before modifiers.
    pub base_ph: f64,
    /// Target domains: any of `surface`, `subsurface`, `air`, `land`.
    pub target_domains: Vec<String>,
}

impl WeaponSpec {
    pub fn validate(&self) -> Result<(), String> {
        let ok = self.min_range_m.is_finite()
            && self.max_range_m.is_finite()
            && self.min_range_m >= 0.0
            && self.max_range_m >= self.min_range_m
            && self.speed_mps.is_finite()
            && self.speed_mps >= 0.0
            && self.warhead_kg.is_finite()
            && self.warhead_kg >= 0.0
            && (0.0..=1.0).contains(&self.base_ph);
        if ok {
            Ok(())
        } else {
            Err(format!("weapon {} has invalid figures", self.id))
        }
    }
}

/// Size modifier: small targets are harder to hit (surface and land targets only; aircraft
/// size is in the weapon's base Ph).
#[must_use]
pub fn ph_size_modifier(target_is_air: bool, length_m: f64) -> f64 {
    if target_is_air {
        1.0
    } else if length_m < 30.0 {
        0.7
    } else if length_m < 60.0 {
        0.85
    } else if length_m > 150.0 {
        1.1
    } else {
        1.0
    }
}

/// Manoeuvre modifier: fast targets are harder to hit.
#[must_use]
pub fn ph_speed_modifier(target_is_air: bool, speed_mps: f64) -> f64 {
    if target_is_air {
        if speed_mps > 300.0 {
            0.8
        } else if speed_mps > 200.0 {
            0.9
        } else {
            1.0
        }
    } else if speed_mps > 15.0 {
        0.9
    } else {
        1.0
    }
}

/// Environment modifier: rough seas hurt sea-skimming missiles and guns.
#[must_use]
pub fn ph_sea_modifier(kind: WeaponKind, sea_state: u8) -> f64 {
    match kind {
        WeaponKind::Asm if sea_state >= 6 => 0.9,
        WeaponKind::Gun | WeaponKind::Ciws if sea_state >= 5 => 0.85,
        _ => 1.0,
    }
}

/// Ph = base × size × manoeuvre × environment, clamped to [0, 0.99]. Countermeasures join in
/// slice 9 (soft kill).
#[must_use]
pub fn ph(base: f64, size: f64, speed: f64, sea: f64) -> f64 {
    (base * size * speed * sea).clamp(0.0, 0.99)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_guidance_groups() {
        assert_eq!(
            Guidance::from_db("inertial_active_radar"),
            Guidance::ActiveRadar
        );
        assert_eq!(
            Guidance::from_db("inertial_datalink_active_radar"),
            Guidance::ActiveRadar
        );
        assert_eq!(
            Guidance::from_db("inertial_semi_active_radar"),
            Guidance::SemiActive
        );
        assert_eq!(Guidance::from_db("semi_active_radar"), Guidance::SemiActive);
        assert_eq!(Guidance::from_db("passive_rf_ir"), Guidance::Passive);
        assert_eq!(Guidance::from_db("infrared"), Guidance::Passive);
        assert_eq!(
            Guidance::from_db("command_line_of_sight"),
            Guidance::Command
        );
        assert_eq!(Guidance::from_db("radar_directed_gun"), Guidance::Command);
        assert_eq!(
            Guidance::from_db("wire_active_passive_acoustic"),
            Guidance::Acoustic
        );
        assert_eq!(
            Guidance::from_db("inertial_then_acoustic"),
            Guidance::Acoustic
        );
        assert_eq!(Guidance::from_db("gnss_laser"), Guidance::Navigation);
        assert_eq!(
            Guidance::from_db("inertial_gnss_terrain"),
            Guidance::Navigation
        );
        assert_eq!(Guidance::from_db("unguided"), Guidance::Unguided);
        assert_eq!(Guidance::from_db("none"), Guidance::Unguided);
    }

    #[test]
    fn kinds_fly_or_resolve_at_once() {
        assert!(WeaponKind::Asm.flies() && WeaponKind::Torpedo.flies());
        assert!(!WeaponKind::Gun.flies() && !WeaponKind::Bomb.flies());
        assert!(!WeaponKind::Mine.engageable() && !WeaponKind::Decoy.engageable());
        assert_eq!(
            WeaponKind::from_db("asw_rocket"),
            Some(WeaponKind::AswRocket)
        );
        assert_eq!(WeaponKind::from_db("laser"), None);
    }

    #[test]
    fn ph_hand_values() {
        // Subsonic ASM (0.8) on a 135 m frigate at 15 kn in sea state 3: no modifiers, 0.8.
        let p = ph(
            0.8,
            ph_size_modifier(false, 135.0),
            ph_speed_modifier(false, 7.7),
            ph_sea_modifier(WeaponKind::Asm, 3),
        );
        assert!((p - 0.8).abs() < 1e-12);
        // Same against a 25 m fast patrol boat at 40 kn in sea state 6: 0.8 × 0.7 × 0.9 × 0.9.
        let p = ph(
            0.8,
            ph_size_modifier(false, 25.0),
            ph_speed_modifier(false, 20.6),
            ph_sea_modifier(WeaponKind::Asm, 6),
        );
        assert!((p - 0.4536).abs() < 1e-12);
        // SAM (0.75) on a 320 m/s fighter: × 0.8 = 0.6.
        let p = ph(0.75, 1.0, ph_speed_modifier(true, 320.0), 1.0);
        assert!((p - 0.6).abs() < 1e-12);
        // Large target bonus is capped at 0.99.
        assert_eq!(ph(0.95, 1.1, 1.0, 1.0), 0.99);
    }

    #[test]
    fn track_quality_and_seekers() {
        assert_eq!(Guidance::ActiveRadar.max_track_sigma_m(), 2_000.0);
        assert_eq!(Guidance::Unguided.max_track_sigma_m(), 100.0);
        assert_eq!(Guidance::ActiveRadar.seeker_range_m(), Some(20_000.0));
        assert_eq!(Guidance::SemiActive.seeker_range_m(), None);
    }
}
