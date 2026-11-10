//! Visual, EO/IR and MAD sensors, and daylight (`docs/models/optical-mad.md`).

use crate::classify::Classification;
use crate::kind::SensorKind;

/// R₅₀ is this fraction of the sensor's instrumented maximum range against a full-size target.
pub const OPTICAL_R50_FRACTION: f64 = 0.7;
/// Targets at least this long count as full size.
pub const FULL_SIZE_LENGTH_M: f64 = 100.0;
/// IR sees through haze better than the eye: visibility × this.
pub const IR_VISIBILITY_FACTOR: f64 = 1.5;
/// Identification to class within this fraction of R₅₀.
pub const CLASS_ID_FRACTION: f64 = 0.3;
/// Identification to category within this fraction of R₅₀.
pub const CATEGORY_ID_FRACTION: f64 = 0.6;
/// MAD R₅₀ as a fraction of its maximum (slant) range.
pub const MAD_R50_FRACTION: f64 = 0.6;
/// One-sigma position error of a MAD contact, metres (it marks "about here").
pub const MAD_POSITION_SIGMA_M: f64 = 300.0;

/// Light at a place and time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Light {
    /// Sun above the horizon.
    Day,
    /// Sun 0–6° below the horizon (civil twilight).
    Twilight,
    Night,
}

/// Solar elevation, degrees, at `unix_s` (UTC seconds) and a position. A simplified solar
/// position: declination δ = −23.44° · cos(360°/365 · (N + 10)) and hour angle from mean solar
/// time (no equation of time), accurate to about a degree (original simplification).
#[must_use]
pub fn sun_elevation_deg(unix_s: f64, lat_deg: f64, lon_deg: f64) -> f64 {
    let day = unix_s / 86_400.0;
    let n = day.rem_euclid(365.2422);
    let dec = (-23.44 * libm::cos((360.0 / 365.0 * (n + 10.0)).to_radians())).to_radians();
    let utc_hours = unix_s.rem_euclid(86_400.0) / 3600.0;
    let hour_angle = (15.0 * (utc_hours + lon_deg / 15.0 - 12.0)).to_radians();
    let lat = lat_deg.to_radians();
    let s =
        libm::sin(lat) * libm::sin(dec) + libm::cos(lat) * libm::cos(dec) * libm::cos(hour_angle);
    libm::asin(s.clamp(-1.0, 1.0)).to_degrees()
}

/// Day, twilight or night from the sun's elevation.
#[must_use]
pub fn light(sun_elevation_deg: f64) -> Light {
    if sun_elevation_deg > 0.0 {
        Light::Day
    } else if sun_elevation_deg > -6.0 {
        Light::Twilight
    } else {
        Light::Night
    }
}

/// The R₅₀ multiplier for light: the eye needs daylight; a combined EO/IR director falls back
/// to its IR channel (× 0.75); IR does not care.
#[must_use]
pub fn light_factor(kind: SensorKind, light: Light) -> f64 {
    match (kind, light) {
        (SensorKind::Visual, Light::Twilight) => 0.5,
        (SensorKind::Visual, Light::Night) => 0.1,
        (SensorKind::Eo, Light::Twilight | Light::Night) => 0.75,
        _ => 1.0,
    }
}

/// The R₅₀ multiplier for target size: √(length / 100 m), at most 1. Aircraft count as full size
/// to IR sensors (hot engines).
#[must_use]
pub fn size_factor(kind: SensorKind, target_is_air: bool, length_m: f64) -> f64 {
    if target_is_air && kind == SensorKind::Ir {
        return 1.0;
    }
    (length_m.max(0.0) / FULL_SIZE_LENGTH_M).min(1.0).sqrt()
}

/// Optical R₅₀: 0.7 × max range × size × light, but never beyond the visibility (× 1.5 for IR
/// and for the IR channel of an EO director out of daylight).
#[must_use]
pub fn optical_r50_m(
    kind: SensorKind,
    max_range_m: f64,
    size: f64,
    light: Light,
    visibility_m: f64,
) -> f64 {
    let lf = light_factor(kind, light);
    let ir = kind == SensorKind::Ir || (kind == SensorKind::Eo && light != Light::Day);
    let vis = if ir {
        visibility_m * IR_VISIBILITY_FACTOR
    } else {
        visibility_m
    };
    (OPTICAL_R50_FRACTION * max_range_m * size * lf).min(vis)
}

