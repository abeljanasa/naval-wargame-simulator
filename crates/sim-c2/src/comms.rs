//! Comms links between a command post and the units it commands (master prompt §8 "C2: comms
//! links with range, jamming, and disruption affecting order delivery"). Model doc:
//! `docs/models/comms.md`. Pure functions; `sim-core` gathers positions and jammers.
//!
//! A hop from a transmitter to a receiver is up when:
//! 1. both ends can take traffic: not deeper than [`SUBMERGED_COMMS_DEPTH_M`], and comms not
//!    knocked out;
//! 2. the receiver is within the link's range;
//! 3. it is within the radio horizon (4/3-earth, as radar) between the two antennas, unless the
//!    link is beyond line of sight (HF or satellite);
//! 4. it is not jammed at either end: the jamming-to-signal ratio J/S stays below
//!    [`LINK_JS_THRESHOLD_DB`] at the receiver (for the traffic) and at the transmitter (for the
//!    acknowledgement).
//!
//! J/S for one jammer at a receiver, with free-space spreading for both paths:
//! J/S = J₁ − S₁ − A + 20·log₁₀(d_s / d_j), where J₁ is the jammer's figure at 1 km, S₁ =
//! [`LINK_SIGNAL_DB_AT_1KM`] the link's signal at 1 km, A the link's anti-jam margin, d_s the
//! signal path and d_j the jammer's distance. Several jammers add in power.

use serde::{Deserialize, Serialize};

/// A link's signal at 1 km, on the same scale as a jammer's `jn_db_at_1km` (original design:
/// a noise jammer that produces 60 dB J/N in a radar at 1 km matches a link's signal at 1 km).
pub const LINK_SIGNAL_DB_AT_1KM: f64 = 60.0;
/// Default anti-jam margin of a link (frequency hopping, coding), dB.
pub const DEFAULT_ANTI_JAM_DB: f64 = 10.0;
/// A hop is jammed when J/S reaches this, dB.
pub const LINK_JS_THRESHOLD_DB: f64 = 0.0;
/// Below this depth (altitude, m) a submarine cannot receive (no VLF modelled).
pub const SUBMERGED_COMMS_DEPTH_M: f64 = -20.0;
/// Antenna height of a post ashore when the planner gives none, m.
pub const DEFAULT_POST_ANTENNA_M: f64 = 30.0;

/// The state of a unit's link to its command post, or of a hop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum LinkStatus {
    Up,
    OutOfRange,
    BeyondHorizon,
    Jammed,
    /// Too deep to receive.
    Submerged,
    /// Comms knocked out by a critical hit (at either end).
    KnockedOut,
    /// No command post commands the unit, or every one is down.
    NoPost,
}

impl LinkStatus {
    /// Words for an order refusal.
    #[must_use]
    pub fn describe(self) -> &'static str {
        match self {
            Self::Up => "link up",
            Self::OutOfRange => "out of range",
            Self::BeyondHorizon => "beyond the radio horizon",
            Self::Jammed => "link jammed",
            Self::Submerged => "submerged too deep to receive",
            Self::KnockedOut => "comms knocked out",
            Self::NoPost => "no command post",
        }
    }
}

/// Whether one end of a hop can take traffic.
pub type Receives = Result<(), LinkStatus>;

/// The link's own figures.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinkSpec {
    pub range_m: f64,
    /// HF or satellite: no radio-horizon limit.
    pub beyond_line_of_sight: bool,
    pub anti_jam_db: f64,
}

/// J/S at a receiver for one jammer, dB.
#[must_use]
pub fn js_db(jammer_db_at_1km: f64, jammer_m: f64, signal_m: f64, anti_jam_db: f64) -> f64 {
    let dj = (jammer_m / 1000.0).max(1e-3);
    let ds = (signal_m / 1000.0).max(1e-3);
    jammer_db_at_1km - LINK_SIGNAL_DB_AT_1KM - anti_jam_db + 20.0 * libm::log10(ds / dj)
}

/// Total J/S from several jammers (`(figure at 1 km, distance m)`, summed in the order given), dB.
/// `None` with no jammers.
#[must_use]
pub fn total_js_db(jammers: &[(f64, f64)], signal_m: f64, anti_jam_db: f64) -> Option<f64> {
    if jammers.is_empty() {
        return None;
    }
    let power: f64 = jammers
        .iter()
        .map(|&(j1, d)| libm::pow(10.0, js_db(j1, d, signal_m, anti_jam_db) / 10.0))
        .sum();
    Some(10.0 * libm::log10(power))
}

