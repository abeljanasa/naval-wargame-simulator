//! Weather zones (Phase 4 slice 7, `docs/models/weather.md`).
//!
//! A **weather zone** is an area with its own sea state, wind, visibility, rain and cloud base.
//! Where zones overlap, the one with the higher id wins (so a squall drawn over a front shows).
//! Outside every zone the scenario-wide [`Environment`] holds, as it always has: a scenario with
//! no zones is one global zone and runs exactly as before.
//!
//! Everything here is plain arithmetic on the inputs (with `libm` for the one power law), so the
//! engine's use of it is the same on every OS.

use crate::Area;
use serde::{Deserialize, Serialize};
use sim_geo::GeoPoint;
use sim_sensors::Environment;

/// Visibility in a zone when none is given, km.
pub const DEFAULT_ZONE_VISIBILITY_KM: f64 = 40.0;
/// Points sampled along a path for rain and visibility.
const PATH_SAMPLES: u32 = 16;

fn default_visibility() -> f64 {
    DEFAULT_ZONE_VISIBILITY_KM
}

/// An area with its own weather.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct WeatherZone {
    pub id: u32,
    pub name: String,
    pub area: Area,
    /// Douglas sea state, 0–9.
    pub sea_state: u8,
    /// Mean wind, knots, and the direction it blows from, degrees true.
    #[serde(default)]
    pub wind_kn: f64,
    #[serde(default)]
    pub wind_from_deg: f64,
    /// Meteorological visibility, km.
    #[serde(default = "default_visibility")]
    pub visibility_km: f64,
    /// Rain rate, mm/h (0: dry).
    #[serde(default)]
    pub rain_mm_h: f64,
    /// Cloud base, metres above sea level (`None`: no low cloud).
    #[serde(default)]
    pub cloud_base_m: Option<f64>,
}

impl WeatherZone {
    /// Checks the zone's figures.
    pub fn validate(&self) -> Result<(), String> {
        let bad = |m: &str| Err(format!("weather zone {:?}: {m}", self.name));
        if self.name.trim().is_empty() {
            return Err(format!("weather zone {} has no name", self.id));
        }
        self.area.validate()?;
        if self.sea_state > 9 {
            return bad("sea state must be 0–9");
        }
        if !(self.wind_kn.is_finite() && (0.0..=200.0).contains(&self.wind_kn)) {
            return bad("wind must be 0–200 kn");
        }
        if !self.wind_from_deg.is_finite() {
            return bad("wind direction is not a number");
        }
        if !(self.visibility_km.is_finite() && self.visibility_km > 0.0) {
            return bad("visibility must be over 0 km");
        }
        if !(self.rain_mm_h.is_finite() && (0.0..=500.0).contains(&self.rain_mm_h)) {
            return bad("rain must be 0–500 mm/h");
        }
        if self
            .cloud_base_m
            .is_some_and(|c| !(c.is_finite() && c >= 0.0))
        {
            return bad("cloud base must be 0 m or higher");
        }
        Ok(())
    }
}

/// Checks a set of zones: each one, and unique ids.
pub fn validate_weather(zones: &[WeatherZone]) -> Result<(), String> {
    let mut ids = std::collections::BTreeSet::new();
    for z in zones {
        if !ids.insert(z.id) {
            return Err(format!("duplicate weather zone id {}", z.id));
        }
        z.validate()?;
    }
    Ok(())
}

/// The weather at one point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LocalWeather {
    pub sea_state: u8,
    pub visibility_km: f64,
    pub rain_mm_h: f64,
    pub wind_kn: f64,
    pub cloud_base_m: Option<f64>,
}

/// The weather at `p`: the zone with the highest id that contains it, else the scenario-wide
/// environment (no rain, no wind, no low cloud: those exist only in zones).
#[must_use]
pub fn weather_at(env: &Environment, zones: &[WeatherZone], p: GeoPoint) -> LocalWeather {
    zones
        .iter()
        .filter(|z| z.area.contains(p))
        .max_by_key(|z| z.id)
        .map_or(
            LocalWeather {
                sea_state: env.sea_state,
                visibility_km: env.visibility_km,
                rain_mm_h: 0.0,
                wind_kn: 0.0,
                cloud_base_m: None,
            },
            |z| LocalWeather {
                sea_state: z.sea_state,
                visibility_km: z.visibility_km,
                rain_mm_h: z.rain_mm_h,
                wind_kn: z.wind_kn,
                cloud_base_m: z.cloud_base_m,
            },
        )
}

