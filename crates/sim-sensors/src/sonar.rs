//! Sonar: passive and active sonar equations, transmission loss and noise
//! (`docs/models/sonar.md`). All levels in dB (re 1 µPa, 1 m, per-band levels as used by the
//! simplified sonar equation). Phase 2 has no sound-velocity profile, layers or convergence
//! zones; those arrive in Phase 4.

use crate::kind::SensorKind;

/// Spherical spreading out to this range, cylindrical beyond (a mixed-layer simplification).
pub const SPREADING_TRANSITION_M: f64 = 5_000.0;
/// Spread of the signal excess about the detection threshold, dB (one sigma).
pub const SE_SIGMA_DB: f64 = 5.0;
/// Speed at which radiated noise and self-noise are quoted, m/s (10 kn).
pub const REFERENCE_SPEED_MPS: f64 = 10.0 * 1852.0 / 3600.0;
/// Radiated noise and self-noise rise as 40·log₁₀(v / 10 kn).
pub const NOISE_SPEED_SLOPE_DB: f64 = 40.0;
/// Slowest speed used in the noise laws (2 kn): machinery noise does not vanish when stopped.
pub const NOISE_SPEED_FLOOR_MPS: f64 = 2.0 * 1852.0 / 3600.0;
/// A dipping sonar only works with its helicopter at or below this altitude…
pub const DIPPING_MAX_ALTITUDE_M: f64 = 100.0;
/// …and at or below this speed (hovering).
pub const DIPPING_MAX_SPEED_MPS: f64 = 2.0;

/// Generic figures for a sonar of one kind (original design; per-sensor database fields later).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SonarParams {
    /// Centre frequency of the processing band, kHz (for absorption).
    pub frequency_khz: f64,
    /// Directivity index, dB.
    pub di_db: f64,
    /// Detection threshold, dB.
    pub dt_db: f64,
    /// Active source level, dB (active-capable sonars only).
    pub active_sl_db: f64,
    /// Own-ship self-noise at 10 kn, dB.
    pub self_noise_db: f64,
}

/// Generic figures by kind. Active-capable hull sonars work in the medium-frequency band;
/// passive hull arrays (submarine bow arrays) listen lower.
#[must_use]
pub fn sonar_params(kind: SensorKind, active_capable: bool) -> SonarParams {
    match kind {
        SensorKind::SonarHull if active_capable => SonarParams {
            frequency_khz: 3.5,
            di_db: 20.0,
            dt_db: 10.0,
            active_sl_db: 220.0,
            self_noise_db: 50.0,
        },
        SensorKind::SonarTowed => SonarParams {
            frequency_khz: 0.5,
            di_db: 25.0,
            dt_db: 10.0,
            active_sl_db: 0.0,
            // Streamed well astern: 15 dB quieter than a hull array.
            self_noise_db: 35.0,
        },
        SensorKind::SonarDipping => SonarParams {
            frequency_khz: 10.0,
            di_db: 15.0,
            dt_db: 10.0,
            active_sl_db: 215.0,
            // Hovering: the helicopter's noise barely reaches the body.
            self_noise_db: 35.0,
        },
        _ => SonarParams {
            frequency_khz: 1.0,
            di_db: 20.0,
            dt_db: 10.0,
            active_sl_db: 0.0,
            self_noise_db: 50.0,
        },
    }
}

/// Thorp's seawater absorption coefficient, dB/km, for frequency `f` in kHz.
#[must_use]
pub fn thorp_absorption_db_per_km(f_khz: f64) -> f64 {
    let f2 = f_khz * f_khz;
    0.11 * f2 / (1.0 + f2) + 44.0 * f2 / (4100.0 + f2) + 2.75e-4 * f2 + 0.003
}

/// One-way transmission loss, dB: spherical spreading (20·log₁₀ r) to
/// [`SPREADING_TRANSITION_M`], cylindrical (10·log₁₀) beyond, plus absorption.
#[must_use]
pub fn transmission_loss_db(range_m: f64, f_khz: f64) -> f64 {
    let r = range_m.max(1.0);
    let rt = SPREADING_TRANSITION_M;
    let spreading = if r <= rt {
        20.0 * libm::log10(r)
    } else {
        20.0 * libm::log10(rt) + 10.0 * libm::log10(r / rt)
    };
    spreading + thorp_absorption_db_per_km(f_khz) * r / 1000.0
}

/// Level at speed `v` of a noise quoted at 10 kn: L₁₀ + 40·log₁₀(max(v, 2 kn) / 10 kn).
#[must_use]
pub fn noise_at_speed_db(level_at_10kn_db: f64, speed_mps: f64) -> f64 {
    let v = speed_mps.max(NOISE_SPEED_FLOOR_MPS);
    level_at_10kn_db + NOISE_SPEED_SLOPE_DB * libm::log10(v / REFERENCE_SPEED_MPS)
}

/// Ambient sea noise, dB: 45 + 5 × sea state (a straight-line fit to the shape of the Wenz
/// curves near 1 kHz; original simplification).
#[must_use]
pub fn ambient_noise_db(sea_state: u8) -> f64 {
    45.0 + 5.0 * f64::from(sea_state)
}

