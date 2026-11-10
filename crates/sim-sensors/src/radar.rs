//! Radar single-scan detection. Gate 1 core (`docs/models/radar-detection-gate1.md`) plus the
//! Phase 2 additions: clutter, weather loss and noise jamming (`docs/models/radar.md`).
//!
//! Every loss is a change in signal-to-noise ratio. Radar range scales with the fourth root of
//! SNR, so a loss of L dB scales R₅₀ by 10^(−L/40), the same rule as the RCS^¼ scaling.

use serde::{Deserialize, Serialize};

/// Targets at or below this altitude compete with surface clutter.
pub const CLUTTER_CEILING_M: f64 = 150.0;
/// Land clutter is treated as sea clutter at this sea state (original design).
pub const LAND_CLUTTER_SEA_STATE: f64 = 5.0;
/// Clutter loss per sea-state step for a radar without a database figure, dB.
pub const DEFAULT_CLUTTER_DB_PER_SEA_STATE: f64 = 1.5;
/// Beamwidth for a radar without a database figure, degrees.
pub const DEFAULT_BEAMWIDTH_DEG: f64 = 2.0;
/// Sidelobe suppression against a jammer outside the main beam, dB.
pub const SIDELOBE_DB: f64 = 30.0;

/// Conditions that every radar in the scenario shares. Constant until Phase 4 weather zones.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Environment {
    /// Douglas sea state, 0 (calm) to 9.
    #[serde(default)]
    pub sea_state: u8,
    /// Two-way propagation loss from weather (rain, fog), dB, applied to every radar.
    #[serde(default)]
    pub weather_loss_db: f64,
}

impl Environment {
    pub fn validate(&self) -> Result<(), String> {
        if self.sea_state > 9 {
            return Err("sea_state must be 0..=9".into());
        }
        if !(self.weather_loss_db.is_finite() && self.weather_loss_db >= 0.0) {
            return Err("weather_loss_db must be finite and >= 0".into());
        }
        Ok(())
    }
}

/// A noise jammer carried by a unit.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct JammerSpec {
    /// Jamming-to-noise ratio it produces in a victim radar's main beam at 1 km, dB.
    pub jn_db_at_1km: f64,
}

/// What surface the target is seen against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClutterBackground {
    /// High enough to be clear of surface clutter.
    None,
    Sea,
    Land,
}

/// R₅₀: the range at which single-scan Pd is 0.5, scaled from the radar's reference value by
/// the fourth root of the RCS ratio and capped at the radar's maximum range.
#[must_use]
pub fn detection_range_m(ref_range_m: f64, ref_rcs_m2: f64, rcs_m2: f64, max_range_m: f64) -> f64 {
    detection_range_with_loss_m(ref_range_m, ref_rcs_m2, rcs_m2, max_range_m, 0.0)
}

/// R₅₀ after a loss of `loss_db` (clutter plus weather): the uncapped range is scaled by
/// 10^(−loss/40), then capped at the radar's maximum range.
#[must_use]
pub fn detection_range_with_loss_m(
    ref_range_m: f64,
    ref_rcs_m2: f64,
    rcs_m2: f64,
    max_range_m: f64,
    loss_db: f64,
) -> f64 {
    // (σ/σ_ref)^¼ as two square roots: sqrt is correctly rounded, so this is bit-exact everywhere.
    let scale = (rcs_m2.max(0.0) / ref_rcs_m2).sqrt().sqrt();
    let factor = if loss_db == 0.0 {
        1.0
    } else {
        libm::pow(10.0, -loss_db / 40.0)
    };
    (ref_range_m * scale * factor).min(max_range_m)
}

/// Signal-to-clutter loss, dB: `db_per_sea_state` × the sea state (or the land equivalent).
#[must_use]
pub fn clutter_loss_db(background: ClutterBackground, sea_state: u8, db_per_sea_state: f64) -> f64 {
    let level = match background {
        ClutterBackground::None => return 0.0,
        ClutterBackground::Sea => f64::from(sea_state),
        ClutterBackground::Land => LAND_CLUTTER_SEA_STATE,
    };
    db_per_sea_state.max(0.0) * level
}

