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
/// Time to load or unload cargo, seconds.
pub const CARGO_HANDLING_TICKS: u64 = 1_800;
/// Swept width of a unit clearing mines, metres (generic minehunter or sweep).
pub const SWEEP_WIDTH_M: f64 = 200.0;
/// A unit clearing mines meets mines at this fraction of a normal ship's rate (low signatures,
/// careful handling).
pub const MCM_RISK_FACTOR: f64 = 0.1;
/// Speed of a unit clearing mines, m/s (8 kn).
pub const MCM_SPEED_MPS: f64 = 4.0;

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
    /// Hold a station (within `radius_m` of it), engaging nothing on its own: an oiler, a
    /// picket, an ESM or relay station.
    Support { station: GeoPoint, radius_m: f64 },
    /// Go to `centre` and lay `mines` (or all it carries, if fewer) in a circle of `radius_m`.
    Mining {
        centre: GeoPoint,
        radius_m: f64,
        mines: u32,
    },
    /// Sweep the area at 8 kn: it removes mines of other sides' fields it passes through, at a
    /// tenth of a normal ship's risk. It never completes on its own: the side cannot know the
    /// area is clear.
    MineClearing { centre: GeoPoint, radius_m: f64 },
    /// Carry `tonnes` to `destination` (loading first at `origin`, if given): 30 minutes to load
    /// and to unload, then complete.
    Cargo {
        #[serde(default)]
        origin: Option<GeoPoint>,
        destination: GeoPoint,
        tonnes: f64,
    },
    /// Steam back and forth between two points across a passage.
    BarrierPatrol { start: GeoPoint, end: GeoPoint },
    /// Patrol inside the side's zones of a choke point and engage only contacts believed inside
    /// them.
    Denial { choke_point: String },
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
            Self::Support { station, radius_m }
            | Self::MineClearing {
                centre: station,
                radius_m,
            } => station.is_valid() && radius_m.is_finite() && *radius_m > 0.0,
            Self::Mining {
                centre,
                radius_m,
                mines,
            } => centre.is_valid() && radius_m.is_finite() && *radius_m > 0.0 && *mines > 0,
            Self::Cargo {
                origin,
                destination,
                tonnes,
            } => {
                origin.is_none_or(|o| o.is_valid())
                    && destination.is_valid()
                    && tonnes.is_finite()
                    && *tonnes > 0.0
            }
            Self::BarrierPatrol { start, end } => start.is_valid() && end.is_valid(),
            Self::Denial { choke_point } => !choke_point.trim().is_empty(),
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
            Self::Support { .. } => 4,
            Self::Mining { .. } => 5,
            Self::MineClearing { .. } => 6,
            Self::Cargo { .. } => 7,
            Self::BarrierPatrol { .. } => 8,
            Self::Denial { .. } => 9,
        }
    }

    /// Whether the AI engages contacts on its own on this mission (support, mining, mine
    /// clearing and cargo units only defend themselves, through point defence).
    #[must_use]
    pub fn engages(&self) -> bool {
        matches!(
            self,
            Self::Patrol { .. }
                | Self::Ferry { .. }
                | Self::Escort { .. }
                | Self::Strike { .. }
                | Self::BarrierPatrol { .. }
                | Self::Denial { .. }
        )
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

/// A point in the triangle (a, b, c) from two uniform draws: with s = √r₁,
/// p = (1 − s)·a + s·(1 − r₂)·b + s·r₂·c (uniform over the triangle's area in latitude and
/// longitude).
#[must_use]
pub fn triangle_point(a: GeoPoint, b: GeoPoint, c: GeoPoint, r1: f64, r2: f64) -> GeoPoint {
    let s = r1.max(0.0).sqrt();
    let (wa, wb, wc) = (1.0 - s, s * (1.0 - r2), s * r2);
    GeoPoint::new(
        wa * a.lat_deg + wb * b.lat_deg + wc * c.lat_deg,
        wa * a.lon_deg + wb * b.lon_deg + wc * c.lon_deg,
    )
}

/// A point in a polygon, by a fan of triangles from its first vertex, weighted by area: `u₁`
/// picks the triangle and (rescaled) gives r₁; `u₂` gives r₂. Exact for convex polygons; for a
/// concave one the point may fall outside, and the caller checks.
#[must_use]
pub fn polygon_point(points: &[GeoPoint], u1: f64, u2: f64) -> Option<GeoPoint> {
    if points.len() < 3 {
        return None;
    }
    let area = |a: GeoPoint, b: GeoPoint, c: GeoPoint| {
        ((b.lon_deg - a.lon_deg) * (c.lat_deg - a.lat_deg)
            - (c.lon_deg - a.lon_deg) * (b.lat_deg - a.lat_deg))
            .abs()
            / 2.0
    };
    let tris: Vec<(GeoPoint, GeoPoint, f64)> = points[1..]
        .windows(2)
        .map(|w| (w[0], w[1], area(points[0], w[0], w[1])))
        .collect();
    let total: f64 = tris.iter().map(|t| t.2).sum();
    if total <= 0.0 {
        return None;
    }
    let mut x = u1.clamp(0.0, 1.0) * total;
    let last = tris.len() - 1;
    for (i, (b, c, w)) in tris.iter().enumerate() {
        if *w > 0.0 && (x < *w || i == last) {
            return Some(triangle_point(points[0], *b, *c, (x / w).min(1.0), u2));
        }
        x -= w;
    }
    None
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
    fn area_points_hand_values() {
        let a = GeoPoint::new(0.0, 0.0);
        let b = GeoPoint::new(0.0, 2.0);
        let c = GeoPoint::new(2.0, 0.0);
        // r₁ = 0: the first vertex. r₁ = 1, r₂ = 0: b. r₁ = 1, r₂ = 1: c.
        assert_eq!(triangle_point(a, b, c, 0.0, 0.7), a);
        assert_eq!(triangle_point(a, b, c, 1.0, 0.0), b);
        assert_eq!(triangle_point(a, b, c, 1.0, 1.0), c);
        // r₁ = 0.25 (s = 0.5), r₂ = 0.5: 0.5·a + 0.25·b + 0.25·c = (0.5, 0.5).
        assert_eq!(triangle_point(a, b, c, 0.25, 0.5), GeoPoint::new(0.5, 0.5));
        // A 2° square: two triangles of equal area; u₁ = 0.75 lands in the second.
        let sq = [a, b, GeoPoint::new(2.0, 2.0), c];
        let p = polygon_point(&sq, 0.75, 0.5).unwrap();
        assert!(
            p.lat_deg >= p.lon_deg - 1e-12,
            "second triangle: above the diagonal"
        );
        for i in 0..20 {
            let u = f64::from(i) / 20.0;
            let p = polygon_point(&sq, u, 1.0 - u).unwrap();
            assert!((0.0..=2.0).contains(&p.lat_deg) && (0.0..=2.0).contains(&p.lon_deg));
        }
        assert_eq!(polygon_point(&sq[..2], 0.5, 0.5), None);
    }

    #[test]
    fn new_missions_validate_and_say_whether_they_engage() {
        let p = GeoPoint::new(-39.0, 146.0);
        let ok = [
            Mission::Support {
                station: p,
                radius_m: 5_000.0,
            },
            Mission::Mining {
                centre: p,
                radius_m: 2_000.0,
                mines: 10,
            },
            Mission::MineClearing {
                centre: p,
                radius_m: 3_000.0,
            },
            Mission::Cargo {
                origin: None,
                destination: p,
                tonnes: 500.0,
            },
            Mission::BarrierPatrol {
                start: p,
                end: GeoPoint::new(-39.5, 146.0),
            },
            Mission::Denial {
                choke_point: "Bass".into(),
            },
        ];
        for m in &ok {
            assert!(m.validate().is_ok(), "{m:?}");
        }
        let engages: Vec<bool> = ok.iter().map(Mission::engages).collect();
        assert_eq!(engages, [false, false, false, false, true, true]);
        let no_mines = Mission::Mining {
            centre: p,
            radius_m: 1.0,
            mines: 0,
        };
        assert!(no_mines.validate().is_err());
        let blank = Mission::Denial {
            choke_point: " ".into(),
        };
        assert!(blank.validate().is_err());
        let m: Mission = serde_json::from_str(
            r#"{"kind":"cargo","destination":{"lat_deg":-39.0,"lon_deg":146.0},"tonnes":800}"#,
        )
        .unwrap();
        assert_eq!(m.code(), 7);
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
