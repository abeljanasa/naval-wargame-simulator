//! Sense-zone coverage and gaps (Phase 3 slice 14; `docs/models/planning-layer.md` §Coverage).
//!
//! A query, not a system: it changes nothing, draws nothing and is not hashed. The zone's area is
//! sampled on a grid of up to [`GRID`] × [`GRID`] cells over its bounding box; a cell whose centre
//! is inside the area is **covered** when some unit of the zone's side has a working sensor of one
//! of the zone's sensor types (all types when the zone lists none) whose **nominal range** reaches
//! it. Nominal ranges are against reference targets (below), so the picture shows where the side
//! could expect to see a typical contact, not a guarantee.
//!
//! | Sensor | Nominal range (each capped at the sensor's maximum range) |
//! |---|---|
//! | Radar (EMCON active) | R₅₀ against a 1 000 m² ship with sea clutter and weather loss, within the radar horizon to a 10 m target |
//! | ESM | the radar horizon to a 15 m emitter |
//! | Visual, EO, IR | optical R₅₀ against a 100 m ship in the light at the sensor, within visibility and the horizon to a 10 m target |
//! | Hull, towed or dipping sonar (usable) | passive R₅₀ (signal excess 0) against a 115 dB submarine at 5 kn |
//! | MAD | MAD R₅₀ |
//! | Sonobuoys | none until laid |

use crate::c2::C2State;
use crate::components::{Domain, Kinematics, Position, Unit};
use crate::detection::{
    clutter_background, sensor_height_m, EnvState, SensorKind, SensorSpec, SensorSuite,
};
use crate::planning::{PlanningLayer, Zone, ZoneKind};
use crate::sim::UnitIndex;
use crate::time::SimClock;
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use sim_c2::Emcon;
use sim_events::Area;
use sim_geo::{inverse, radar_horizon_m, GeoPoint};
use sim_sensors::{
    ambient_noise_db, clutter_loss_db, detection_range_with_loss_m, light, mad_r50_m,
    noise_at_speed_db, optical_r50_m, power_sum_db, sonar_params, sun_elevation_deg,
    transmission_loss_db, Environment, DEFAULT_CLUTTER_DB_PER_SEA_STATE,
};

/// Cells per side of the sampling grid.
pub const GRID: usize = 24;
/// Reference ship for radar coverage: RCS, m².
pub const REFERENCE_SHIP_RCS_M2: f64 = 1_000.0;
/// Reference ship's height for horizon checks, m.
pub const REFERENCE_SHIP_HEIGHT_M: f64 = 10.0;
/// Reference emitter height for ESM coverage, m.
pub const REFERENCE_EMITTER_HEIGHT_M: f64 = 15.0;
/// Reference submarine for sonar coverage: radiated noise at 10 kn, dB.
pub const REFERENCE_SUBMARINE_NOISE_DB: f64 = 115.0;
/// Reference submarine's speed, m/s (5 kn).
pub const REFERENCE_SUBMARINE_SPEED_MPS: f64 = 2.572;

/// One sampled cell.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct CoverageCell {
    pub centre: GeoPoint,
    pub covered: bool,
}

/// A sense zone's coverage by its side's sensors, now.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ZoneCoverage {
    pub zone: u32,
    /// The sensor types counted (the zone's list, or every type).
    pub sensors: Vec<SensorKind>,
    /// Cells inside the zone, row by row from the south-west.
    pub cells: Vec<CoverageCell>,
    /// Half a cell's height and width, degrees (to draw the cells).
    pub half_lat_deg: f64,
    pub half_lon_deg: f64,
    /// Covered cells over cells inside the zone (0 when it has none).
    pub fraction: f64,
}

/// Every sensor type, for zones that list none.
const ALL_KINDS: [SensorKind; 10] = [
    SensorKind::Radar,
    SensorKind::SonarHull,
    SensorKind::SonarTowed,
    SensorKind::SonarDipping,
    SensorKind::Sonobuoy,
    SensorKind::Esm,
    SensorKind::Ir,
    SensorKind::Eo,
    SensorKind::Visual,
    SensorKind::Mad,
];

