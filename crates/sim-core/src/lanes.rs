//! Shipping lanes, choke-point depths and transit risk (Phase 4 slice 5,
//! `docs/models/choke-points.md`).
//!
//! A **lane** is a polyline with a width that traffic follows: drawn by the planner, or imported
//! from a traffic data set. Its traffic figure (ships per day) is accepted only with a named
//! source, so no unlicensed traffic data gets in. A lane belongs to a choke point by name, or
//! by passing through the choke point's zones.
//!
//! When another side's unit enters a choke point, its transit is assessed: the traffic of the
//! choke point's lanes, the lane it is in, and its under-keel margin there (terrain packs). The
//! result is a `TransitAssessed` event and a row of the AAR's transit table. Display and
//! analysis only: nothing here changes the run.

use crate::depths::DepthSample;
use crate::planning::PlanningLayer;
use serde::{Deserialize, Serialize};
use sim_events::Area;
use sim_geo::terrain::Terrain;
use sim_geo::GeoPoint;
use std::collections::BTreeSet;

/// Lane width when none is given, metres.
pub const DEFAULT_LANE_WIDTH_M: f64 = 2_000.0;
/// Traffic at or above which a transit's risk is moderate, ships per day.
pub const TRAFFIC_MODERATE_PER_DAY: f64 = 20.0;
/// Traffic at or above which a transit's risk is high, ships per day.
pub const TRAFFIC_HIGH_PER_DAY: f64 = 100.0;
/// Under-keel margin below which a transit's risk is moderate, metres.
pub const MARGIN_MODERATE_M: f64 = 5.0;
/// Under-keel margin below which a transit's risk is high, metres.
pub const MARGIN_HIGH_M: f64 = 2.0;
/// Samples across a choke point's depth section.
pub const SECTION_SAMPLES: usize = 120;
/// Cells per side of the grid a choke point's depth statistics are taken on.
const STATS_GRID: u32 = 24;

const METRES_PER_DEG: f64 = 111_320.0;

fn default_width() -> f64 {
    DEFAULT_LANE_WIDTH_M
}

/// A shipping lane.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Lane {
    pub id: u32,
    pub name: String,
    /// The lane's centreline, at least two points.
    pub points: Vec<GeoPoint>,
    #[serde(default = "default_width")]
    pub width_m: f64,
    /// Traffic using the lane, ships per day. Only with a `source`.
    #[serde(default)]
    pub ships_per_day: Option<f64>,
    /// Where the lane and its traffic figure come from: a licensed traffic data set, a chart,
    /// or "planner's estimate".
    #[serde(default)]
    pub source: Option<String>,
    /// The choke point the lane runs through (as zones name it).
    #[serde(default)]
    pub choke_point: Option<String>,
}

/// Checks lanes: unique ids, a name, at least two valid points, a width, and a traffic figure
/// only with a named source.
pub fn validate_lanes(lanes: &[Lane]) -> Result<(), String> {
    let mut ids = BTreeSet::new();
    for l in lanes {
        if !ids.insert(l.id) {
            return Err(format!("duplicate lane id {}", l.id));
        }
        if l.name.trim().is_empty() {
            return Err(format!("lane {} has no name", l.id));
        }
        if l.points.len() < 2 || !l.points.iter().all(GeoPoint::is_valid) {
            return Err(format!("lane {:?} needs at least two valid points", l.name));
        }
        if !(l.width_m.is_finite() && l.width_m > 0.0 && l.width_m <= 100_000.0) {
            return Err(format!(
                "lane {:?} needs a width over 0 and up to 100 km",
                l.name
            ));
        }
        if let Some(t) = l.ships_per_day {
            if !(t.is_finite() && t >= 0.0) {
                return Err(format!("lane {:?} has traffic that is not ≥ 0", l.name));
            }
            if l.source.as_deref().is_none_or(|s| s.trim().is_empty()) {
                return Err(format!(
                    "lane {:?} gives traffic without a source: name the data set it comes from \
                     (or \"planner's estimate\")",
                    l.name
                ));
            }
        }
    }
    Ok(())
}