/// Linear jamming-to-noise ratio from one jammer `distance_m` from the radar. It falls as 1/d²
/// (one-way propagation); outside the main beam it is reduced by [`SIDELOBE_DB`].
#[must_use]
pub fn jamming_to_noise(jammer: JammerSpec, distance_m: f64, in_main_beam: bool) -> f64 {
    let d_km = (distance_m / 1000.0).max(1e-3);
    let sidelobe = if in_main_beam { 0.0 } else { SIDELOBE_DB };
    libm::pow(10.0, (jammer.jn_db_at_1km - sidelobe) / 10.0) / (d_km * d_km)
}

/// Single-scan probability of detection at range `r` for a radar whose R₅₀ is `r50`.
#[must_use]
pub fn pd_per_scan(range_m: f64, r50_m: f64) -> f64 {
    pd_jammed(range_m, r50_m, 0.0)
}

/// Pd with jamming: the noise floor rises by (1 + J/N), which shortens range by (1 + J/N)^¼:
/// x = (r / R₅₀)·(1 + J/N)^¼, Pd = 1 / (1 + x⁸).
#[must_use]
pub fn pd_jammed(range_m: f64, r50_m: f64, jn: f64) -> f64 {
    if r50_m <= 0.0 {
        return 0.0;
    }
    let x = range_m / r50_m * (1.0 + jn.max(0.0)).sqrt().sqrt();
    let x2 = x * x;
    let x4 = x2 * x2;
    1.0 / (1.0 + x4 * x4)
}

/// Burn-through range against a self-screening jammer: the range at which Pd returns to 0.5.
/// Solves (r/R₅₀)⁴·(1 + J₁·(1 km/r)²) = 1, a quadratic in u = r²:
/// u² + J₁·(1 km)²·u − R₅₀⁴ = 0, with J₁ the linear J/N at 1 km.
#[must_use]
pub fn burn_through_range_m(r50_m: f64, jammer: JammerSpec) -> f64 {
    let a = libm::pow(10.0, jammer.jn_db_at_1km / 10.0) * 1.0e6;
    let r4 = (r50_m * r50_m) * (r50_m * r50_m);
    let u = 0.5 * (-a + (a * a + 4.0 * r4).sqrt());
    u.sqrt()
}

