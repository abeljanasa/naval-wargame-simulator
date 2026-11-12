//! Depth along a unit's route, for the planner (`docs/models/shallow-water.md`).
//!
//! Display only: nothing here changes the run.

use crate::components::{Position, Route, Unit};
use serde::{Deserialize, Serialize};
use sim_geo::terrain::Terrain;
use sim_geo::GeoPoint;

/// Most samples along a route.
const MAX_SAMPLES: usize = 200;
/// Spacing of samples along a route, metres (wider on long routes, to stay within
/// [`MAX_SAMPLES`]).
const SAMPLE_SPACING_M: f64 = 250.0;

/// Water depth at one point along a route.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct DepthSample {
    /// Along the route from the unit, metres.
    pub distance_m: f64,
    /// Water depth, metres (0 on land); `None` where no terrain pack covers.
    pub depth_m: Option<f64>,
}

/// The water under a unit and along the rest of its route.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct RouteDepths {
    pub draft_m: Option<f64>,
    /// Depth where the unit is now.
    pub here_m: Option<f64>,
    /// The rest of the route, from the unit (empty without a route).
    pub profile: Vec<DepthSample>,
    /// The shallowest sample on the rest of the route, and how far ahead it is.
    pub least_m: Option<f64>,
    pub least_at_m: Option<f64>,
    /// The route was planned deep enough for the draft (the unit is on the navigable graph).
    pub charted: bool,
}

pub(crate) fn route_depths(
    terrain: &Terrain,
    unit: &Unit,
    pos: &Position,
    route: &Route,
) -> RouteDepths {
    let mut points: Vec<GeoPoint> = vec![pos.point];
    points.extend(route.waypoints.iter().skip(route.next).copied());
    let legs: Vec<(GeoPoint, f64, f64)> = points
        .windows(2)
        .map(|w| {
            let inv = sim_geo::inverse(w[0], w[1]);
            (w[0], inv.azimuth1_deg, inv.distance_m)
        })
        .collect();
    let total: f64 = legs.iter().map(|l| l.2).sum();
    let mut profile = Vec::new();
    if total > 0.0 {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let n = ((total / SAMPLE_SPACING_M) as usize).clamp(2, MAX_SAMPLES);
        let step = total / (n - 1) as f64;
        let (mut leg, mut leg_start) = (0usize, 0.0f64);
        for i in 0..n {
            let d = (i as f64 * step).min(total);
            while leg + 1 < legs.len() && d > leg_start + legs[leg].2 {
                leg_start += legs[leg].2;
                leg += 1;
            }
            let (from, bearing, len) = legs[leg];
            let p = sim_geo::direct(from, bearing, (d - leg_start).clamp(0.0, len)).point;
            profile.push(DepthSample {
                distance_m: d,
                depth_m: terrain.depth_m(p),
            });
        }
    }
    let least = profile
        .iter()
        .filter_map(|s| s.depth_m.map(|d| (d, s.distance_m)))
        .min_by(|a, b| a.0.total_cmp(&b.0));
    RouteDepths {
        draft_m: unit.draft_m,
        here_m: terrain.depth_m(pos.point),
        profile,
        least_m: least.map(|l| l.0),
        least_at_m: least.map(|l| l.1),
        charted: route.charted && route.is_active(),
    }
}