/// A sensor's position and nominal range.
struct Reach {
    at: GeoPoint,
    range_m: f64,
    /// Sensor height above sea level and the reference target's height, for terrain masking;
    /// `None` for sensors terrain does not mask (sonar, MAD).
    heights: Option<(f64, f64)>,
}

/// Passive sonar R₅₀ (signal excess 0) against the reference submarine, by bisection on range:
/// transmission loss grows with range, so SE falls monotonically.
fn sonar_reach_m(s: &SensorSpec, own_speed_mps: f64, env: Environment) -> f64 {
    let p = sonar_params(s.kind, s.active);
    let nl = power_sum_db(
        ambient_noise_db(env.sea_state),
        noise_at_speed_db(p.self_noise_db, own_speed_mps),
    );
    let sl = noise_at_speed_db(REFERENCE_SUBMARINE_NOISE_DB, REFERENCE_SUBMARINE_SPEED_MPS);
    let allowed_tl = sl - nl + p.di_db - p.dt_db;
    let tl = |r: f64| transmission_loss_db(r, p.frequency_khz);
    if tl(s.max_range_m) <= allowed_tl {
        return s.max_range_m;
    }
    let (mut lo, mut hi) = (1.0, s.max_range_m);
    if tl(lo) > allowed_tl {
        return 0.0;
    }
    for _ in 0..60 {
        let mid = 0.5 * (lo + hi);
        if tl(mid) <= allowed_tl {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo
}

fn nominal_range_m(
    s: &SensorSpec,
    unit: (&Unit, &Position, f64),
    emits: bool,
    env: Environment,
    unix_s: f64,
) -> f64 {
    let (u, p, speed) = unit;
    let height = sensor_height_m(s.mount_height_m, u.domain, p.altitude_m);
    let horizon = |h_t: f64| height.map_or(0.0, |h| radar_horizon_m(h, h_t));
    match s.kind {
        SensorKind::Radar => {
            let (Some(rr), Some(rc), true) = (s.ref_range_m, s.ref_rcs_m2, emits) else {
                return 0.0;
            };
            let clutter = clutter_loss_db(
                clutter_background(Domain::Surface, 0.0, false),
                env.sea_state,
                s.clutter_db_per_sea_state
                    .unwrap_or(DEFAULT_CLUTTER_DB_PER_SEA_STATE),
            );
            detection_range_with_loss_m(
                rr,
                rc,
                REFERENCE_SHIP_RCS_M2,
                s.max_range_m,
                clutter + env.weather_loss_db,
            )
            .min(horizon(REFERENCE_SHIP_HEIGHT_M))
        }
        SensorKind::Esm => s.max_range_m.min(horizon(REFERENCE_EMITTER_HEIGHT_M)),
        SensorKind::Visual | SensorKind::Eo | SensorKind::Ir => {
            let sun = sun_elevation_deg(unix_s, p.point.lat_deg, p.point.lon_deg);
            optical_r50_m(
                s.kind,
                s.max_range_m,
                1.0,
                light(sun),
                env.visibility_km * 1000.0,
            )
            .min(horizon(REFERENCE_SHIP_HEIGHT_M))
        }
        SensorKind::SonarHull | SensorKind::SonarTowed | SensorKind::SonarDipping => {
            if crate::sonar::sonar_usable(s.kind, u.domain, p.altitude_m, speed) {
                sonar_reach_m(s, speed, env)
            } else {
                0.0
            }
        }
        SensorKind::Mad => mad_r50_m(s.max_range_m),
        SensorKind::Sonobuoy => 0.0,
    }
    .min(s.max_range_m)
    .max(0.0)
}

fn bounds(area: &Area) -> (f64, f64, f64, f64) {
    match area {
        Area::Circle { centre, radius_m } => {
            let n = sim_geo::direct(*centre, 0.0, *radius_m).point;
            let s = sim_geo::direct(*centre, 180.0, *radius_m).point;
            let e = sim_geo::direct(*centre, 90.0, *radius_m).point;
            let w = sim_geo::direct(*centre, 270.0, *radius_m).point;
            (s.lat_deg, n.lat_deg, w.lon_deg, e.lon_deg)
        }
        Area::Polygon { points } => points.iter().fold(
            (f64::MAX, f64::MIN, f64::MAX, f64::MIN),
            |(s, n, w, e), p| {
                (
                    s.min(p.lat_deg),
                    n.max(p.lat_deg),
                    w.min(p.lon_deg),
                    e.max(p.lon_deg),
                )
            },
        ),
    }
}

/// The coverage of sense zone `zone` (see the module docs). `None`: no such sense zone.
#[allow(clippy::cast_precision_loss)]
pub(crate) fn zone_coverage(world: &mut World, zone: u32) -> Option<ZoneCoverage> {
    let z: Zone = world
        .resource::<PlanningLayer>()
        .zones
        .iter()
        .find(|z| z.id == zone && z.kind == ZoneKind::Sense)?
        .clone();
    let kinds: Vec<SensorKind> = if z.sensors.is_empty() {
        ALL_KINDS.to_vec()
    } else {
        z.sensors.clone()
    };
    let env = world.resource::<EnvState>().0;
    let zones = world.resource::<crate::detection::Weather>().0.clone();
    let terrain = world.resource::<crate::sim::TerrainRes>().0.clone();
    let tick = world.resource::<SimClock>().tick;
    #[allow(clippy::cast_precision_loss)]
    let unix_s = env.start_unix_s as f64 + tick.seconds();
    let c2 = world.resource::<C2State>().clone();
    let layer = world.resource::<PlanningLayer>().clone();
    let entities: Vec<Entity> = world.resource::<UnitIndex>().0.values().copied().collect();
    let mut reaches: Vec<Reach> = Vec::new();
    for e in entities {
        let (Some(u), Some(p), Some(k), Some(suite)) = (
            world.get::<Unit>(e),
            world.get::<Position>(e),
            world.get::<Kinematics>(e),
            world.get::<SensorSuite>(e),
        ) else {
            continue;
        };
        if u.side != z.side {
            continue;
        }
        let emits = layer.doctrine_for(&c2, u, p.point).emcon == Emcon::Active;
        for s in suite.0.iter().filter(|s| kinds.contains(&s.kind)) {
            let env = crate::detection::local_env(env, &zones, p.point);
            let range_m = nominal_range_m(s, (u, p, k.speed_mps), emits, env, unix_s);
            if range_m > 0.0 {
                let reference = match s.kind {
                    SensorKind::Radar | SensorKind::Visual | SensorKind::Eo | SensorKind::Ir => {
                        Some(REFERENCE_SHIP_HEIGHT_M)
                    }
                    SensorKind::Esm => Some(REFERENCE_EMITTER_HEIGHT_M),
                    _ => None,
                };
                let ground = if u.domain == Domain::Land {
                    crate::detection::ground_m(terrain.as_deref(), p.point)
                } else {
                    0.0
                };
                let heights = reference.and_then(|r| {
                    sensor_height_m(s.mount_height_m, u.domain, p.altitude_m)
                        .map(|h| (h + ground, r))
                });
                reaches.push(Reach {
                    at: p.point,
                    range_m,
                    heights,
                });
            }
        }
    }
    let (south, north, west, east) = bounds(&z.area);
    let dlat = (north - south) / GRID as f64;
    let dlon = (east - west) / GRID as f64;
    let mut cells = Vec::new();
    for row in 0..GRID {
        for col in 0..GRID {
            let centre = GeoPoint::new(
                south + (row as f64 + 0.5) * dlat,
                west + (col as f64 + 0.5) * dlon,
            );
            if !z.area.contains(centre) {
                continue;
            }
            let covered = reaches.iter().any(|r| {
                let d = inverse(r.at, centre).distance_m;
                d <= r.range_m
                    && !r.heights.is_some_and(|(h_s, h_t)| {
                        crate::detection::terrain_masks(
                            terrain.as_deref(),
                            r.at,
                            h_s,
                            centre,
                            h_t,
                            d,
                        )
                    })
            });
            cells.push(CoverageCell { centre, covered });
        }
    }
    let covered = cells.iter().filter(|c| c.covered).count();
    let fraction = if cells.is_empty() {
        0.0
    } else {
        covered as f64 / cells.len() as f64
    };
    Some(ZoneCoverage {
        zone,
        sensors: kinds,
        cells,
        half_lat_deg: dlat / 2.0,
        half_lon_deg: dlon / 2.0,
        fraction,
    })
}
