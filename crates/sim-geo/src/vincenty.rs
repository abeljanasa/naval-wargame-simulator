//! Vincenty's direct and inverse geodesic formulae on the WGS-84 ellipsoid.
//!
//! Reference: T. Vincenty, "Direct and inverse solutions of geodesics on the ellipsoid with
//! application of nested equations", Survey Review XXIII (176), 1975.
//!
//! Accuracy is about 0.5 mm on the ellipsoid. The inverse iteration does not converge for some
//! nearly antipodal points. For those we fall back to a spherical great circle on the mean
//! radius (error ≤ ~0.5 %) and report it through [`InverseMethod::SphericalFallback`] so callers
//! can log it. Karney's algorithm is the planned replacement if this ever matters (backlog).

use crate::angle::{normalize_azimuth_deg, normalize_lon_deg};
use crate::ellipsoid::{WGS84_A, WGS84_B, WGS84_F};
use crate::GeoPoint;
use libm::{atan2, cos, sin, tan};

const MAX_ITERATIONS: u32 = 200;
const CONVERGENCE: f64 = 1e-12;

/// How an [`Inverse`] result was obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InverseMethod {
    /// Vincenty's ellipsoidal iteration converged.
    Vincenty,
    /// The iteration did not converge (nearly antipodal points), so a spherical great circle was used instead.
    SphericalFallback,
}

/// Result of the inverse problem: distance and azimuths between two points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Inverse {
    /// Geodesic distance, metres.
    pub distance_m: f64,
    /// Forward azimuth at the start point, degrees clockwise from north, 0..360.
    pub azimuth1_deg: f64,
    /// Forward azimuth at the end point (direction of travel on arrival), 0..360.
    pub azimuth2_deg: f64,
    pub method: InverseMethod,
}

/// Result of the direct problem: destination and arrival heading.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Direct {
    pub point: GeoPoint,
    /// Forward azimuth at the destination, 0..360.
    pub azimuth2_deg: f64,
}

fn reduced_latitude(lat_rad: f64) -> (f64, f64) {
    let tan_u = (1.0 - WGS84_F) * tan(lat_rad);
    let cos_u = 1.0 / (1.0 + tan_u * tan_u).sqrt();
    (tan_u * cos_u, cos_u)
}

fn series_a_b(cos_sq_alpha: f64) -> (f64, f64) {
    let u_sq = cos_sq_alpha * (WGS84_A * WGS84_A - WGS84_B * WGS84_B) / (WGS84_B * WGS84_B);
    let a = 1.0 + u_sq / 16384.0 * (4096.0 + u_sq * (-768.0 + u_sq * (320.0 - 175.0 * u_sq)));
    let b = u_sq / 1024.0 * (256.0 + u_sq * (-128.0 + u_sq * (74.0 - 47.0 * u_sq)));
    (a, b)
}

fn delta_sigma(b: f64, sin_sigma: f64, cos_sigma: f64, cos_2sm: f64) -> f64 {
    b * sin_sigma
        * (cos_2sm
            + b / 4.0
                * (cos_sigma * (-1.0 + 2.0 * cos_2sm * cos_2sm)
                    - b / 6.0
                        * cos_2sm
                        * (-3.0 + 4.0 * sin_sigma * sin_sigma)
                        * (-3.0 + 4.0 * cos_2sm * cos_2sm)))
}