/// Whether a hop is up. `jam_at_rx` and `jam_at_tx` are the jammers (figure at 1 km, distance m)
/// within line of sight of each end, in a fixed order. `horizon_m` is the radio horizon between
/// the two antennas.
#[must_use]
pub fn hop(
    link: LinkSpec,
    (rx, tx): (Receives, Receives),
    distance_m: f64,
    horizon_m: f64,
    jam_at_rx: &[(f64, f64)],
    jam_at_tx: &[(f64, f64)],
) -> LinkStatus {
    if let Err(s) = rx {
        return s;
    }
    if let Err(s) = tx {
        return s;
    }
    if distance_m > link.range_m {
        return LinkStatus::OutOfRange;
    }
    if !link.beyond_line_of_sight && distance_m > horizon_m {
        return LinkStatus::BeyondHorizon;
    }
    let jammed = |js: Option<f64>| js.is_some_and(|v| v >= LINK_JS_THRESHOLD_DB);
    if jammed(total_js_db(jam_at_rx, distance_m, link.anti_jam_db))
        || jammed(total_js_db(jam_at_tx, distance_m, link.anti_jam_db))
    {
        return LinkStatus::Jammed;
    }
    LinkStatus::Up
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOTH: (Receives, Receives) = (Ok(()), Ok(()));
    const LINK: LinkSpec = LinkSpec {
        range_m: 200_000.0,
        beyond_line_of_sight: true,
        anti_jam_db: DEFAULT_ANTI_JAM_DB,
    };

    #[test]
    fn js_hand_values() {
        // Jammer 60 dB at 1 km, 10 km from the receiver; the post 100 km away; 10 dB anti-jam:
        // 60 − 60 − 10 + 20·log10(100/10) = 10 dB → jammed.
        assert!((js_db(60.0, 10_000.0, 100_000.0, 10.0) - 10.0).abs() < 1e-9);
        // Same jammer 50 km away: −10 + 20·log10(2) = −3.98 dB → not jammed.
        let far = -10.0 + 20.0 * libm::log10(2.0);
        assert!((js_db(60.0, 50_000.0, 100_000.0, 10.0) - far).abs() < 1e-9);
        // Two equal jammers add 3.01 dB.
        let two = total_js_db(&[(60.0, 50_000.0), (60.0, 50_000.0)], 100_000.0, 10.0).unwrap();
        assert!((two - (far + 10.0 * libm::log10(2.0))).abs() < 1e-9);
        assert_eq!(total_js_db(&[], 1.0, 0.0), None);
    }

    #[test]
    fn hop_rules_in_order() {
        let up = hop(LINK, BOTH, 100_000.0, 40_000.0, &[], &[]);
        assert_eq!(
            up,
            LinkStatus::Up,
            "beyond line of sight ignores the horizon"
        );
        let los = LinkSpec {
            beyond_line_of_sight: false,
            ..LINK
        };
        assert_eq!(
            hop(los, BOTH, 100_000.0, 40_000.0, &[], &[]),
            LinkStatus::BeyondHorizon
        );
        assert_eq!(
            hop(LINK, BOTH, 250_000.0, 1e9, &[], &[]),
            LinkStatus::OutOfRange
        );
        let deep = (Err(LinkStatus::Submerged), Ok(()));
        assert_eq!(hop(LINK, deep, 1.0, 1e9, &[], &[]), LinkStatus::Submerged);
        // Jammed at the receiver, or at the transmitter (the acknowledgement).
        let near = [(60.0, 10_000.0)];
        assert_eq!(
            hop(LINK, BOTH, 100_000.0, 1e9, &near, &[]),
            LinkStatus::Jammed
        );
        assert_eq!(
            hop(LINK, BOTH, 100_000.0, 1e9, &[], &near),
            LinkStatus::Jammed
        );
        let far = [(60.0, 50_000.0)];
        assert_eq!(hop(LINK, BOTH, 100_000.0, 1e9, &far, &far), LinkStatus::Up);
    }
}