/// A transit's risk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum TransitRisk {
    Low,
    Moderate,
    High,
}

/// The risk of a transit through `traffic` ships per day with `margin_m` of water under the
/// keel (`None`: not known): the higher of the two ratings.
#[must_use]
pub fn transit_risk(traffic_per_day: f64, margin_m: Option<f64>) -> TransitRisk {
    let traffic = if traffic_per_day >= TRAFFIC_HIGH_PER_DAY {
        TransitRisk::High
    } else if traffic_per_day >= TRAFFIC_MODERATE_PER_DAY {
        TransitRisk::Moderate
    } else {
        TransitRisk::Low
    };
    let depth = match margin_m {
        Some(m) if m < MARGIN_HIGH_M => TransitRisk::High,
        Some(m) if m < MARGIN_MODERATE_M => TransitRisk::Moderate,
        _ => TransitRisk::Low,
    };
    traffic.max(depth)
}

/// Local east/north offsets of `p` from `origin`, metres (equirectangular; fine over a lane's
/// width).
fn local(origin: GeoPoint, p: GeoPoint) -> (f64, f64) {
    let mut dlon = p.lon_deg - origin.lon_deg;
    if dlon > 180.0 {
        dlon -= 360.0;
    } else if dlon < -180.0 {
        dlon += 360.0;
    }
    let k = libm::cos(origin.lat_deg.to_radians());
    (
        dlon * METRES_PER_DEG * k,
        (p.lat_deg - origin.lat_deg) * METRES_PER_DEG,
    )
}

/// Distance from `p` to the lane's centreline, metres.
#[must_use]
pub fn distance_to_lane_m(lane: &Lane, p: GeoPoint) -> f64 {
    lane.points
        .windows(2)
        .map(|w| {
            let (ax, ay) = local(p, w[0]);
            let (bx, by) = local(p, w[1]);
            let (dx, dy) = (bx - ax, by - ay);
            let len2 = dx * dx + dy * dy;
            let t = if len2 > 0.0 {
                (-(ax * dx + ay * dy) / len2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let (x, y) = (ax + t * dx, ay + t * dy);
            libm::sqrt(x * x + y * y)
        })
        .fold(f64::INFINITY, f64::min)
}

/// Whether the lane runs through any of `areas` (its centreline, sampled).
fn passes_through(lane: &Lane, areas: &[Area]) -> bool {
    lane.points.windows(2).any(|w| {
        (0..=20).any(|i| {
            let t = f64::from(i) / 20.0;
            let p = GeoPoint::new(
                w[0].lat_deg + t * (w[1].lat_deg - w[0].lat_deg),
                w[0].lon_deg + t * (w[1].lon_deg - w[0].lon_deg),
            );
            areas.iter().any(|a| a.contains(p))
        })
    })
}

/// The lanes of a choke point: named for it, or running through its zones.
pub(crate) fn choke_lanes<'a>(
    layer: &'a PlanningLayer,
    name: &str,
    areas: &[Area],
) -> Vec<&'a Lane> {
    layer
        .lanes
        .iter()
        .filter(|l| l.choke_point.as_deref() == Some(name) || passes_through(l, areas))
        .collect()
}

/// The areas of every zone of choke point `name`.
pub(crate) fn choke_areas(layer: &PlanningLayer, name: &str) -> Vec<Area> {
    layer
        .zones
        .iter()
        .filter(|z| z.choke_point.as_deref() == Some(name))
        .map(|z| z.area.clone())
        .collect()
}

/// A transit's assessment, when there is anything to assess (lanes here, or terrain packs).
pub(crate) struct Assessment {
    pub lane: Option<String>,
    pub traffic_per_day: f64,
    pub depth_m: Option<f64>,
    pub risk: TransitRisk,
}