/// Solves the inverse geodesic problem from `p1` to `p2`.
#[must_use]
pub fn inverse(p1: GeoPoint, p2: GeoPoint) -> Inverse {
    let phi1 = p1.lat_deg.to_radians();
    let phi2 = p2.lat_deg.to_radians();
    let l = normalize_lon_deg(p2.lon_deg - p1.lon_deg).to_radians();

    let (sin_u1, cos_u1) = reduced_latitude(phi1);
    let (sin_u2, cos_u2) = reduced_latitude(phi2);

    let mut lambda = l;
    let mut iterations = 0;
    loop {
        let sin_lambda = sin(lambda);
        let cos_lambda = cos(lambda);
        let t1 = cos_u2 * sin_lambda;
        let t2 = cos_u1 * sin_u2 - sin_u1 * cos_u2 * cos_lambda;
        let sin_sigma = (t1 * t1 + t2 * t2).sqrt();
        if sin_sigma == 0.0 {
            // Coincident points.
            return Inverse {
                distance_m: 0.0,
                azimuth1_deg: 0.0,
                azimuth2_deg: 0.0,
                method: InverseMethod::Vincenty,
            };
        }
        let cos_sigma = sin_u1 * sin_u2 + cos_u1 * cos_u2 * cos_lambda;
        let sigma = atan2(sin_sigma, cos_sigma);
        let sin_alpha = cos_u1 * cos_u2 * sin_lambda / sin_sigma;
        let cos_sq_alpha = 1.0 - sin_alpha * sin_alpha;
        // On the equator cos²α = 0 and cos2σm is defined as 0.
        let cos_2sm = if cos_sq_alpha == 0.0 {
            0.0
        } else {
            cos_sigma - 2.0 * sin_u1 * sin_u2 / cos_sq_alpha
        };
        let c = WGS84_F / 16.0 * cos_sq_alpha * (4.0 + WGS84_F * (4.0 - 3.0 * cos_sq_alpha));
        let lambda_prev = lambda;
        lambda = l
            + (1.0 - c)
                * WGS84_F
                * sin_alpha
                * (sigma
                    + c * sin_sigma * (cos_2sm + c * cos_sigma * (-1.0 + 2.0 * cos_2sm * cos_2sm)));

        iterations += 1;
        let converged = (lambda - lambda_prev).abs() < CONVERGENCE;
        if converged {
            let (a, b) = series_a_b(cos_sq_alpha);
            let ds = delta_sigma(b, sin_sigma, cos_sigma, cos_2sm);
            let s = WGS84_B * a * (sigma - ds);
            let sin_l = sin(lambda);
            let cos_l = cos(lambda);
            let az1 = atan2(cos_u2 * sin_l, cos_u1 * sin_u2 - sin_u1 * cos_u2 * cos_l);
            let az2 = atan2(cos_u1 * sin_l, -sin_u1 * cos_u2 + cos_u1 * sin_u2 * cos_l);
            return Inverse {
                distance_m: s,
                azimuth1_deg: normalize_azimuth_deg(az1.to_degrees()),
                azimuth2_deg: normalize_azimuth_deg(az2.to_degrees()),
                method: InverseMethod::Vincenty,
            };
        }
        if iterations >= MAX_ITERATIONS || lambda.abs() > std::f64::consts::PI {
            return spherical_inverse(p1, p2);
        }
    }
}

/// Great-circle inverse on a sphere of mean radius R₁ = (2a + b)/3.
fn spherical_inverse(p1: GeoPoint, p2: GeoPoint) -> Inverse {
    let r = (2.0 * WGS84_A + WGS84_B) / 3.0;
    let phi1 = p1.lat_deg.to_radians();
    let phi2 = p2.lat_deg.to_radians();
    let dlon = normalize_lon_deg(p2.lon_deg - p1.lon_deg).to_radians();
    let dphi = phi2 - phi1;
    let h = sin(dphi / 2.0) * sin(dphi / 2.0)
        + cos(phi1) * cos(phi2) * sin(dlon / 2.0) * sin(dlon / 2.0);
    let central = 2.0 * atan2(h.sqrt(), (1.0 - h).max(0.0).sqrt());
    let az1 = atan2(
        sin(dlon) * cos(phi2),
        cos(phi1) * sin(phi2) - sin(phi1) * cos(phi2) * cos(dlon),
    );
    // Arrival azimuth = reverse of the initial bearing from p2 back to p1.
    let back = atan2(
        sin(-dlon) * cos(phi1),
        cos(phi2) * sin(phi1) - sin(phi2) * cos(phi1) * cos(-dlon),
    );
    Inverse {
        distance_m: r * central,
        azimuth1_deg: normalize_azimuth_deg(az1.to_degrees()),
        azimuth2_deg: normalize_azimuth_deg(back.to_degrees() + 180.0),
        method: InverseMethod::SphericalFallback,
    }
}