/// Whether a jammer at bearing `jammer_bearing_deg` is inside the main beam of a radar looking
/// at `target_bearing_deg`.
#[must_use]
pub fn in_main_beam(target_bearing_deg: f64, jammer_bearing_deg: f64, beamwidth_deg: f64) -> bool {
    let mut d = (jammer_bearing_deg - target_bearing_deg).abs() % 360.0;
    if d > 180.0 {
        d = 360.0 - d;
    }
    d <= 0.5 * beamwidth_deg
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn pd_curve_hand_values() {
        assert!(close(pd_per_scan(100.0, 100.0), 0.5, 1e-15));
        // x = 0.8: x⁸ = 0.16777216 → 1/1.16777216 = 0.856329…
        assert!(close(pd_per_scan(80.0, 100.0), 1.0 / 1.167_772_16, 1e-12));
        // x = 1.2: x⁸ = 4.29981696 → 0.188686…
        assert!(close(pd_per_scan(120.0, 100.0), 1.0 / 5.299_816_96, 1e-12));
        assert!(close(pd_per_scan(0.0, 100.0), 1.0, 0.0));
        assert_eq!(pd_per_scan(10.0, 0.0), 0.0);
    }

    #[test]
    fn clutter_and_weather_scale_range_by_tenth_power_over_forty() {
        // Sea state 3 at 1.5 dB per step = 4.5 dB: R₅₀ × 10^(−4.5/40) = × 0.771 791 5.
        let loss = clutter_loss_db(ClutterBackground::Sea, 3, 1.5);
        assert!(close(loss, 4.5, 1e-12));
        assert!(close(
            detection_range_with_loss_m(100_000.0, 1.0, 1.0, 1e9, loss),
            77_179.151_558_5,
            1e-6
        ));
        // A 40 dB loss divides range by exactly 10.
        assert!(close(
            detection_range_with_loss_m(100_000.0, 1.0, 1.0, 1e9, 40.0),
            10_000.0,
            1e-6
        ));
        // The loss applies before the instrumented cap.
        assert!(close(
            detection_range_with_loss_m(120_000.0, 1.0, 10_000.0, 250_000.0, 4.5),
            250_000.0,
            0.0
        ));
        assert_eq!(clutter_loss_db(ClutterBackground::None, 6, 1.5), 0.0);
        assert!(close(
            clutter_loss_db(ClutterBackground::Land, 0, 2.0),
            10.0,
            1e-12
        ));
    }

    #[test]
    fn jamming_hand_values() {
        // 60 dB at 1 km, 10 km away in a sidelobe: 10^((60 − 30)/10) / 10² = 10.
        let j = JammerSpec { jn_db_at_1km: 60.0 };
        assert!(close(jamming_to_noise(j, 10_000.0, false), 10.0, 1e-9));
        // Main beam: 10^6 / 100 = 10 000.
        assert!(close(jamming_to_noise(j, 10_000.0, true), 10_000.0, 1e-6));
        // J/N = 15 makes the noise 16× higher: range shrinks by 16^¼ = 2, so Pd at R₅₀/2 is 0.5.
        assert!(close(pd_jammed(50.0, 100.0, 15.0), 0.5, 1e-12));
        assert!(close(
            pd_jammed(50.0, 100.0, 0.0),
            pd_per_scan(50.0, 100.0),
            0.0
        ));
    }

    #[test]
    fn burn_through_hand_value() {
        // 60 dB self-screening jammer against R₅₀ = 50 km. J₁ = 10⁶ is large, so
        // r ≈ R₅₀² / (1 km · √J₁) = 2.5×10⁹ / 10⁶ = 2.5 km (exact root 2 499.992 m).
        let j = JammerSpec { jn_db_at_1km: 60.0 };
        let r = burn_through_range_m(50_000.0, j);
        assert!(close(r, 2_499.992_187_6, 1e-3), "{r}");
        // Pd there is exactly 0.5 with the self-screening (main-beam) J/N.
        assert!(close(
            pd_jammed(r, 50_000.0, jamming_to_noise(j, r, true)),
            0.5,
            1e-9
        ));
    }

    #[test]
    fn main_beam_test_wraps_round_north() {
        assert!(in_main_beam(359.5, 0.4, 2.0));
        assert!(!in_main_beam(359.5, 1.0, 2.0));
        assert!(in_main_beam(90.0, 90.0, 1.0));
    }

    #[test]
    fn environment_validation() {
        assert!(Environment::default().validate().is_ok());
        assert!(Environment {
            sea_state: 10,
            weather_loss_db: 0.0
        }
        .validate()
        .is_err());
        assert!(Environment {
            sea_state: 3,
            weather_loss_db: -1.0
        }
        .validate()
        .is_err());
    }

    #[test]
    fn detection_range_scales_with_fourth_root_of_rcs() {
        // 16× the reference RCS doubles R₅₀: 120 km → 240 km.
        assert!(close(
            detection_range_m(120_000.0, 1.0, 16.0, 1e9),
            240_000.0,
            1e-6
        ));
        // 1/16 halves it.
        assert!(close(
            detection_range_m(120_000.0, 1.0, 1.0 / 16.0, 1e9),
            60_000.0,
            1e-6
        ));
        // Capped at the instrumented maximum.
        assert!(close(
            detection_range_m(120_000.0, 1.0, 10_000.0, 250_000.0),
            250_000.0,
            0.0
        ));
    }
}
