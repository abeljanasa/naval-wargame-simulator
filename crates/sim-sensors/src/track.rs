//! Track position estimate: a point and its error covariance, fused across detections and grown
//! with time since the last update (`docs/models/tracks.md`).
//!
//! Covariances live in a local east/north tangent plane, in m². Positions move between the plane
//! and the ellipsoid through the Vincenty solutions in `sim-geo`, so no map projection is involved.

use serde::{Deserialize, Serialize};
use sim_geo::{direct, inverse, GeoPoint};

/// A 2-D position covariance in a local east/north plane, m².
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Cov2 {
    pub ee: f64,
    pub en: f64,
    pub nn: f64,
}

type M2 = [[f64; 2]; 2];

fn mul(a: M2, b: M2) -> M2 {
    [
        [
            a[0][0] * b[0][0] + a[0][1] * b[1][0],
            a[0][0] * b[0][1] + a[0][1] * b[1][1],
        ],
        [
            a[1][0] * b[0][0] + a[1][1] * b[1][0],
            a[1][0] * b[0][1] + a[1][1] * b[1][1],
        ],
    ]
}

impl Cov2 {
    /// Isotropic covariance with standard deviation `sigma_m` on each axis.
    #[must_use]
    pub fn isotropic(sigma_m: f64) -> Self {
        let v = sigma_m * sigma_m;
        Self {
            ee: v,
            en: 0.0,
            nn: v,
        }
    }

    /// Covariance of a range/bearing measurement: `along_sigma_m` along the line of sight at
    /// `bearing_deg` (clockwise from north) and `cross_sigma_m` across it.
    #[must_use]
    pub fn from_polar(along_sigma_m: f64, cross_sigma_m: f64, bearing_deg: f64) -> Self {
        let b = bearing_deg.to_radians();
        let (s, c) = (libm::sin(b), libm::cos(b));
        let a2 = along_sigma_m * along_sigma_m;
        let c2 = cross_sigma_m * cross_sigma_m;
        Self {
            ee: a2 * s * s + c2 * c * c,
            en: (a2 - c2) * s * c,
            nn: a2 * c * c + c2 * s * s,
        }
    }

    /// Adds an isotropic spread of `sigma_m` per axis (error growth between updates).
    #[must_use]
    pub fn grown(self, sigma_m: f64) -> Self {
        let v = sigma_m * sigma_m;
        Self {
            ee: self.ee + v,
            en: self.en,
            nn: self.nn + v,
        }
    }

    fn m(self) -> M2 {
        [[self.ee, self.en], [self.en, self.nn]]
    }

    /// One-sigma error ellipse: the square roots of the eigenvalues, and the major axis direction.
    #[must_use]
    pub fn ellipse(self) -> ErrorEllipse {
        let mean = 0.5 * (self.ee + self.nn);
        let half_diff = 0.5 * (self.ee - self.nn);
        let rad = (half_diff * half_diff + self.en * self.en).sqrt();
        let major = (mean + rad).max(0.0).sqrt();
        let minor = (mean - rad).max(0.0).sqrt();
        // Major-axis angle from the east axis, counter-clockwise; then as a bearing from north.
        let from_east = 0.5 * libm::atan2(2.0 * self.en, self.ee - self.nn).to_degrees();
        let mut orientation = 90.0 - from_east;
        if orientation >= 180.0 {
            orientation -= 180.0;
        }
        if orientation < 0.0 {
            orientation += 180.0;
        }
        ErrorEllipse {
            semi_major_m: major,
            semi_minor_m: minor,
            orientation_deg: orientation,
        }
    }
}

/// A one-sigma position error ellipse, as shown to players.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ErrorEllipse {
    pub semi_major_m: f64,
    pub semi_minor_m: f64,
    /// Bearing of the major axis, degrees clockwise from north, in [0, 180).
    pub orientation_deg: f64,
}

/// A point in the local east/north plane centred on `origin`, metres.
fn to_local(origin: GeoPoint, p: GeoPoint) -> [f64; 2] {
    let inv = inverse(origin, p);
    if inv.distance_m == 0.0 {
        return [0.0, 0.0];
    }
    let az = inv.azimuth1_deg.to_radians();
    [
        inv.distance_m * libm::sin(az),
        inv.distance_m * libm::cos(az),
    ]
}

