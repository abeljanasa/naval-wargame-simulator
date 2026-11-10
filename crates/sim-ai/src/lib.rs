//! Missions and automated tasking (master prompt §8 "basic missions", §7 "Missions and AI"), as
//! plain deterministic functions with no ECS dependency (ADR-0005). `sim-core` runs them every
//! [`AI_PERIOD_TICKS`]. Model doc: `docs/models/missions.md`.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use sim_geo::{direct, GeoPoint};

/// The AI runs every 10 s (master prompt §6.2: AI every 10–30 s).
pub const AI_PERIOD_TICKS: u64 = 10;
/// A ferry or strike leg counts as reached within this distance, metres.
pub const ARRIVAL_M: f64 = 1_000.0;
/// An escort further than this from its station steams at full speed to regain it, metres.
pub const STATION_TOLERANCE_M: f64 = 500.0;

/// What a unit has been told to do.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Mission {
    /// Steam between random points in a circular area.
    Patrol { centre: GeoPoint, radius_m: f64 },
    /// Go to a point, then the mission is complete.
    Ferry { destination: GeoPoint },
    /// Keep station on another unit: `bearing_deg` relative to its heading, at `range_m`.
    Escort {
        #[cfg_attr(feature = "specta", specta(type = u32))]
        escorted: u64,
        bearing_deg: f64,
        range_m: f64,
    },
    /// Close and attack one of the side's contacts.
    Strike { contact: u32 },
}

impl Mission {
    pub fn validate(&self) -> Result<(), String> {
        let ok = match self {
            Self::Patrol { centre, radius_m } => {
                centre.is_valid() && radius_m.is_finite() && *radius_m > 0.0
            }
            Self::Ferry { destination } => destination.is_valid(),
            Self::Escort {
                bearing_deg,
                range_m,
                ..
            } => bearing_deg.is_finite() && range_m.is_finite() && *range_m >= 0.0,
            Self::Strike { .. } => true,
        };
        if ok {
            Ok(())
        } else {
            Err("mission has invalid figures".into())
        }
    }

    /// Stable code for the state hash.
    #[must_use]
    pub fn code(&self) -> u8 {
        match self {
            Self::Patrol { .. } => 0,
            Self::Ferry { .. } => 1,
            Self::Escort { .. } => 2,
            Self::Strike { .. } => 3,
        }
    }
}

/// A point uniformly distributed over a circular patrol area, from two uniform draws:
/// bearing 360° × u₁, distance radius × √u₂ (the square root makes the area density uniform).
#[must_use]
pub fn patrol_point(centre: GeoPoint, radius_m: f64, u1: f64, u2: f64) -> GeoPoint {
    let d = radius_m * u2.max(0.0).sqrt();
    if d == 0.0 {
        return centre;
    }
    direct(centre, 360.0 * u1, d).point
}

/// An escort's station: `bearing_deg` off the escorted unit's heading, at `range_m`.
#[must_use]
pub fn escort_station(
    escorted: GeoPoint,
    escorted_heading_deg: f64,
    bearing_deg: f64,
    range_m: f64,
) -> GeoPoint {
    if range_m == 0.0 {
        return escorted;
    }
    direct(escorted, escorted_heading_deg + bearing_deg, range_m).point
}

/// Speed to order an escort: full speed while off station, else the escorted unit's speed.
#[must_use]
pub fn station_speed(
    distance_to_station_m: f64,
    escorted_speed_mps: f64,
    max_speed_mps: f64,
) -> f64 {
    if distance_to_station_m > STATION_TOLERANCE_M {
        max_speed_mps
    } else {
        escorted_speed_mps.min(max_speed_mps)
    }
}

/// Candidate targets for automated engagement, nearest first: (contact number, range m).
/// Ties keep contact-number order, so the choice is deterministic.
#[must_use]
pub fn nearest_first(mut candidates: Vec<(u32, f64)>) -> Vec<u32> {
    candidates.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
    candidates.into_iter().map(|(c, _)| c).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim_geo::inverse;

    #[test]
    fn patrol_points_hand_values() {
        let c = GeoPoint::new(-39.0, 146.0);
        // u₁ = 0.25 → due east; u₂ = 0.25 → half the radius.
        let p = patrol_point(c, 20_000.0, 0.25, 0.25);
        let inv = inverse(c, p);
        assert!((inv.distance_m - 10_000.0).abs() < 1e-6);
        assert!((inv.azimuth1_deg - 90.0).abs() < 1e-9);
        assert_eq!(patrol_point(c, 20_000.0, 0.7, 0.0), c);
        // Every point lies inside the area.
        for i in 0..50 {
            let u = f64::from(i) / 50.0;
            let d = inverse(c, patrol_point(c, 20_000.0, u, 1.0 - u)).distance_m;
            assert!(d <= 20_000.0 + 1e-6);
        }
    }

    #[test]
    fn escort_station_and_speed() {
        let e = GeoPoint::new(0.0, 0.0);
        // Escorted ship heading 090°, station 45° off the bow at 2 km: bearing 135°.
        let s = escort_station(e, 90.0, 45.0, 2_000.0);
        let inv = inverse(e, s);
        assert!((inv.distance_m - 2_000.0).abs() < 1e-6);
        assert!((inv.azimuth1_deg - 135.0).abs() < 1e-9);
        assert_eq!(station_speed(3_000.0, 8.0, 15.0), 15.0);
        assert_eq!(station_speed(100.0, 8.0, 15.0), 8.0);
    }

    #[test]
    fn nearest_target_first_with_stable_ties() {
        assert_eq!(
            nearest_first(vec![(3, 50_000.0), (1, 20_000.0), (2, 20_000.0)]),
            vec![1, 2, 3]
        );
    }

    #[test]
    fn mission_json_and_validation() {
        let m: Mission = serde_json::from_str(
            r#"{"kind":"patrol","centre":{"lat_deg":-39.0,"lon_deg":146.0},"radius_m":30000}"#,
        )
        .unwrap();
        assert!(m.validate().is_ok());
        let bad = Mission::Patrol {
            centre: GeoPoint::new(0.0, 0.0),
            radius_m: -1.0,
        };
        assert!(bad.validate().is_err());
    }
}
