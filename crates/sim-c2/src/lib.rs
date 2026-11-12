//! Command and control models (master prompt §8 "C2"), as plain deterministic functions with no
//! ECS dependency (ADR-0005). Model doc: `docs/models/c2-doctrine.md`.
//!
//! * **Postures:** how one side regards another (friendly, neutral, unfriendly, hostile). Postures
//!   are one-directional: side A may hold B hostile while B holds A unfriendly.
//! * **Doctrine:** weapons control status (WCS), emission control (EMCON) and the ROE setting
//!   `engage_unfriendly`. Each field can be set on a side, a group or a unit; the most specific
//!   setting wins, and unset fields fall back to the defaults in [`ResolvedDoctrine::DEFAULT`].
//! * **Release rule:** [`may_engage`] says whether doctrine allows firing at a track, given what
//!   the side knows about the track's affiliation ([`track_affiliation`]).
//! * **Comms links:** whether an order gets from a command post to a unit ([`comms`]).

#![forbid(unsafe_code)]

pub mod comms;

use serde::{Deserialize, Serialize};
use sim_sensors::Classification;

/// How one side regards another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum Posture {
    Friendly,
    /// The default between sides that have not set a posture.
    #[default]
    Neutral,
    Unfriendly,
    Hostile,
}

impl Posture {
    /// Stable code for the state hash.
    #[must_use]
    pub fn code(self) -> u8 {
        self as u8
    }
}

/// Weapons control status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum Wcs {
    /// Weapons safe: do not fire (self-defence arrives with point defence, Phase 2 slice 9).
    /// Called "hold" before 2026-11-10; files using that word still load.
    #[serde(alias = "hold")]
    Safe,
    /// Fire only at tracks identified as hostile.
    Tight,
    /// Fire at any track not identified as friendly or neutral.
    Free,
}

impl Wcs {
    /// Stable code for the state hash.
    #[must_use]
    pub fn code(self) -> u8 {
        self as u8
    }
}

/// Emission control for active sensors. Phase 2 slice 2 has two states; graded states (for
/// example navigation radar only) can be added without changing the inheritance rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum Emcon {
    /// Active sensors (radar, active sonar) may transmit.
    Active,
    /// No active emissions. Passive sensors still work.
    Silent,
}

impl Emcon {
    /// Stable code for the state hash.
    #[must_use]
    pub fn code(self) -> u8 {
        self as u8
    }
}

/// Doctrine set at one level (side, group or unit). `None` inherits from the level above.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Doctrine {
    #[serde(default)]
    pub wcs: Option<Wcs>,
    #[serde(default)]
    pub emcon: Option<Emcon>,
    /// ROE: may unfriendly (not only hostile) tracks be engaged?
    #[serde(default)]
    pub engage_unfriendly: Option<bool>,
}

/// Doctrine with every field decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ResolvedDoctrine {
    pub wcs: Wcs,
    pub emcon: Emcon,
    pub engage_unfriendly: bool,
}

impl ResolvedDoctrine {
    /// Used where no level sets a field: weapons tight, emitting, hostile tracks only.
    pub const DEFAULT: Self = Self {
        wcs: Wcs::Tight,
        emcon: Emcon::Active,
        engage_unfriendly: false,
    };
}

/// Resolves doctrine from the most specific level to the least: pass `[unit, group, side]`.
/// Each field takes the first level that sets it, else the default.
#[must_use]
pub fn resolve(levels: &[&Doctrine]) -> ResolvedDoctrine {
    let d = ResolvedDoctrine::DEFAULT;
    ResolvedDoctrine {
        wcs: levels.iter().find_map(|l| l.wcs).unwrap_or(d.wcs),
        emcon: levels.iter().find_map(|l| l.emcon).unwrap_or(d.emcon),
        engage_unfriendly: levels
            .iter()
            .find_map(|l| l.engage_unfriendly)
            .unwrap_or(d.engage_unfriendly),
    }
}

/// What a side knows about a track's affiliation: its own posture toward the track's side, but
/// only once the track is identified to class level (a class belongs to a known navy). Below that
/// the side does not know whose the track is, and gets `None`.
#[must_use]
pub fn track_affiliation(
    classification: Classification,
    posture_toward_target_side: Posture,
) -> Option<Posture> {
    (classification >= Classification::Class).then_some(posture_toward_target_side)
}