fn from_local(origin: GeoPoint, v: [f64; 2]) -> GeoPoint {
    let d = libm::hypot(v[0], v[1]);
    if d == 0.0 {
        return origin;
    }
    direct(origin, libm::atan2(v[0], v[1]).to_degrees(), d).point
}

/// A track's position estimate.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TrackEstimate {
    pub position: GeoPoint,
    pub cov: Cov2,
}

impl TrackEstimate {
    /// The estimate `dt_s` seconds after its last update, for a target that may move at up to
    /// `speed_bound_mps` in any direction: the reach v·Δt is added as a one-sigma isotropic spread.
    /// The position itself does not move (no velocity estimate yet; see the model doc).
    #[must_use]
    pub fn predicted(&self, speed_bound_mps: f64, dt_s: f64) -> Self {
        Self {
            position: self.position,
            cov: self.cov.grown(speed_bound_mps.max(0.0) * dt_s.max(0.0)),
        }
    }

    /// Kalman update with an identity measurement matrix: fuses a position measurement `z` with
    /// covariance `r` into this (already predicted) estimate.
    ///
    /// K = P (P + R)⁻¹, x' = x + K (z − x), P' = (I − K) P.
    #[must_use]
    pub fn fused(&self, z: GeoPoint, r: Cov2) -> Self {
        let p = self.cov.m();
        let s = Cov2 {
            ee: self.cov.ee + r.ee,
            en: self.cov.en + r.en,
            nn: self.cov.nn + r.nn,
        };
        let det = s.ee * s.nn - s.en * s.en;
        if det.is_nan() || det <= 0.0 {
            // A degenerate prior (zero covariance) cannot be corrected; keep it.
            return *self;
        }
        let s_inv = [[s.nn / det, -s.en / det], [-s.en / det, s.ee / det]];
        let k = mul(p, s_inv);
        // The prior is the origin of the local plane, so the innovation is z itself.
        let dz = to_local(self.position, z);
        let dx = [
            k[0][0] * dz[0] + k[0][1] * dz[1],
            k[1][0] * dz[0] + k[1][1] * dz[1],
        ];
        let i_k = [[1.0 - k[0][0], -k[0][1]], [-k[1][0], 1.0 - k[1][1]]];
        let pn = mul(i_k, p);
        Self {
            position: from_local(self.position, dx),
            cov: Cov2 {
                ee: pn[0][0],
                en: 0.5 * (pn[0][1] + pn[1][0]),
                nn: pn[1][1],
            },
        }
    }
}