/// Power sum of two levels in dB.
#[must_use]
pub fn power_sum_db(a: f64, b: f64) -> f64 {
    10.0 * libm::log10(libm::pow(10.0, a / 10.0) + libm::pow(10.0, b / 10.0))
}

/// Passive signal excess: SE = SL − TL − (NL − DI) − DT.
#[must_use]
pub fn passive_se_db(sl: f64, tl: f64, nl: f64, di: f64, dt: f64) -> f64 {
    sl - tl - (nl - di) - dt
}

/// Active (noise-limited) signal excess: SE = SL − 2TL + TS − (NL − DI) − DT.
/// Reverberation is not modelled.
#[must_use]
pub fn active_se_db(sl: f64, tl: f64, ts: f64, nl: f64, di: f64, dt: f64) -> f64 {
    sl - 2.0 * tl + ts - (nl - di) - dt
}

/// Probability of detection from signal excess: Pd = Φ(SE / σ), the normal distribution
/// function with σ = [`SE_SIGMA_DB`]. Pd is 0.5 at SE = 0.
#[must_use]
pub fn pd_from_se(se_db: f64) -> f64 {
    0.5 * (1.0 + libm::erf(se_db / (SE_SIGMA_DB * core::f64::consts::SQRT_2)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn thorp_and_transmission_loss_hand_values() {
        assert!(close(thorp_absorption_db_per_km(1.0), 0.069_004, 1e-6));
        assert!(close(thorp_absorption_db_per_km(3.5), 0.239_139, 1e-6));
        // 2 km at 1 kHz: 20·log₁₀ 2000 + 0.069 × 2 = 66.0206 + 0.1380 = 66.1586.
        assert!(close(transmission_loss_db(2_000.0, 1.0), 66.158_608, 1e-6));
        // 20 km: 20·log₁₀ 5000 + 10·log₁₀ 4 + 0.069 × 20 = 73.979 + 6.021 + 1.380 = 81.380.
        assert!(close(transmission_loss_db(20_000.0, 1.0), 81.380_082, 1e-6));
    }

    #[test]
    fn noise_rises_forty_db_per_decade_of_speed() {
        assert!(close(
            noise_at_speed_db(140.0, REFERENCE_SPEED_MPS),
            140.0,
            1e-12
        ));
        // 20 kn: + 40·log₁₀ 2 = + 12.04 dB.
        assert!(close(
            noise_at_speed_db(140.0, 2.0 * REFERENCE_SPEED_MPS),
            152.041_2,
            1e-4
        ));
        // Stopped counts as 2 kn: − 40·log₁₀ 5 = − 27.96 dB.
        assert!(close(noise_at_speed_db(140.0, 0.0), 112.041_2, 1e-4));
        assert!(close(power_sum_db(60.0, 60.0), 63.010_3, 1e-4));
        assert!(close(ambient_noise_db(3), 60.0, 0.0));
    }

    #[test]
    fn passive_towed_array_against_a_quiet_submarine() {
        // Towed array (0.5 kHz, DI 25, DT 10, self-noise 35 dB at 10 kn) on a ship at 10 kn,
        // sea state 3; submarine SL 115 dB at 10 kn, 20 km away.
        // NL = 60 ⊕ 35 = 60.014; TL = 80.555; SE = 115 − 80.555 − (60.014 − 25) − 10 = −10.569.
        let p = sonar_params(SensorKind::SonarTowed, false);
        let nl = power_sum_db(ambient_noise_db(3), p.self_noise_db);
        let tl = transmission_loss_db(20_000.0, p.frequency_khz);
        let se = passive_se_db(115.0, tl, nl, p.di_db, p.dt_db);
        assert!(close(se, -10.568_742, 1e-5), "{se}");
        assert!(close(pd_from_se(se), 0.017_268, 1e-5));
    }

    #[test]
    fn active_hull_sonar_at_ten_km() {
        // MF hull sonar (3.5 kHz, SL 220, DI 20, DT 10), own ship at 15 kn, sea state 3,
        // submarine TS 15 dB at 10 km. NL = 60 ⊕ (50 + 40·log₁₀ 1.5) = 61.779;
        // TL = 79.381; SE = 220 − 158.762 + 15 − 41.779 − 10 = 24.459.
        let p = sonar_params(SensorKind::SonarHull, true);
        let nl = power_sum_db(
            ambient_noise_db(3),
            noise_at_speed_db(p.self_noise_db, 1.5 * REFERENCE_SPEED_MPS),
        );
        let tl = transmission_loss_db(10_000.0, p.frequency_khz);
        let se = active_se_db(p.active_sl_db, tl, 15.0, nl, p.di_db, p.dt_db);
        assert!(close(se, 24.458_856, 1e-5), "{se}");
        assert!(pd_from_se(se) > 0.999_999);
    }

    #[test]
    fn pd_curve_is_the_normal_distribution() {
        assert!(close(pd_from_se(0.0), 0.5, 1e-15));
        assert!(close(pd_from_se(5.0), 0.841_344_7, 1e-7));
        assert!(close(pd_from_se(-5.0), 0.158_655_3, 1e-7));
    }
}
