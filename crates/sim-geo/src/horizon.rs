//! Radar horizon for a 4/3-effective-earth-radius atmosphere.
//!
//! d = √(2·k·R·h_s) + √(2·k·R·h_t), with k = 4/3 and R = 6 371 000 m, which gives
//! d ≈ 4 121.8·(√h_s + √h_t) metres. The project spec rounds this to the conventional
//! 4.12 km/√m, and we use that rounded value (`docs/models/radar-horizon.md`).
//! Terrain masking is not modelled here; it comes from `sim-geo` terrain sampling in a later change.

/// Metres of horizon range per √(metre of height): 4.12 km/√m.
pub const RADAR_HORIZON_COEFF_M: f64 = 4_120.0;

/// Radar horizon range in metres between a sensor at `sensor_height_m` and a target at
/// `target_height_m`, both above mean sea level. Negative heights (e.g. a submerged
/// periscope reference) are clamped to zero; non-finite heights give 0.
#[must_use]
pub fn radar_horizon_m(sensor_height_m: f64, target_height_m: f64) -> f64 {
    let hs = clamp_height(sensor_height_m);
    let ht = clamp_height(target_height_m);
    // sqrt is IEEE-754 correctly rounded, so std sqrt is deterministic (ADR-0003).
    RADAR_HORIZON_COEFF_M * (hs.sqrt() + ht.sqrt())
}

/// True when a target at `range_m` is not beyond the radar horizon.
#[must_use]
pub fn within_radar_horizon(sensor_height_m: f64, target_height_m: f64, range_m: f64) -> bool {
    range_m <= radar_horizon_m(sensor_height_m, target_height_m)
}

fn clamp_height(h: f64) -> f64 {
    if h.is_finite() && h > 0.0 {
        h
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn mast_25m_vs_surface_target() {
        // 4.12 × √25 = 20.6 km
        assert!(close(radar_horizon_m(25.0, 0.0), 20_600.0, 1e-9));
    }

    #[test]
    fn mast_16m_vs_9m_target() {
        // 4.12 × (4 + 3) = 28.84 km
        assert!(close(radar_horizon_m(16.0, 9.0), 28_840.0, 1e-9));
    }

    #[test]
    fn sea_skimmer_vs_frigate_radar() {
        // Sensor 20 m, missile 5 m: 4.12 × (4.4721 + 2.2361) = 27.638 km
        assert!(close(radar_horizon_m(20.0, 5.0), 27_637.8, 0.1));
    }

    #[test]
    fn aircraft_at_10000m_vs_surface() {
        // 4.12 × 100 = 412 km
        assert!(close(radar_horizon_m(10_000.0, 0.0), 412_000.0, 1e-6));
    }

    #[test]
    fn coefficient_matches_four_thirds_earth_derivation() {
        let k = 4.0 / 3.0;
        let r = 6_371_000.0_f64;
        let exact = (2.0 * k * r).sqrt(); // 4121.8 m/√m
        assert!((exact - RADAR_HORIZON_COEFF_M).abs() / exact < 0.001);
    }

    #[test]
    fn negative_and_nan_heights_clamp_to_zero() {
        assert_eq!(radar_horizon_m(-5.0, 0.0), 0.0);
        assert_eq!(radar_horizon_m(f64::NAN, 0.0), 0.0);
    }

    #[test]
    fn within_horizon_boundary() {
        assert!(within_radar_horizon(25.0, 0.0, 20_600.0));
        assert!(!within_radar_horizon(25.0, 0.0, 20_600.1));
    }
}
