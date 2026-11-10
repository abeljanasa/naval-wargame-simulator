use crate::ids::{SideId, UnitId};
use bevy_ecs::prelude::Component;
use serde::{Deserialize, Serialize};
use sim_c2::Doctrine;
use sim_geo::GeoPoint;

/// Broad physical domain of a unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum Domain {
    Surface,
    Subsurface,
    Air,
    Land,
}

impl Domain {
    pub(crate) fn code(self) -> u8 {
        match self {
            Domain::Surface => 0,
            Domain::Subsurface => 1,
            Domain::Air => 2,
            Domain::Land => 3,
        }
    }
}

/// Identity of a unit.
#[derive(Debug, Clone, Component, Serialize, Deserialize)]
pub struct Unit {
    pub id: UnitId,
    pub name: String,
    pub side: SideId,
    pub domain: Domain,
    /// Platform database class, if the unit came from one.
    #[serde(default)]
    pub platform: Option<String>,
    /// Group (task group, formation) within the side, for doctrine inheritance.
    #[serde(default)]
    pub group: Option<String>,
    /// Doctrine set on this unit; unset fields inherit from the group and the side.
    #[serde(default)]
    pub doctrine: Doctrine,
}

/// Geodetic position. `altitude_m` is height above MSL; negative values are depth.
#[derive(Debug, Clone, Copy, Component, Serialize, Deserialize)]
pub struct Position {
    pub point: GeoPoint,
    pub altitude_m: f64,
}

/// Speed and heading state with simple performance limits.
#[derive(Debug, Clone, Copy, Component, Serialize, Deserialize)]
pub struct Kinematics {
    /// True heading, degrees 0..360.
    pub heading_deg: f64,
    /// Current speed over ground, m/s.
    pub speed_mps: f64,
    /// Ordered speed, m/s. The unit accelerates toward it, limited by `max_speed_mps`.
    pub ordered_speed_mps: f64,
    pub max_speed_mps: f64,
    /// Magnitude of acceleration and deceleration, m/s².
    pub accel_mps2: f64,
    /// Turn-rate cap, deg/s. `None` = heading changes instantly (Phase 1 behaviour, kept for
    /// units without turn data).
    #[serde(default)]
    pub max_turn_rate_dps: Option<f64>,
    /// Minimum turn radius, metres (0 = no radius limit). The achievable turn rate is
    /// min(cap, speed / radius).
    #[serde(default)]
    pub min_turn_radius_m: f64,
}

/// Ordered route. `next` indexes the waypoint being steered for. The route is finished when `next == waypoints.len()`.
#[derive(Debug, Clone, Default, Component, Serialize, Deserialize)]
pub struct Route {
    pub waypoints: Vec<GeoPoint>,
    pub next: usize,
}

impl Route {
    #[must_use]
    pub fn current(&self) -> Option<GeoPoint> {
        self.waypoints.get(self.next).copied()
    }

    #[must_use]
    pub fn is_active(&self) -> bool {
        self.next < self.waypoints.len()
    }
}