/// The position a range/bearing sensor reports, and its covariance.
///
/// The true range and azimuth from `observer` are perturbed by `n_range`·σ_r and
/// `n_bearing`·σ_θ, where the n are standard normal draws. Returns (point, covariance, range
/// error m, bearing error deg).
#[must_use]
pub fn measured_point(
    observer: GeoPoint,
    true_range_m: f64,
    true_azimuth_deg: f64,
    range_sigma_m: f64,
    bearing_sigma_deg: f64,
    n_range: f64,
    n_bearing: f64,
) -> (GeoPoint, Cov2, f64, f64) {
    let range_err = n_range * range_sigma_m;
    let bearing_err = n_bearing * bearing_sigma_deg;
    let range = (true_range_m + range_err).max(0.0);
    let point = direct(observer, true_azimuth_deg + bearing_err, range).point;
    let cross = true_range_m * bearing_sigma_deg.to_radians();
    let cov = Cov2::from_polar(range_sigma_m, cross, true_azimuth_deg);
    (point, cov, range_err, bearing_err)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn polar_covariance_hand_values() {
        // Due north: along-track error is north–south.
        let c = Cov2::from_polar(300.0, 100.0, 0.0);
        assert!(
            close(c.nn, 90_000.0, 1e-6) && close(c.ee, 10_000.0, 1e-6) && close(c.en, 0.0, 1e-9)
        );
        // Due east: swapped.
        let c = Cov2::from_polar(300.0, 100.0, 90.0);
        assert!(close(c.ee, 90_000.0, 1e-6) && close(c.nn, 10_000.0, 1e-6));
        // 45°: ee = nn = (a² + c²)/2 = 50 000, en = (a² − c²)/2 = 40 000.
        let c = Cov2::from_polar(300.0, 100.0, 45.0);
        assert!(close(c.ee, 50_000.0, 1e-6) && close(c.nn, 50_000.0, 1e-6));
        assert!(close(c.en, 40_000.0, 1e-6));
    }

    #[test]
    fn ellipse_recovers_axes_and_orientation() {
        for bearing in [0.0, 30.0, 90.0, 135.0, 210.0] {
            let e = Cov2::from_polar(300.0, 100.0, bearing).ellipse();
            assert!(close(e.semi_major_m, 300.0, 1e-6), "{bearing}: {e:?}");
            assert!(close(e.semi_minor_m, 100.0, 1e-6), "{bearing}: {e:?}");
            let expected = if bearing >= 180.0 {
                bearing - 180.0
            } else {
                bearing
            };
            assert!(close(e.orientation_deg, expected, 1e-6), "{bearing}: {e:?}");
        }
        let e = Cov2::isotropic(50.0).ellipse();
        assert!(close(e.semi_major_m, 50.0, 1e-9) && close(e.semi_minor_m, 50.0, 1e-9));
    }

    #[test]
    fn growth_adds_reach_squared() {
        // 100 m² grown by 10 m/s for 5 s: + 50² = 2 600 m².
        let t = TrackEstimate {
            position: GeoPoint::new(-39.0, 146.0),
            cov: Cov2::isotropic(10.0),
        };
        let g = t.predicted(10.0, 5.0);
        assert!(close(g.cov.ee, 2_600.0, 1e-9) && close(g.cov.nn, 2_600.0, 1e-9));
        assert_eq!(g.position, t.position);
    }

    #[test]
    fn fusion_hand_values() {
        // Prior variance 4 m² (σ 2 m), measurement variance 1 m², 10 m east of the prior:
        // K = 4/5, so the estimate moves 8 m east and the variance falls to 0.8 m².
        let origin = GeoPoint::new(-39.0, 146.0);
        let prior = TrackEstimate {
            position: origin,
            cov: Cov2::isotropic(2.0),
        };
        let z = direct(origin, 90.0, 10.0).point;
        let post = prior.fused(z, Cov2::isotropic(1.0));
        let moved = inverse(origin, post.position);
        assert!(close(moved.distance_m, 8.0, 1e-6), "{moved:?}");
        assert!(close(moved.azimuth1_deg, 90.0, 1e-6));
        assert!(close(post.cov.ee, 0.8, 1e-12) && close(post.cov.nn, 0.8, 1e-12));
        assert!(close(post.cov.en, 0.0, 1e-12));
    }

    #[test]
    fn fusion_trusts_the_sharper_axis() {
        // Two crossing radar fixes: the fused ellipse is smaller than either along both axes.
        let origin = GeoPoint::new(-39.0, 146.0);
        let a = TrackEstimate {
            position: origin,
            cov: Cov2::from_polar(50.0, 500.0, 0.0),
        };
        let post = a.fused(origin, Cov2::from_polar(50.0, 500.0, 90.0));
        let e = post.cov.ellipse();
        assert!(e.semi_major_m < 50.0 && e.semi_minor_m < 50.0, "{e:?}");
    }

    #[test]
    fn measurement_lands_at_the_perturbed_range_and_bearing() {
        let obs = GeoPoint::new(-39.0, 146.0);
        let (p, cov, dr, db) = measured_point(obs, 20_000.0, 45.0, 50.0, 0.5, 1.0, -2.0);
        assert!(close(dr, 50.0, 1e-12) && close(db, -1.0, 1e-12));
        let inv = inverse(obs, p);
        assert!(close(inv.distance_m, 20_050.0, 1e-6));
        assert!(close(inv.azimuth1_deg, 44.0, 1e-9));
        // Cross-range σ = 20 km × 0.5° in radians = 174.5 m.
        let e = cov.ellipse();
        assert!(close(e.semi_major_m, 20_000.0 * 0.5_f64.to_radians(), 1e-6));
        assert!(close(e.semi_minor_m, 50.0, 1e-6));
    }
}
