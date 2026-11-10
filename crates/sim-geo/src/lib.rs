//! Deterministic WGS-84 geodesy for the simulation core.
//!
//! All transcendental maths goes through the pure-Rust [`libm`] crate, so results are
//! bit-identical on Windows, macOS and Linux (ADR-0003). Model documentation:
//! `docs/models/geodesy-and-movement.md` and `docs/models/radar-horizon.md`.

#![forbid(unsafe_code)]

mod angle;
mod ellipsoid;
mod horizon;
mod vincenty;

pub use angle::{normalize_azimuth_deg, normalize_lon_deg};
pub use ellipsoid::{WGS84_A, WGS84_B, WGS84_F};
pub use horizon::{radar_horizon_m, within_radar_horizon, RADAR_HORIZON_COEFF_M};
pub use vincenty::{direct, inverse, Direct, Inverse, InverseMethod};

use serde::{Deserialize, Serialize};

/// A point on the WGS-84 ellipsoid, in decimal degrees.
///
/// Latitude is positive north and lies in −90..=90. Longitude is positive east and lies in −180..180.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct GeoPoint {
    pub lat_deg: f64,
    pub lon_deg: f64,
}

impl GeoPoint {
    /// Builds a point, normalising longitude into −180..180.
    #[must_use]
    pub fn new(lat_deg: f64, lon_deg: f64) -> Self {
        Self {
            lat_deg,
            lon_deg: normalize_lon_deg(lon_deg),
        }
    }

    /// True when the coordinates are finite and the latitude is in range.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.lat_deg.is_finite()
            && self.lon_deg.is_finite()
            && (-90.0..=90.0).contains(&self.lat_deg)
    }
}
