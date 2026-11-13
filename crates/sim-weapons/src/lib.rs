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
    /// Ranges against particular target domains, m, where they are shorter than `max_range_m`
    /// (a gun against aircraft). Empty: `max_range_m` for every domain.
    #[serde(default)]
    pub max_range_by_target: std::collections::BTreeMap<String, f64>,
}

impl WeaponSpec {
    /// The weapon's maximum range against a target domain (`surface`, `air`, …).
    #[must_use]
    pub fn max_range_for(&self, domain: &str) -> f64 {
        self.max_range_by_target
            .get(domain)
            .copied()
            .unwrap_or(self.max_range_m)
            .min(self.max_range_m)
    }

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

/// Ph = base × size × manoeuvre × environment, clamped to [0, 0.99]. Multiply by
/// [`countermeasure_modifier`] for soft kill.
#[must_use]
pub fn ph(base: f64, size: f64, speed: f64, sea: f64) -> f64 {
    (base * size * speed * sea).clamp(0.0, 0.99)
}

// ---- Point defence and soft kill (slice 9) ----

/// Radar cross-section of an incoming missile, m², for point-defence detection.
pub const MISSILE_RCS_M2: f64 = 0.1;
/// A radiating jammer on the target cuts an active radar seeker's Ph by this factor.
pub const ECM_SEEKER_FACTOR: f64 = 0.8;
/// A target fires a decoy at an incoming missile inside this range, metres.
pub const DECOY_RANGE_MISSILE_M: f64 = 10_000.0;
/// A target fires a torpedo countermeasure inside this range, metres.
pub const DECOY_RANGE_TORPEDO_M: f64 = 3_000.0;

/// Whether a weapon can shoot down incoming missiles (an air-capable SAM, gun or CIWS).
#[must_use]
pub fn point_defence_capable(spec: &WeaponSpec) -> bool {
    matches!(
        spec.kind,
        WeaponKind::Sam | WeaponKind::Gun | WeaponKind::Ciws
    ) && spec.target_domains.iter().any(|d| d == "air")
}

/// Seconds from detecting a threat to the first shot (original design).
#[must_use]
pub fn reaction_time_s(kind: WeaponKind) -> u64 {
    match kind {
        WeaponKind::Sam => 8,
        WeaponKind::Gun => 5,
        _ => 3,
    }
}

/// Threats a unit can engage at once with this weapon kind (illuminators or directors).
#[must_use]
pub fn fire_channels(kind: WeaponKind) -> usize {
    match kind {
        WeaponKind::Sam => 2,
        _ => 1,
    }
}

/// Seconds to intercept a threat closing head-on: range / (threat speed + interceptor speed),
/// rounded up to whole ticks, at least 1.
#[must_use]
pub fn intercept_ticks(range_m: f64, threat_mps: f64, interceptor_mps: f64) -> u64 {
    let closing = (threat_mps + interceptor_mps).max(1.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let t = (range_m.max(0.0) / closing).ceil() as u64;
    t.max(1)
}

/// Whether a decoy defends against a weapon: chaff and IR decoys (air) against radar and IR
/// seekers; torpedo countermeasures (subsurface) against acoustic homing.
#[must_use]
pub fn decoy_matches(decoy: &WeaponSpec, guidance: Guidance) -> bool {
    let air = decoy.target_domains.iter().any(|d| d == "air");
    let sub = decoy.target_domains.iter().any(|d| d == "subsurface");
    match guidance {
        Guidance::ActiveRadar | Guidance::Passive | Guidance::SemiActive => air,
        Guidance::Acoustic => sub,
        _ => false,
    }
}

/// The countermeasures Ph modifier: × (1 − decoy effect) for a matched decoy, and
/// × [`ECM_SEEKER_FACTOR`] against an active radar seeker when the target is jamming.
#[must_use]
pub fn countermeasure_modifier(
    guidance: Guidance,
    decoy_effect: Option<f64>,
    jamming: bool,
) -> f64 {
    let decoy = 1.0 - decoy_effect.unwrap_or(0.0).clamp(0.0, 1.0);
    let ecm = if jamming && guidance == Guidance::ActiveRadar {
        ECM_SEEKER_FACTOR
    } else {
        1.0
    };
    decoy * ecm
}

// ---- Mines (slice 11) ----

/// Width a mine field sweeps against a passing ship: twice the mine's actuation radius
/// (its database maximum range).
#[must_use]
pub fn mine_swept_width_m(mine: &WeaponSpec) -> f64 {
    2.0 * mine.max_range_m
}

/// Probability that a ship moving `distance_m` through a uniform field of `mines` mines over a
/// circle of `radius_m` comes within actuation range of at least one: the simple initial threat
/// P = 1 − exp(−ρ·W·d), with density ρ = mines / (π r²) and swept width W.
#[must_use]
pub fn mine_encounter_probability(
    mines: u32,
    radius_m: f64,
    swept_width_m: f64,
    distance_m: f64,
) -> f64 {
    if mines == 0 || radius_m <= 0.0 {
        return 0.0;
    }
    let density = f64::from(mines) / (core::f64::consts::PI * radius_m * radius_m);
    1.0 - libm::exp(-density * swept_width_m.max(0.0) * distance_m.max(0.0))
}

#[cfg(test)]
mod mine_tests {
    use super::*;

    #[test]
    fn simple_initial_threat_hand_values() {
        // 20 mines over a 2 km circle: ρ = 20 / (π × 4×10⁶) = 1.5915×10⁻⁶ per m²; W = 100 m.
        // Crossing the full 4 km diameter: λ = 1.5915e-6 × 100 × 4000 = 0.6366, P = 0.4710.
        let p = mine_encounter_probability(20, 2_000.0, 100.0, 4_000.0);
        assert!((p - 0.471_0).abs() < 1e-4, "{p}");
        // One 10 s step at 10 m/s (100 m): λ = 0.015 915, P = 0.015 79.
        let step = mine_encounter_probability(20, 2_000.0, 100.0, 100.0);
        assert!((step - 0.015_79).abs() < 1e-5, "{step}");
        assert_eq!(mine_encounter_probability(0, 2_000.0, 100.0, 100.0), 0.0);
        assert_eq!(mine_encounter_probability(20, 2_000.0, 100.0, 0.0), 0.0);
    }
}

#[cfg(test)]
mod pd_tests {
    use super::*;

    fn weapon(kind: WeaponKind, domains: &[&str]) -> WeaponSpec {
        WeaponSpec {
            id: "X".into(),
            name: "x".into(),
            kind,
            guidance: Guidance::Unguided,
            min_range_m: 0.0,
            max_range_m: 1.0,
            speed_mps: 0.0,
            warhead_kg: 0.0,
            base_ph: 0.3,
            target_domains: domains.iter().map(|d| (*d).to_owned()).collect(),
            max_range_by_target: std::collections::BTreeMap::new(),
        }
    }

    #[test]
    fn point_defence_rules_hand_values() {
        assert!(point_defence_capable(&weapon(WeaponKind::Sam, &["air"])));
        assert!(point_defence_capable(&weapon(
            WeaponKind::Ciws,
            &["air", "surface"]
        )));
        assert!(!point_defence_capable(&weapon(
            WeaponKind::Asm,
            &["surface"]
        )));
        assert!(!point_defence_capable(&weapon(
            WeaponKind::Gun,
            &["surface"]
        )));
        // SAM (900 m/s) against a subsonic ASM (270 m/s) at 20 km: 20 000 / 1170 = 17.1 → 18 s.
        assert_eq!(intercept_ticks(20_000.0, 270.0, 900.0), 18);
        assert_eq!(intercept_ticks(0.0, 270.0, 900.0), 1);
        assert_eq!(reaction_time_s(WeaponKind::Sam), 8);
        assert_eq!(fire_channels(WeaponKind::Sam), 2);
        assert_eq!(fire_channels(WeaponKind::Ciws), 1);
    }

    #[test]
    fn soft_kill_hand_values() {
        let chaff = weapon(WeaponKind::Decoy, &["air"]);
        let torp_decoy = weapon(WeaponKind::Decoy, &["subsurface"]);
        assert!(decoy_matches(&chaff, Guidance::ActiveRadar));
        assert!(!decoy_matches(&chaff, Guidance::Acoustic));
        assert!(decoy_matches(&torp_decoy, Guidance::Acoustic));
        // Chaff (0.3) alone: × 0.7. Chaff and jamming against a radar seeker: × 0.56.
        assert!(
            (countermeasure_modifier(Guidance::ActiveRadar, Some(0.3), false) - 0.7).abs() < 1e-12
        );
        assert!(
            (countermeasure_modifier(Guidance::ActiveRadar, Some(0.3), true) - 0.56).abs() < 1e-12
        );
        // Jamming does nothing to an IR seeker.
        assert!((countermeasure_modifier(Guidance::Passive, None, true) - 1.0).abs() < 1e-12);
    }
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