/// One-way specific attenuation of rain for a generic X-band (about 10 GHz) radar, dB/km:
/// `γ = k·Rᵅ` with k = 0.0122, α = 1.257 (the power-law form and order of magnitude of
/// ITU-R P.838 at 10 GHz). 0 when dry.
#[must_use]
pub fn rain_db_per_km(rain_mm_h: f64) -> f64 {
    if rain_mm_h <= 0.0 {
        0.0
    } else {
        0.0122 * libm::pow(rain_mm_h, 1.257)
    }
}

/// Points along the straight path from `a` to `b` (16 segments' midpoints), interpolated in
/// latitude and longitude.
fn path_points(a: GeoPoint, b: GeoPoint) -> impl Iterator<Item = GeoPoint> {
    let mut dlon = b.lon_deg - a.lon_deg;
    if dlon > 180.0 {
        dlon -= 360.0;
    } else if dlon < -180.0 {
        dlon += 360.0;
    }
    let dlat = b.lat_deg - a.lat_deg;
    (0..PATH_SAMPLES).map(move |i| {
        let t = (f64::from(i) + 0.5) / f64::from(PATH_SAMPLES);
        GeoPoint::new(a.lat_deg + t * dlat, a.lon_deg + t * dlon)
    })
}

/// Two-way rain loss on a radar path of `range_m`, dB: the rain at 16 points along it. 0 with
/// no zones (the scenario-wide `weather_loss_db` is separate and unchanged).
#[must_use]
pub fn path_rain_loss_db(
    env: &Environment,
    zones: &[WeatherZone],
    a: GeoPoint,
    b: GeoPoint,
    range_m: f64,
) -> f64 {
    if zones.iter().all(|z| z.rain_mm_h <= 0.0) {
        return 0.0;
    }
    let seg_km = range_m / 1000.0 / f64::from(PATH_SAMPLES);
    let one_way: f64 = path_points(a, b)
        .map(|p| rain_db_per_km(weather_at(env, zones, p).rain_mm_h) * seg_km)
        .sum();
    2.0 * one_way
}

/// The least visibility along the path (both ends and 16 points between), km. The
/// scenario-wide visibility with no zones.
#[must_use]
pub fn path_visibility_km(
    env: &Environment,
    zones: &[WeatherZone],
    a: GeoPoint,
    b: GeoPoint,
) -> f64 {
    if zones.is_empty() {
        return env.visibility_km;
    }
    std::iter::once(a)
        .chain(path_points(a, b))
        .chain(std::iter::once(b))
        .map(|p| weather_at(env, zones, p).visibility_km)
        .fold(f64::INFINITY, f64::min)
}

/// Whether low cloud stands between an observer at `h_a` and a target at `h_b` (metres above sea
/// level): one is below the cloud base at the path's middle and the other above it.
#[must_use]
pub fn cloud_between(
    env: &Environment,
    zones: &[WeatherZone],
    a: GeoPoint,
    h_a: f64,
    b: GeoPoint,
    h_b: f64,
) -> bool {
    let mid = GeoPoint::new(
        (a.lat_deg + b.lat_deg) / 2.0,
        a.lon_deg + {
            let mut d = b.lon_deg - a.lon_deg;
            if d > 180.0 {
                d -= 360.0;
            } else if d < -180.0 {
                d += 360.0;
            }
            d / 2.0
        },
    );
    weather_at(env, zones, mid)
        .cloud_base_m
        .is_some_and(|base| (h_a < base) != (h_b < base))
}