/// How far a close look identifies a target: class within 0.3 R₅₀, category within 0.6 R₅₀,
/// else domain.
#[must_use]
pub fn optical_identification(range_m: f64, r50_m: f64) -> Classification {
    if range_m <= CLASS_ID_FRACTION * r50_m {
        Classification::Class
    } else if range_m <= CATEGORY_ID_FRACTION * r50_m {
        Classification::Category
    } else {
        Classification::Domain
    }
}

/// MAD R₅₀ (slant range).
#[must_use]
pub fn mad_r50_m(max_range_m: f64) -> f64 {
    MAD_R50_FRACTION * max_range_m
}

/// Slant range from horizontal distance and the height difference.
#[must_use]
pub fn slant_range_m(horizontal_m: f64, height_difference_m: f64) -> f64 {
    libm::hypot(horizontal_m, height_difference_m)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol
    }

    /// 2026-11-10 00:00 UTC.
    const T: f64 = 1_794_268_800.0;

    #[test]
    fn sun_elevation_hand_values() {
        // Declination on 10 November ≈ −17.7° (true value −17.3°: within the stated degree).
        // Bass Strait (−39.3°, 146°E) at 00Z is mid-morning local time: 53.5° up.
        assert!(close(sun_elevation_deg(T, -39.3, 146.0), 53.483, 0.01));
        // Greenwich meridian at the equator: midnight −72.3°, noon +72.2°.
        assert!(close(sun_elevation_deg(T, 0.0, 0.0), -72.308, 0.01));
        assert!(close(
            sun_elevation_deg(T + 43_200.0, 0.0, 0.0),
            72.176,
            0.01
        ));
        // Bass Strait at 12Z is night.
        let night = sun_elevation_deg(T + 43_200.0, -39.3, 146.0);
        assert!(close(night, -24.638, 0.01));
        assert_eq!(light(night), Light::Night);
        assert_eq!(light(-3.0), Light::Twilight);
        assert_eq!(light(0.5), Light::Day);
    }

    #[test]
    fn optical_r50_hand_values() {
        // Lookout (30 km max), 100 m ship, day, clear: 0.7 × 30 = 21 km.
        let full = size_factor(SensorKind::Visual, false, 100.0);
        assert!(close(
            optical_r50_m(SensorKind::Visual, 30_000.0, full, Light::Day, 40_000.0),
            21_000.0,
            1e-9
        ));
        // A 25 m patrol boat is half the range: √0.25 = 0.5.
        let small = size_factor(SensorKind::Visual, false, 25.0);
        assert!(close(small, 0.5, 1e-12));
        // At night the eye sees a tenth as far; IR is unaffected.
        assert!(close(
            optical_r50_m(SensorKind::Visual, 30_000.0, full, Light::Night, 40_000.0),
            2_100.0,
            1e-9
        ));
        assert!(close(
            optical_r50_m(SensorKind::Ir, 80_000.0, 1.0, Light::Night, 40_000.0),
            56_000.0,
            1e-9
        ));
        // Haze: 5 km visibility caps the eye at 5 km and IR at 7.5 km.
        assert!(close(
            optical_r50_m(SensorKind::Visual, 30_000.0, full, Light::Day, 5_000.0),
            5_000.0,
            1e-9
        ));
        assert!(close(
            optical_r50_m(SensorKind::Ir, 80_000.0, 1.0, Light::Day, 5_000.0),
            7_500.0,
            1e-9
        ));
        // Aircraft are full size to IR, small to the eye.
        assert_eq!(size_factor(SensorKind::Ir, true, 15.0), 1.0);
        assert!(close(
            size_factor(SensorKind::Visual, true, 16.0),
            0.4,
            1e-12
        ));
    }

    #[test]
    fn identification_bands_and_mad() {
        assert_eq!(
            optical_identification(3_000.0, 21_000.0),
            Classification::Class
        );
        assert_eq!(
            optical_identification(12_000.0, 21_000.0),
            Classification::Category
        );
        assert_eq!(
            optical_identification(15_000.0, 21_000.0),
            Classification::Domain
        );
        assert!(close(mad_r50_m(1_500.0), 900.0, 1e-12));
        // Aircraft at 150 m over a submarine at −100 m, 600 m away horizontally: 650 m slant.
        assert!(close(slant_range_m(600.0, 250.0), 650.0, 1e-9));
    }
}