/// Whether doctrine allows firing at a track whose known affiliation is `known` (`None` =
/// not identified). This is the release rule only; weapon range, track quality and fire
/// channels belong to the engagement model (Phase 2 slice 7).
#[must_use]
pub fn may_engage(doctrine: ResolvedDoctrine, known: Option<Posture>) -> bool {
    let unfriendly_ok = doctrine.engage_unfriendly;
    match doctrine.wcs {
        Wcs::Safe => false,
        Wcs::Tight => match known {
            Some(Posture::Hostile) => true,
            Some(Posture::Unfriendly) => unfriendly_ok,
            _ => false,
        },
        Wcs::Free => match known {
            None | Some(Posture::Hostile) => true,
            Some(Posture::Unfriendly) => unfriendly_ok,
            Some(Posture::Friendly | Posture::Neutral) => false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(wcs: Wcs, engage_unfriendly: bool) -> ResolvedDoctrine {
        ResolvedDoctrine {
            wcs,
            emcon: Emcon::Active,
            engage_unfriendly,
        }
    }

    #[test]
    fn release_rule_truth_table() {
        use Posture::*;
        let knowns = [
            None,
            Some(Friendly),
            Some(Neutral),
            Some(Unfriendly),
            Some(Hostile),
        ];
        // Expected results in the order of `knowns`.
        let table = [
            (Wcs::Safe, false, [false, false, false, false, false]),
            (Wcs::Safe, true, [false, false, false, false, false]),
            (Wcs::Tight, false, [false, false, false, false, true]),
            (Wcs::Tight, true, [false, false, false, true, true]),
            (Wcs::Free, false, [true, false, false, false, true]),
            (Wcs::Free, true, [true, false, false, true, true]),
        ];
        for (wcs, eu, expected) in table {
            for (k, e) in knowns.iter().zip(expected) {
                assert_eq!(may_engage(doc(wcs, eu), *k), e, "{wcs:?} eu={eu} {k:?}");
            }
        }
    }

    #[test]
    fn most_specific_level_wins_per_field() {
        let side = Doctrine {
            wcs: Some(Wcs::Free),
            emcon: Some(Emcon::Silent),
            engage_unfriendly: Some(true),
        };
        let group = Doctrine {
            wcs: Some(Wcs::Tight),
            ..Doctrine::default()
        };
        let unit = Doctrine {
            emcon: Some(Emcon::Active),
            ..Doctrine::default()
        };
        let r = resolve(&[&unit, &group, &side]);
        assert_eq!(r.wcs, Wcs::Tight, "group overrides side");
        assert_eq!(r.emcon, Emcon::Active, "unit overrides side");
        assert!(r.engage_unfriendly, "inherited from side");
        assert_eq!(resolve(&[]), ResolvedDoctrine::DEFAULT);
        assert_eq!(resolve(&[&Doctrine::default()]), ResolvedDoctrine::DEFAULT);
    }

    #[test]
    fn affiliation_is_known_only_from_class_level() {
        use Classification::*;
        for c in [Unknown, Domain, Category] {
            assert_eq!(track_affiliation(c, Posture::Hostile), None);
        }
        for c in [Class, Identified] {
            assert_eq!(
                track_affiliation(c, Posture::Hostile),
                Some(Posture::Hostile)
            );
        }
    }

    #[test]
    fn doctrine_json_is_compact_and_partial() {
        let d: Doctrine = serde_json::from_str(r#"{"wcs":"free"}"#).unwrap();
        assert_eq!(d.wcs, Some(Wcs::Free));
        assert_eq!(d.emcon, None);
        // "safe" is the name; "hold" (the earlier name) still loads as safe and saves as safe.
        let w: Wcs = serde_json::from_str(r#""hold""#).unwrap();
        assert_eq!(w, Wcs::Safe);
        assert_eq!(serde_json::to_string(&Wcs::Safe).unwrap(), r#""safe""#);
        let p: Posture = serde_json::from_str(r#""hostile""#).unwrap();
        assert_eq!(p, Posture::Hostile);
    }
}