pub(crate) fn assess(
    layer: &PlanningLayer,
    terrain: Option<&Terrain>,
    choke: &str,
    areas: &[Area],
    at: GeoPoint,
    draft_m: Option<f64>,
) -> Option<Assessment> {
    let lanes = choke_lanes(layer, choke, areas);
    if lanes.is_empty() && terrain.is_none() {
        return None;
    }
    let traffic_per_day = lanes.iter().filter_map(|l| l.ships_per_day).sum();
    let lane = lanes
        .iter()
        .find(|l| distance_to_lane_m(l, at) <= l.width_m / 2.0)
        .map(|l| l.name.clone());
    let depth_m = terrain.and_then(|t| t.depth_m(at));
    let margin = depth_m.zip(draft_m).map(|(d, k)| d - k);
    Some(Assessment {
        lane,
        traffic_per_day,
        depth_m,
        risk: transit_risk(traffic_per_day, margin),
    })
}

/// A choke point's water: depth statistics over its zones, a depth section across it, and its
/// lanes' traffic.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ChokeDepths {
    pub choke_point: String,
    /// Over a 24 × 24 grid of the zones' area (`None` without terrain packs there).
    pub least_m: Option<f64>,
    pub median_m: Option<f64>,
    pub deepest_m: Option<f64>,
    /// Across the choke point: square to its lane where it has one, else across its narrower
    /// way. Samples outside its zones have no depth.
    pub section_from: GeoPoint,
    pub section_to: GeoPoint,
    pub section: Vec<DepthSample>,
    pub lanes: Vec<u32>,
    pub traffic_per_day: f64,
}

/// The choke point's water (see [`ChokeDepths`]). `None` if no zone names it.
pub(crate) fn choke_depths(
    layer: &PlanningLayer,
    terrain: Option<&Terrain>,
    name: &str,
) -> Option<ChokeDepths> {
    let areas = choke_areas(layer, name);
    if areas.is_empty() {
        return None;
    }
    let (mut s, mut w, mut n, mut e) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for a in &areas {
        let (as_, aw, an, ae) = area_bounds(a);
        s = s.min(as_);
        w = w.min(aw);
        n = n.max(an);
        e = e.max(ae);
    }
    let inside = |p: GeoPoint| areas.iter().any(|a| a.contains(p));
    let depth = |p: GeoPoint| {
        if inside(p) {
            terrain.and_then(|t| t.depth_m(p))
        } else {
            None
        }
    };
    // Statistics.
    let mut depths: Vec<f64> = Vec::new();
    for i in 0..STATS_GRID {
        for j in 0..STATS_GRID {
            let p = GeoPoint::new(
                s + (f64::from(i) + 0.5) / f64::from(STATS_GRID) * (n - s),
                w + (f64::from(j) + 0.5) / f64::from(STATS_GRID) * (e - w),
            );
            if let Some(d) = depth(p) {
                depths.push(d);
            }
        }
    }
    depths.sort_by(f64::total_cmp);
    let lanes = choke_lanes(layer, name, &areas);
    // The section: through the box's centre, square to the lane, else across the narrow way.
    let centre = GeoPoint::new((s + n) / 2.0, (w + e) / 2.0);
    let (half_x, half_y) = local(centre, GeoPoint::new(n, e));
    let half = libm::sqrt(half_x * half_x + half_y * half_y);
    let across = match lanes
        .first()
        .and_then(|l| Some((l.points.first()?, l.points.last()?)))
    {
        Some((a, b)) => sim_geo::inverse(*a, *b).azimuth1_deg + 90.0,
        None if half_y > half_x => 90.0,
        None => 0.0,
    };
    let from = sim_geo::direct(centre, across + 180.0, half).point;
    let to = sim_geo::direct(centre, across, half).point;
    let section: Vec<DepthSample> = (0..SECTION_SAMPLES)
        .map(|i| {
            let t = i as f64 / (SECTION_SAMPLES - 1) as f64;
            let p = GeoPoint::new(
                from.lat_deg + t * (to.lat_deg - from.lat_deg),
                from.lon_deg + t * (to.lon_deg - from.lon_deg),
            );
            DepthSample {
                distance_m: t * 2.0 * half,
                depth_m: depth(p),
            }
        })
        .collect();
    Some(ChokeDepths {
        choke_point: name.to_owned(),
        least_m: depths.first().copied(),
        median_m: depths.get(depths.len() / 2).copied(),
        deepest_m: depths.last().copied(),
        section_from: from,
        section_to: to,
        section,
        lanes: lanes.iter().map(|l| l.id).collect(),
        traffic_per_day: lanes.iter().filter_map(|l| l.ships_per_day).sum(),
    })
}

