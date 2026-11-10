//! Radar single-scan detection (Gate 1 interim model, `docs/models/radar-detection-gate1.md`).

/// R₅₀: the range at which single-scan Pd is 0.5, scaled from the radar's reference value by
/// the fourth root of the RCS ratio and capped at the radar's maximum range.
#[must_use]
pub fn detection_range_m(ref_range_m: f64, ref_rcs_m2: f64, rcs_m2: f64, max_range_m: f64) -> f64 {
    // (σ/σ_ref)^¼ as two square roots: sqrt is correctly rounded, so this is bit-exact everywhere.
    let scale = (rcs_m2.max(0.0) / ref_rcs_m2).sqrt().sqrt();
    (ref_range_m * scale).min(max_range_m)
}

/// Single-scan probability of detection at range `r` for a radar whose R₅₀ is `r50`.
#[must_use]
pub fn pd_per_scan(range_m: f64, r50_m: f64) -> f64 {
    if r50_m <= 0.0 {
        return 0.0;
    }
    let x = range_m / r50_m;
    let x2 = x * x;
    let x4 = x2 * x2;
    1.0 / (1.0 + x4 * x4)
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