/// The share of its top speed a surface hull of `length_m` can make in `sea_state`: full speed
/// up to two steps below its weather limit (`limit`, from the planning layer: 5 under 60 m, 7
/// under 120 m, 8 above), 85 % one step below, 60 % at or above it (heading into shelter).
#[must_use]
pub fn speed_factor(sea_state: u8, limit: u8) -> f64 {
    if sea_state >= limit {
        0.6
    } else if sea_state + 1 == limit {
        0.85
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zone(id: u32, centre: (f64, f64), radius_m: f64) -> WeatherZone {
        WeatherZone {
            id,
            name: format!("zone {id}"),
            area: Area::Circle {
                centre: GeoPoint::new(centre.0, centre.1),
                radius_m,
            },
            sea_state: 6,
            wind_kn: 35.0,
            wind_from_deg: 45.0,
            visibility_km: 2.0,
            rain_mm_h: 16.0,
            cloud_base_m: Some(300.0),
        }
    }

    fn env() -> Environment {
        Environment {
            sea_state: 2,
            visibility_km: 30.0,
            ..Environment::default()
        }
    }

    #[test]
    fn the_highest_zone_wins_and_outside_is_the_scenario_weather() {
        let mut squall = zone(2, (0.0, 0.0), 5_000.0);
        squall.sea_state = 8;
        let zones = vec![zone(1, (0.0, 0.0), 50_000.0), squall];
        let e = env();
        assert_eq!(weather_at(&e, &zones, GeoPoint::new(0.0, 0.0)).sea_state, 8);
        assert_eq!(weather_at(&e, &zones, GeoPoint::new(0.2, 0.0)).sea_state, 6);
        let out = weather_at(&e, &zones, GeoPoint::new(5.0, 5.0));
        assert_eq!(
            (out.sea_state, out.visibility_km, out.rain_mm_h),
            (2, 30.0, 0.0)
        );
    }

    #[test]
    fn rain_attenuates_only_where_it_falls() {
        // 4 mm/h: about 0.07 dB/km; 16 mm/h about 0.40 dB/km (one way).
        assert!((rain_db_per_km(4.0) - 0.0697).abs() < 0.001);
        assert!((rain_db_per_km(16.0) - 0.399).abs() < 0.002);
        assert_eq!(rain_db_per_km(0.0), 0.0);
        let zones = vec![zone(1, (0.0, 0.5), 20_000.0)];
        let e = env();
        let (a, b) = (GeoPoint::new(0.0, 0.0), GeoPoint::new(0.0, 1.0));
        let d = sim_geo::inverse(a, b).distance_m;
        let loss = path_rain_loss_db(&e, &zones, a, b, d);
        // About 40 km of the 111 km path is in 16 mm/h rain: about 2 × 0.4 × 40 = 32 dB.
        assert!((25.0..40.0).contains(&loss), "{loss}");
        assert_eq!(path_rain_loss_db(&e, &[], a, b, d), 0.0);
    }

    #[test]
    fn visibility_and_cloud_along_a_path() {
        let zones = vec![zone(1, (0.0, 0.5), 20_000.0)];
        let e = env();
        let (a, b) = (GeoPoint::new(0.0, 0.0), GeoPoint::new(0.0, 1.0));
        assert_eq!(path_visibility_km(&e, &zones, a, b), 2.0);
        assert_eq!(path_visibility_km(&e, &[], a, b), 30.0);
        // A ship looking at an aircraft above the 300 m cloud base: hidden; below it: seen.
        assert!(cloud_between(&e, &zones, a, 20.0, b, 3000.0));
        assert!(!cloud_between(&e, &zones, a, 20.0, b, 200.0));
        assert!(!cloud_between(&e, &[], a, 20.0, b, 3000.0));
    }

    #[test]
    fn heavy_seas_slow_hulls_by_their_weather_limit() {
        assert_eq!(speed_factor(3, 7), 1.0);
        assert_eq!(speed_factor(6, 7), 0.85);
        assert_eq!(speed_factor(7, 7), 0.6);
        assert_eq!(speed_factor(9, 8), 0.6);
    }

    #[test]
    fn checks_the_figures() {
        assert!(validate_weather(&[zone(1, (0.0, 0.0), 1_000.0)]).is_ok());
        let mut z = zone(1, (0.0, 0.0), 1_000.0);
        z.sea_state = 10;
        assert!(z.validate().is_err());
        z.sea_state = 3;
        z.visibility_km = 0.0;
        assert!(z.validate().is_err());
        let dup = vec![zone(1, (0.0, 0.0), 1.0), zone(1, (0.0, 0.0), 1.0)];
        assert!(validate_weather(&dup).unwrap_err().contains("duplicate"));
    }
}
