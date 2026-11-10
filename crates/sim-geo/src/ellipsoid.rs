//! WGS-84 defining parameters (NIMA TR8350.2).

/// Semi-major axis, metres.
pub const WGS84_A: f64 = 6_378_137.0;
/// Flattening.
pub const WGS84_F: f64 = 1.0 / 298.257_223_563;
/// Semi-minor axis, metres (derived: a·(1 − f)).
pub const WGS84_B: f64 = WGS84_A * (1.0 - WGS84_F);

/// Earth-centred, Earth-fixed position (metres) of a point on the ellipsoid surface.
#[must_use]
pub fn ecef_surface(p: crate::GeoPoint) -> [f64; 3] {
    let e2 = WGS84_F * (2.0 - WGS84_F);
    let (lat, lon) = (p.lat_deg.to_radians(), p.lon_deg.to_radians());
    let (sl, cl) = (libm::sin(lat), libm::cos(lat));
    let n = WGS84_A / (1.0 - e2 * sl * sl).sqrt();
    [
        n * cl * libm::cos(lon),
        n * cl * libm::sin(lon),
        n * (1.0 - e2) * sl,
    ]
}

/// Straight-line (chord) distance between two surface points, metres. Never longer than the
/// geodesic between them, so `chord > d` proves `geodesic > d`: an exact, cheap pre-check.
#[must_use]
pub fn chord_m(a: [f64; 3], b: [f64; 3]) -> f64 {
    let (dx, dy, dz) = (a[0] - b[0], a[1] - b[1], a[2] - b[2]);
    (dx * dx + dy * dy + dz * dz).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{inverse, GeoPoint};

    #[test]
    fn chord_hand_value_and_bound() {
        // Two equator points 90° apart: chord = a·√2.
        let a = ecef_surface(GeoPoint::new(0.0, 0.0));
        let b = ecef_surface(GeoPoint::new(0.0, 90.0));
        assert!((chord_m(a, b) - WGS84_A * 2f64.sqrt()).abs() < 1e-6);
        // The chord never exceeds the geodesic, up to rounding (≈1e-9 m at metre scales, which
        // is why callers keep a 1 m margin).
        for (p, q) in [
            ((10.0, 20.0), (10.3, 20.4)),
            ((-39.3, 146.0), (-38.9, 148.6)),
            ((60.0, -5.0), (61.0, 5.0)),
            ((0.0, 0.0), (0.0, 0.000_1)),
        ] {
            let (p, q) = (GeoPoint::new(p.0, p.1), GeoPoint::new(q.0, q.1));
            let c = chord_m(ecef_surface(p), ecef_surface(q));
            assert!(c <= inverse(p, q).distance_m + 1e-6, "{p:?} {q:?}");
        }
    }
}
