//! Measurement error: a detection reports the target's position with a random range and bearing
//! error (`docs/models/tracks.md` §Measurement).

use crate::kind::SensorKind;
use core::f64::consts::PI;

/// One-sigma measurement errors of a sensor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeasurementError {
    /// Range error, metres. `None` for bearing-only sensors (passive sonar, ESM, IR/EO).
    pub range_sigma_m: Option<f64>,
    /// Bearing error, degrees.
    pub bearing_sigma_deg: f64,
}

/// Generic one-sigma errors per sensor kind, used until the platform database carries per-sensor
/// accuracy. Original design, order-of-magnitude values; see the model doc for the reasoning.
/// Only radar is used in this slice: the other kinds join with their detection models.
#[must_use]
pub fn measurement_error(kind: SensorKind) -> MeasurementError {
    let (range_sigma_m, bearing_sigma_deg) = match kind {
        SensorKind::Radar => (Some(50.0), 0.5),
        SensorKind::SonarHull | SensorKind::SonarDipping => (Some(200.0), 2.0),
        SensorKind::SonarTowed | SensorKind::Sonobuoy | SensorKind::Esm => (None, 2.0),
        SensorKind::Ir | SensorKind::Eo => (None, 0.2),
        SensorKind::Visual => (Some(300.0), 1.0),
        SensorKind::Mad => (Some(100.0), 30.0),
    };
    MeasurementError {
        range_sigma_m,
        bearing_sigma_deg,
    }
}

/// Two independent standard normal values from two uniforms in [0, 1), by the Box–Muller
/// transform. `libm` keeps the transcendental functions bit-identical on every OS (ADR-0003).
#[must_use]
pub fn gaussian_pair(u1: f64, u2: f64) -> (f64, f64) {
    // 1 − u1 is in (0, 1], so the logarithm is finite.
    let r = (-2.0 * libm::log(1.0 - u1)).sqrt();
    let theta = 2.0 * PI * u2;
    (r * libm::cos(theta), r * libm::sin(theta))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_muller_hand_values() {
        // u1 = 1 − e^(−0.5) gives r = 1; u2 = 0 gives (1, 0); u2 = 0.25 gives (0, 1).
        let u1 = 1.0 - libm::exp(-0.5);
        let (a, b) = gaussian_pair(u1, 0.0);
        assert!((a - 1.0).abs() < 1e-12 && b.abs() < 1e-12);
        let (a, b) = gaussian_pair(u1, 0.25);
        assert!(a.abs() < 1e-12 && (b - 1.0).abs() < 1e-12);
        let (a, b) = gaussian_pair(0.0, 0.3);
        assert!(a == 0.0 && b == 0.0);
    }

    #[test]
    fn box_muller_moments() {
        // A deterministic midpoint grid over the unit square: mean 0, variance close to 1.
        let n = 200;
        let (mut s, mut s2, mut count) = (0.0, 0.0, 0.0);
        for i in 0..n {
            for j in 0..n {
                let u1 = (f64::from(i) + 0.5) / f64::from(n);
                let u2 = (f64::from(j) + 0.5) / f64::from(n);
                let (a, b) = gaussian_pair(u1, u2);
                s += a + b;
                s2 += a * a + b * b;
                count += 2.0;
            }
        }
        assert!((s / count).abs() < 1e-9);
        assert!((s2 / count - 1.0).abs() < 0.01);
    }

    #[test]
    fn radar_is_ranging_and_esm_is_bearing_only() {
        assert_eq!(
            measurement_error(SensorKind::Radar).range_sigma_m,
            Some(50.0)
        );
        assert_eq!(measurement_error(SensorKind::Esm).range_sigma_m, None);
    }
}
