//! Sonar pass of the detection system. The equations are in `sim_sensors::sonar` (ADR-0005);
//! model doc `docs/models/sonar.md`.
//!
//! Order (fixed for determinism, ADR-0003): observers in `UnitId` order, their sonars in mount
//! order, waterborne targets of other sides in `UnitId` order. Each pair in range takes one
//! `Sensors` draw; a detection takes two more for its measurement error.
//!
//! * **Active** (an active-capable sonar on a unit whose EMCON is active): range and bearing,
//!   fused into the side's track.
//! * **Passive** (every other sonar): a bearing, pooled with ESM bearings for fixes.
//!   A target that is pinging is heard at its ping level: active sonar reveals the emitter.

use crate::components::Domain;
use crate::detection::{ContactTable, Measurement, Observed, SensorKind};
use crate::esm::{note_bearing, BearingTable, FreshBearings};
use crate::events::{EventLog, SimEventKind};
use crate::rng::{RngStreams, StreamId};
use crate::time::Tick;
use sim_geo::{chord_m, inverse, normalize_azimuth_deg};
use sim_sensors::{
    active_se_db, ambient_noise_db, gaussian_pair, measured_point, measurement_error,
    noise_at_speed_db, passive_se_db, pd_from_se, power_sum_db, sonar_params, transmission_loss_db,
    BearingLine, Classification, Environment, Identification, DIPPING_MAX_ALTITUDE_M,
    DIPPING_MAX_SPEED_MPS,
};

/// Target strength of a submarine for active sonar, dB (generic).
pub const TARGET_STRENGTH_SUBMARINE_DB: f64 = 15.0;
/// Target strength of a surface ship for active sonar, dB (generic).
pub const TARGET_STRENGTH_SURFACE_DB: f64 = 20.0;
/// Radiated noise at 10 kn for a surface unit without a platform figure, dB.
pub const DEFAULT_SURFACE_NOISE_DB: f64 = 140.0;
/// Radiated noise at 10 kn for a submarine without a platform figure, dB.
pub const DEFAULT_SUBMARINE_NOISE_DB: f64 = 115.0;
/// Label on passive sonar bearing lines.
pub const ACOUSTIC_LABEL: &str = "acoustic";
const MIN_METRES_PER_DEG_LAT: f64 = 110_000.0;

fn is_sonar(kind: SensorKind) -> bool {
    matches!(
        kind,
        SensorKind::SonarHull
            | SensorKind::SonarTowed
            | SensorKind::SonarDipping
            | SensorKind::Sonobuoy
    )
}

fn waterborne(domain: Domain) -> bool {
    matches!(domain, Domain::Surface | Domain::Subsurface)
}

/// Whether a sonar can work from its platform's current state. Sonobuoys must be laid first
/// (missions, Phase 2 slice 10); a dipping sonar needs a low hover.
#[must_use]
pub fn sonar_usable(kind: SensorKind, domain: Domain, altitude_m: f64, speed_mps: f64) -> bool {
    match kind {
        SensorKind::Sonobuoy => false,
        SensorKind::SonarDipping => {
            domain == Domain::Air
                && altitude_m <= DIPPING_MAX_ALTITUDE_M
                && speed_mps <= DIPPING_MAX_SPEED_MPS
        }
        _ => waterborne(domain),
    }
}

/// Default radiated noise at 10 kn for a domain.
#[must_use]
pub fn default_radiated_noise_db(domain: Domain) -> f64 {
    match domain {
        Domain::Subsurface => DEFAULT_SUBMARINE_NOISE_DB,
        _ => DEFAULT_SURFACE_NOISE_DB,
    }
}