fn area_bounds(a: &Area) -> (f64, f64, f64, f64) {
    match a {
        Area::Circle { centre, radius_m } => {
            let dlat = radius_m / METRES_PER_DEG;
            let dlon =
                radius_m / (METRES_PER_DEG * libm::cos(centre.lat_deg.to_radians()).max(0.01));
            (
                centre.lat_deg - dlat,
                centre.lon_deg - dlon,
                centre.lat_deg + dlat,
                centre.lon_deg + dlon,
            )
        }
        Area::Polygon { points } => points.iter().fold(
            (f64::MAX, f64::MAX, f64::MIN, f64::MIN),
            |(s, w, n, e), p| {
                (
                    s.min(p.lat_deg),
                    w.min(p.lon_deg),
                    n.max(p.lat_deg),
                    e.max(p.lon_deg),
                )
            },
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lane(points: &[(f64, f64)], traffic: Option<f64>, source: Option<&str>) -> Lane {
        Lane {
            id: 1,
            name: "Main channel".into(),
            points: points.iter().map(|&(a, b)| GeoPoint::new(a, b)).collect(),
            width_m: 4_000.0,
            ships_per_day: traffic,
            source: source.map(str::to_owned),
            choke_point: None,
        }
    }

    #[test]
    fn traffic_needs_a_source() {
        assert!(validate_lanes(&[lane(&[(0.0, 0.0), (1.0, 0.0)], None, None)]).is_ok());
        let err = validate_lanes(&[lane(&[(0.0, 0.0), (1.0, 0.0)], Some(50.0), None)]);
        assert!(err.unwrap_err().contains("without a source"));
        assert!(validate_lanes(&[lane(
            &[(0.0, 0.0), (1.0, 0.0)],
            Some(50.0),
            Some("planner's estimate")
        )])
        .is_ok());
        assert!(validate_lanes(&[lane(&[(0.0, 0.0)], None, None)]).is_err());
    }

    #[test]
    fn risk_is_the_worse_of_traffic_and_depth() {
        assert_eq!(transit_risk(5.0, None), TransitRisk::Low);
        assert_eq!(transit_risk(20.0, Some(30.0)), TransitRisk::Moderate);
        assert_eq!(transit_risk(150.0, Some(30.0)), TransitRisk::High);
        assert_eq!(transit_risk(5.0, Some(4.0)), TransitRisk::Moderate);
        assert_eq!(transit_risk(5.0, Some(1.5)), TransitRisk::High);
    }

    #[test]
    fn distance_to_a_lane() {
        let l = lane(&[(0.0, 0.0), (0.0, 1.0)], None, None);
        // 0.01° north of the centreline: about 1.1 km.
        let d = distance_to_lane_m(&l, GeoPoint::new(0.01, 0.5));
        assert!((d - 1_113.2).abs() < 1.0, "{d}");
        // Past the end: the distance to the end point.
        let d = distance_to_lane_m(&l, GeoPoint::new(0.0, 1.01));
        assert!((d - 1_113.2).abs() < 1.0, "{d}");
        assert!(passes_through(
            &l,
            &[Area::Circle {
                centre: GeoPoint::new(0.0, 0.5),
                radius_m: 500.0
            }]
        ));
    }
}