/// Solves the direct geodesic problem: start at `p1`, head `azimuth_deg`, travel `distance_m`.
///
/// Negative distances travel backwards along the same geodesic.
#[must_use]
pub fn direct(p1: GeoPoint, azimuth_deg: f64, distance_m: f64) -> Direct {
    if distance_m == 0.0 {
        return Direct {
            point: p1,
            azimuth2_deg: normalize_azimuth_deg(azimuth_deg),
        };
    }
    let phi1 = p1.lat_deg.to_radians();
    let alpha1 = azimuth_deg.to_radians();
    let sin_alpha1 = sin(alpha1);
    let cos_alpha1 = cos(alpha1);

    let (sin_u1, cos_u1) = reduced_latitude(phi1);

    // tanU1 / cosα1, written as atan2(sinU1, cosU1·cosα1) to stay finite at the poles.
    let sigma1 = atan2(sin_u1, cos_u1 * cos_alpha1);
    let sin_alpha = cos_u1 * sin_alpha1;
    let cos_sq_alpha = 1.0 - sin_alpha * sin_alpha;
    let (a, b) = series_a_b(cos_sq_alpha);

    let sigma0 = distance_m / (WGS84_B * a);
    let mut sigma = sigma0;
    let mut sin_sigma;
    let mut cos_sigma;
    let mut cos_2sm;
    let mut iterations = 0;
    loop {
        cos_2sm = cos(2.0 * sigma1 + sigma);
        sin_sigma = sin(sigma);
        cos_sigma = cos(sigma);
        let ds = delta_sigma(b, sin_sigma, cos_sigma, cos_2sm);
        let sigma_prev = sigma;
        sigma = sigma0 + ds;
        iterations += 1;
        if (sigma - sigma_prev).abs() < CONVERGENCE || iterations >= MAX_ITERATIONS {
            break;
        }
    }
    // Recompute the trigonometric terms for the final σ.
    cos_2sm = cos(2.0 * sigma1 + sigma);
    sin_sigma = sin(sigma);
    cos_sigma = cos(sigma);

    let x = sin_u1 * sin_sigma - cos_u1 * cos_sigma * cos_alpha1;
    let phi2 = atan2(
        sin_u1 * cos_sigma + cos_u1 * sin_sigma * cos_alpha1,
        (1.0 - WGS84_F) * (sin_alpha * sin_alpha + x * x).sqrt(),
    );
    let lambda = atan2(
        sin_sigma * sin_alpha1,
        cos_u1 * cos_sigma - sin_u1 * sin_sigma * cos_alpha1,
    );
    let c = WGS84_F / 16.0 * cos_sq_alpha * (4.0 + WGS84_F * (4.0 - 3.0 * cos_sq_alpha));
    let l = lambda
        - (1.0 - c)
            * WGS84_F
            * sin_alpha
            * (sigma
                + c * sin_sigma * (cos_2sm + c * cos_sigma * (-1.0 + 2.0 * cos_2sm * cos_2sm)));
    let alpha2 = atan2(sin_alpha, -x);

    Direct {
        point: GeoPoint::new(phi2.to_degrees(), p1.lon_deg + l.to_degrees()),
        azimuth2_deg: normalize_azimuth_deg(alpha2.to_degrees()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn dms(deg: f64, min: f64, sec: f64) -> f64 {
        let sign = if deg < 0.0 { -1.0 } else { 1.0 };
        sign * (deg.abs() + min / 60.0 + sec / 3600.0)
    }

    // Geoscience Australia worked example (Vincenty's formulae), GDA/GRS80.
    // GRS80 and WGS-84 differ only in f at the 1e-11 level: under 0.1 mm over this 55 km line.
    fn flinders_peak() -> GeoPoint {
        GeoPoint::new(dms(-37.0, 57.0, 3.720_30), dms(144.0, 25.0, 29.524_40))
    }
    fn buninyong() -> GeoPoint {
        GeoPoint::new(dms(-37.0, 39.0, 10.156_10), dms(143.0, 55.0, 35.383_90))
    }

    /// 0.01 arc-second, the precision of the published azimuths, plus half a unit of rounding.
    const AZ_TOL_DEG: f64 = 0.006 / 3600.0;

    #[test]
    fn inverse_flinders_peak_to_buninyong() {
        let r = inverse(flinders_peak(), buninyong());
        assert_eq!(r.method, InverseMethod::Vincenty);
        assert!(
            (r.distance_m - 54_972.271).abs() < 0.001,
            "s = {}",
            r.distance_m
        );
        let az1 = dms(306.0, 52.0, 5.37);
        assert!(
            (r.azimuth1_deg - az1).abs() < AZ_TOL_DEG,
            "az1 = {}",
            r.azimuth1_deg
        );
        // Published reverse azimuth 127°10′25.07″ → forward azimuth at P2 = reverse + 180°.
        let az2 = dms(127.0, 10.0, 25.07) + 180.0;
        assert!(
            (r.azimuth2_deg - az2).abs() < AZ_TOL_DEG,
            "az2 = {}",
            r.azimuth2_deg
        );
    }

    #[test]
    fn direct_flinders_peak_to_buninyong() {
        let r = direct(flinders_peak(), dms(306.0, 52.0, 5.37), 54_972.271);
        let b = buninyong();
        // 0.0001″ of latitude ≈ 3 mm; allow the rounding of the published inputs.
        let tol = 0.0005 / 3600.0;
        assert!(
            (r.point.lat_deg - b.lat_deg).abs() < tol,
            "lat {}",
            r.point.lat_deg
        );
        assert!(
            (r.point.lon_deg - b.lon_deg).abs() < tol,
            "lon {}",
            r.point.lon_deg
        );
        let az2 = dms(127.0, 10.0, 25.07) + 180.0;
        assert!((r.azimuth2_deg - az2).abs() < AZ_TOL_DEG);
    }

    #[test]
    fn quarter_meridian_length() {
        // WGS-84 equator-to-pole meridian arc: 10 001 965.729 m (NIMA TR8350.2).
        let r = inverse(GeoPoint::new(0.0, 0.0), GeoPoint::new(90.0, 0.0));
        assert!(
            (r.distance_m - 10_001_965.729).abs() < 0.001,
            "s = {}",
            r.distance_m
        );
        assert!(r.azimuth1_deg.abs() < 1e-9);
    }

    #[test]
    fn one_degree_of_equator() {
        // On the equator the geodesic is the equator itself: s = a·Δλ.
        let r = inverse(GeoPoint::new(0.0, 0.0), GeoPoint::new(0.0, 1.0));
        let expected = WGS84_A * 1.0_f64.to_radians();
        assert!(
            (r.distance_m - expected).abs() < 1e-6,
            "s = {}",
            r.distance_m
        );
        assert!((r.azimuth1_deg - 90.0).abs() < 1e-9);
    }

    #[test]
    fn coincident_points() {
        let p = GeoPoint::new(10.0, 20.0);
        assert_eq!(inverse(p, p).distance_m, 0.0);
    }

    #[test]
    fn nearly_antipodal_falls_back_without_panicking() {
        let r = inverse(GeoPoint::new(0.0, 0.0), GeoPoint::new(0.5, 179.7));
        assert!(r.distance_m.is_finite());
        assert!(r.distance_m > 19_900_000.0 && r.distance_m < 20_050_000.0);
    }

    #[test]
    fn crossing_the_antimeridian() {
        let r = inverse(GeoPoint::new(0.0, 179.5), GeoPoint::new(0.0, -179.5));
        assert!((r.distance_m - WGS84_A * 1.0_f64.to_radians()).abs() < 1e-6);
        assert!((r.azimuth1_deg - 90.0).abs() < 1e-9);
        let d = direct(GeoPoint::new(0.0, 179.5), 90.0, r.distance_m);
        assert!((d.point.lon_deg - -179.5).abs() < 1e-9);
    }

    proptest! {
        /// direct(inverse(p1, p2)) returns to p2 within 1 mm for non-antipodal pairs.
        #[test]
        fn direct_inverts_inverse(
            lat1 in -80.0f64..80.0, lon1 in -180.0f64..180.0,
            lat2 in -80.0f64..80.0, dlon in -150.0f64..150.0,
        ) {
            let p1 = GeoPoint::new(lat1, lon1);
            let p2 = GeoPoint::new(lat2, lon1 + dlon);
            let inv = inverse(p1, p2);
            prop_assume!(inv.method == InverseMethod::Vincenty);
            let d = direct(p1, inv.azimuth1_deg, inv.distance_m);
            let back = inverse(d.point, p2);
            prop_assert!(back.distance_m < 1e-3, "miss {} m", back.distance_m);
        }

        /// Distance is symmetric.
        #[test]
        fn inverse_is_symmetric(
            lat1 in -85.0f64..85.0, lon1 in -180.0f64..180.0,
            lat2 in -85.0f64..85.0, lon2 in -180.0f64..180.0,
        ) {
            let a = inverse(GeoPoint::new(lat1, lon1), GeoPoint::new(lat2, lon2));
            let b = inverse(GeoPoint::new(lat2, lon2), GeoPoint::new(lat1, lon1));
            prop_assume!(a.method == InverseMethod::Vincenty && b.method == InverseMethod::Vincenty);
            prop_assert!((a.distance_m - b.distance_m).abs() < 1e-6);
        }
    }
}