fn target_strength_db(domain: Domain) -> f64 {
    if domain == Domain::Subsurface {
        TARGET_STRENGTH_SUBMARINE_DB
    } else {
        TARGET_STRENGTH_SURFACE_DB
    }
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub(crate) fn sonar_pass(
    all: &[Observed<'_>],
    ecef: &[[f64; 3]],
    tick: Tick,
    env: Environment,
    rng: &mut RngStreams,
    contacts: &mut ContactTable,
    log: &mut EventLog,
    bearings: &mut BearingTable,
    fresh: &mut FreshBearings,
) {
    // The loudest active sonar each unit is transmitting with, if any.
    let pinging: Vec<Option<f64>> = all
        .iter()
        .map(|o| {
            if !o.emits {
                return None;
            }
            o.sensors
                .0
                .iter()
                .filter(|s| {
                    is_sonar(s.kind)
                        && s.active
                        && sonar_usable(s.kind, o.domain, o.position.altitude_m, o.speed_mps)
                })
                .map(|s| sonar_params(s.kind, true).active_sl_db)
                .fold(None, |acc: Option<f64>, sl| {
                    Some(acc.map_or(sl, |a| a.max(sl)))
                })
        })
        .collect();
    let mut by_lat: Vec<(f64, usize)> = all
        .iter()
        .enumerate()
        .filter(|(_, t)| waterborne(t.domain))
        .map(|(i, t)| (t.position.point.lat_deg, i))
        .collect();
    if by_lat.is_empty() {
        return;
    }
    by_lat.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    let ambient = ambient_noise_db(env.sea_state);
    let mut candidates: Vec<usize> = Vec::new();
    for (oi, obs) in all.iter().enumerate() {
        for s in obs.sensors.0.iter().filter(|s| is_sonar(s.kind)) {
            if !sonar_usable(s.kind, obs.domain, obs.position.altitude_m, obs.speed_mps) {
                continue;
            }
            let active = s.active && obs.emits;
            let p = sonar_params(s.kind, s.active);
            let nl = power_sum_db(ambient, noise_at_speed_db(p.self_noise_db, obs.speed_mps));
            let err = measurement_error(s.kind);
            let lat0 = obs.position.point.lat_deg;
            let band = s.max_range_m / MIN_METRES_PER_DEG_LAT;
            let lo = by_lat.partition_point(|&(lat, _)| lat < lat0 - band);
            let hi = by_lat.partition_point(|&(lat, _)| lat <= lat0 + band);
            candidates.clear();
            candidates.extend(
                by_lat[lo..hi]
                    .iter()
                    .map(|&(_, i)| i)
                    .filter(|&i| all[i].side != obs.side),
            );
            candidates.sort_unstable();
            for &i in &candidates {
                let tgt = &all[i];
                // Exact pre-check (chord ≤ geodesic), as in the radar and ESM passes.
                if chord_m(ecef[oi], ecef[i]) > s.max_range_m + 1.0 {
                    continue;
                }
                let inv = inverse(obs.position.point, tgt.position.point);
                let range = inv.distance_m;
                if range < s.min_range_m || range > s.max_range_m {
                    continue;
                }
                let tl = transmission_loss_db(range, p.frequency_khz);
                let (sl, se) = if active {
                    let sl = p.active_sl_db;
                    let ts = target_strength_db(tgt.domain);
                    (sl, active_se_db(sl, tl, ts, nl, p.di_db, p.dt_db))
                } else {
                    let radiated = noise_at_speed_db(tgt.radiated_noise_db, tgt.speed_mps);
                    let sl = pinging[i].map_or(radiated, |ping| power_sum_db(radiated, ping));
                    (sl, passive_se_db(sl, tl, nl, p.di_db, p.dt_db))
                };
                let pd = pd_from_se(se);
                let draw = rng.uniform(StreamId::Sensors);
                let detected = draw < pd;
                let normals = detected.then(|| {
                    gaussian_pair(
                        rng.uniform(StreamId::Sensors),
                        rng.uniform(StreamId::Sensors),
                    )
                });
                let range_sigma = err.range_sigma_m.filter(|_| active);
                let (range_error, bearing_error) = match normals {
                    Some((n1, n2)) if range_sigma.is_some() => (
                        range_sigma.map(|r| n1 * r),
                        Some(n2 * err.bearing_sigma_deg),
                    ),
                    Some((n1, _)) => (None, Some(n1 * err.bearing_sigma_deg)),
                    None => (None, None),
                };
                log.push(
                    tick,
                    SimEventKind::SonarAttempt {
                        observer: obs.id,
                        sensor: s.sensor_id.clone(),
                        target: tgt.id,
                        active,
                        range_m: range,
                        sl_db: sl,
                        tl_db: tl,
                        nl_db: nl,
                        di_db: p.di_db,
                        dt_db: p.dt_db,
                        se_db: se,
                        pd,
                        draw,
                        detected,
                        range_error_m: range_error,
                        bearing_error_deg: bearing_error,
                    },
                );
                let Some(bearing_err) = bearing_error else {
                    continue;
                };
                if let (Some(sigma_r), Some(range_err)) = (range_sigma, range_error) {
                    // Active: range and bearing, a full position measurement.
                    let (point, cov, _, _) = measured_point(
                        obs.position.point,
                        range,
                        inv.azimuth1_deg,
                        sigma_r,
                        err.bearing_sigma_deg,
                        range_err / sigma_r,
                        bearing_err / err.bearing_sigma_deg,
                    );
                    let m = Measurement {
                        point,
                        cov,
                        classification: Classification::Domain,
                        category: None,
                        platform: None,
                        domain: tgt.domain,
                        observer: obs.id,
                        sensor_id: &s.sensor_id,
                    };
                    if let Some(number) = contacts.record(obs.side, tgt.id, tick, &m) {
                        log.push(
                            tick,
                            SimEventKind::ContactGained {
                                side: obs.side,
                                number,
                                target: tgt.id,
                            },
                        );
                    }
                } else {
                    // Passive: a bearing. It tells surface from submerged (domain), no more.
                    let line = BearingLine {
                        from: obs.position.point,
                        bearing_deg: normalize_azimuth_deg(inv.azimuth1_deg + bearing_err),
                        sigma_deg: err.bearing_sigma_deg,
                    };
                    note_bearing(
                        fresh,
                        bearings,
                        obs.side,
                        obs.id,
                        tgt.id,
                        i,
                        line,
                        tick,
                        ACOUSTIC_LABEL,
                        Identification {
                            classification: Classification::Domain,
                            platform: None,
                            category: None,
                        },
                    );
                }
            }
        }
    }
}
