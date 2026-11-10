//! WGS-84 defining parameters (NIMA TR8350.2).

/// Semi-major axis, metres.
pub const WGS84_A: f64 = 6_378_137.0;
/// Flattening.
pub const WGS84_F: f64 = 1.0 / 298.257_223_563;
/// Semi-minor axis, metres (derived: a·(1 − f)).
pub const WGS84_B: f64 = WGS84_A * (1.0 - WGS84_F);
