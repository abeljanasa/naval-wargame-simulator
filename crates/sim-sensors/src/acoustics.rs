//! Ocean acoustics with a sound-velocity profile (Phase 4 slice 8, `docs/models/acoustics.md`).
//!
//! With a profile in force, one-way transmission loss is the least of the paths sound can take:
//!
//! - **surface duct**: both ends in the mixed layer: spherical spreading to the skip distance,
//!   cylindrical beyond;
//! - **direct**: spherical spreading within the direct-path range, a deep **shadow** beyond;
//! - **cross-layer**: one end in the layer and one below: the direct path plus a layer loss;
//! - **convergence zone**: within an annulus at each multiple of the CZ range, where the deep
//!   refracted rays come back up, with a focusing gain;
//! - **bottom bounce**: off the sea floor where the depth is known (the terrain packs), with a
//!   bottom loss per bounce.
//!
//! Absorption (Thorp) applies on every path. The CZ range comes from tracing the limiting ray
//! through the piecewise-linear profile (closed-form arcs per layer). All arithmetic is plain
//! or `libm`, so results are the same on every OS.

use crate::sonar::thorp_absorption_db_per_km;
use serde::{Deserialize, Serialize};

/// Below the deepest profile point, sound speed rises with pressure at this gradient, s⁻¹.
pub const DEEP_GRADIENT_PER_S: f64 = 0.017;
/// Sound speed rise in an isothermal mixed layer, s⁻¹ (pressure alone).
pub const LAYER_GRADIENT_PER_S: f64 = 0.0163;
/// Extra loss crossing the layer boundary, dB (one way).
pub const CROSS_LAYER_DB: f64 = 10.0;
/// Extra loss deep in the shadow zone, dB.
pub const SHADOW_DB: f64 = 30.0;
/// Focusing gain inside a convergence-zone annulus, dB.
pub const CZ_GAIN_DB: f64 = 10.0;
/// Width of a convergence-zone annulus as a share of its range.
pub const CZ_WIDTH_FRACTION: f64 = 0.06;
/// Bottom loss per bounce, dB (moderate sediment, low grazing angle), and its rise above 5 kHz.
pub const BOTTOM_LOSS_DB: f64 = 10.0;
pub const BOTTOM_LOSS_HF_DB: f64 = 5.0;
/// Shallowest water in which a bottom-bounce path is used, m.
pub const MIN_BOUNCE_DEPTH_M: f64 = 50.0;
/// Convergence zones considered (first, second, third).
pub const CZ_ORDERS: u32 = 3;

/// One point of a sound-velocity profile.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SvpPoint {
    pub depth_m: f64,
    pub speed_mps: f64,
}

/// A sound-velocity profile: points by increasing depth, from the surface.
#[derive(Debug, Clone, PartialEq)]
pub struct Svp {
    points: Vec<SvpPoint>,
}

/// Which way the sound went.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum AcousticPath {
    SurfaceDuct,
    Direct,
    CrossLayer,
    Shadow,
    ConvergenceZone,
    BottomBounce,
}

/// Checks profile points: at least two, depths from 0 increasing, plausible speeds.
pub fn validate_svp(points: &[SvpPoint]) -> Result<(), String> {
    if points.len() < 2 {
        return Err("a sound-velocity profile needs at least two points".into());
    }
    if points[0].depth_m != 0.0 {
        return Err("a sound-velocity profile starts at the surface (depth 0)".into());
    }
    for w in points.windows(2) {
        if w[1].depth_m.is_nan() || w[1].depth_m <= w[0].depth_m {
            return Err("profile depths must increase".into());
        }
    }
    if points
        .iter()
        .any(|p| !(p.speed_mps.is_finite() && (1400.0..=1600.0).contains(&p.speed_mps)))
    {
        return Err("sound speeds must be 1,400–1,600 m/s".into());
    }
    if points
        .iter()
        .any(|p| !p.depth_m.is_finite() || p.depth_m > 12_000.0)
    {
        return Err("profile depths must be 0–12,000 m".into());
    }
    Ok(())
}

impl Svp {
    /// A profile from checked points.
    pub fn new(points: Vec<SvpPoint>) -> Result<Self, String> {
        validate_svp(&points)?;
        Ok(Self { points })
    }

    /// Sound speed at depth `z`, linear between points, the deep gradient below the last.
    #[must_use]
    pub fn speed(&self, z: f64) -> f64 {
        let z = z.max(0.0);
        let last = self.points[self.points.len() - 1];
        if z >= last.depth_m {
            return last.speed_mps + DEEP_GRADIENT_PER_S * (z - last.depth_m);
        }
        for w in self.points.windows(2) {
            if z <= w[1].depth_m {
                let t = (z - w[0].depth_m) / (w[1].depth_m - w[0].depth_m);
                return w[0].speed_mps + t * (w[1].speed_mps - w[0].speed_mps);
            }
        }
        last.speed_mps
    }

    /// Mixed-layer depth: the depth of the greatest speed in the top 500 m when the speed rises
    /// from the surface to it (a surface duct), else 0.
    #[must_use]
    pub fn mixed_layer_depth(&self) -> f64 {
        let mut best = (0.0, self.points[0].speed_mps);
        for p in self.points.iter().take_while(|p| p.depth_m <= 500.0) {
            if p.speed_mps > best.1 {
                best = (p.depth_m, p.speed_mps);
            } else if p.speed_mps < best.1 {
                break;
            }
        }
        best.0
    }

    /// Breakpoints between `z1` and `z2` (inclusive ends), for tracing layer by layer.
    fn breaks(&self, z1: f64, z2: f64) -> Vec<f64> {
        let mut v = vec![z1];
        v.extend(
            self.points
                .iter()
                .map(|p| p.depth_m)
                .filter(|&d| d > z1 && d < z2),
        );
        v.push(z2);
        v
    }

    /// The limiting ray leaving `z0` horizontally, traced down: the depth where it turns back
    /// up (sound speed back to the launch speed) and the horizontal distance to it. `None` if
    /// it does not turn above `max_depth`.
    #[must_use]
    pub fn turning_point(&self, z0: f64, max_depth: f64) -> Option<(f64, f64)> {
        let a = self.speed(z0);
        let mut range = 0.0;
        let zs = self.breaks(z0, max_depth.max(z0 + 1.0));
        for w in zs.windows(2) {
            let (z1, z2) = (w[0], w[1]);
            let (c1, c2) = (self.speed(z1), self.speed(z2));
            let g = (c2 - c1) / (z2 - z1);
            let s1 = libm::sqrt((a * a - c1 * c1).max(0.0));
            if c2 >= a && g > 0.0 {
                // Turns inside this layer, where c = a.
                return Some((z1 + (a - c1) / g, range + s1 / g));
            }
            let s2 = libm::sqrt((a * a - c2 * c2).max(0.0));
            if g.abs() < 1e-12 {
                // Constant speed: a straight ray at the angle it has.
                if s1 <= 0.0 {
                    return None;
                }
                range += (z2 - z1) / (s1 / c1);
            } else {
                range += (s2 - s1).abs() / g.abs();
            }
        }
        None
    }

    /// The first convergence-zone range and the depth its rays reach, for sound leaving the
    /// layer (or the surface). `None` if the profile, or the sea floor (`water_depth_m`), stops
    /// the rays turning.
    #[must_use]
    pub fn convergence_zone(&self, water_depth_m: Option<f64>) -> Option<(f64, f64)> {
        let z0 = self.mixed_layer_depth();
        let floor = water_depth_m.unwrap_or(12_000.0);
        let (turn, half) = self.turning_point(z0, floor)?;
        (turn < floor).then_some((2.0 * half, turn))
    }
}

/// Spherical then cylindrical spreading with a transition at `r_t`, plus absorption, dB.
fn ducted_tl(r: f64, r_t: f64, alpha_db_per_m: f64) -> f64 {
    let spreading = if r <= r_t {
        20.0 * libm::log10(r)
    } else {
        20.0 * libm::log10(r_t) + 10.0 * libm::log10(r / r_t)
    };
    spreading + alpha_db_per_m * r
}

/// One-way transmission loss from a source at `src_depth_m` to a receiver at `rcv_depth_m`,
/// `range_m` apart, at `f_khz`, in water `water_depth_m` deep (`None`: not known), and the path
/// it took (see the module notes).
#[must_use]
pub fn transmission_loss(
    svp: &Svp,
    f_khz: f64,
    src_depth_m: f64,
    rcv_depth_m: f64,
    range_m: f64,
    water_depth_m: Option<f64>,
) -> (f64, AcousticPath) {
    let r = range_m.max(1.0);
    let alpha = thorp_absorption_db_per_km(f_khz) / 1000.0;
    let mld = svp.mixed_layer_depth();
    let in_layer = |z: f64| mld > 0.0 && z <= mld;
    let c0 = svp.speed(0.0);
    let mut paths: Vec<(f64, AcousticPath)> = Vec::new();

    // Surface duct: skip distance √(8·Rc·H) with Rc = c / g_layer.
    if in_layer(src_depth_m) && in_layer(rcv_depth_m) {
        let rc = c0 / LAYER_GRADIENT_PER_S;
        let skip = libm::sqrt(8.0 * rc * mld);
        paths.push((ducted_tl(r, skip, alpha), AcousticPath::SurfaceDuct));
    } else {
        // Direct path, limited by downward refraction below the layer: the horizon of each end
        // on a ray circle of radius Rc = c / |g| (g the gradient just below the layer).
        let below = mld.max(src_depth_m.min(rcv_depth_m)) + 1.0;
        let g = (svp.speed(below + 50.0) - svp.speed(below)) / 50.0;
        let rc = if g < 0.0 {
            svp.speed(below) / -g
        } else {
            f64::INFINITY
        };
        let dz = |z: f64| (z - mld).abs().max(10.0);
        let r_dp = libm::sqrt(2.0 * rc * dz(src_depth_m)) + libm::sqrt(2.0 * rc * dz(rcv_depth_m));
        let mut tl = 20.0 * libm::log10(r) + alpha * r;
        let crossing = in_layer(src_depth_m) != in_layer(rcv_depth_m);
        if crossing {
            tl += CROSS_LAYER_DB;
        }
        if r > r_dp {
            paths.push((tl + SHADOW_DB, AcousticPath::Shadow));
        } else if crossing {
            paths.push((tl, AcousticPath::CrossLayer));
        } else {
            paths.push((tl, AcousticPath::Direct));
        }
    }

    // Convergence zones.
    if let Some((cz, _)) = svp.convergence_zone(water_depth_m) {
        for k in 1..=CZ_ORDERS {
            let centre = cz * f64::from(k);
            if (r - centre).abs() <= CZ_WIDTH_FRACTION * centre / 2.0 {
                paths.push((
                    20.0 * libm::log10(r) + alpha * r - CZ_GAIN_DB,
                    AcousticPath::ConvergenceZone,
                ));
            }
        }
    }

    // Bottom bounce: one bounce, the slant path down and up.
    if let Some(d) = water_depth_m.filter(|&d| d >= MIN_BOUNCE_DEPTH_M) {
        let vertical = (2.0 * d - src_depth_m - rcv_depth_m).max(1.0);
        let slant = libm::sqrt(r * r + vertical * vertical);
        let bl = BOTTOM_LOSS_DB + if f_khz > 5.0 { BOTTOM_LOSS_HF_DB } else { 0.0 };
        paths.push((
            20.0 * libm::log10(slant) + alpha * slant + bl,
            AcousticPath::BottomBounce,
        ));
    }

    paths
        .into_iter()
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .unwrap_or((20.0 * libm::log10(r) + alpha * r, AcousticPath::Direct))
}

/// Pulse length of a generic active sonar, s, and its horizontal beamwidth, radians.
pub const PULSE_S: f64 = 0.1;
pub const BEAM_RAD: f64 = 0.2;

/// Reverberation level at range `range_m` for an active sonar of source level `sl_db` with
/// one-way loss `tl_db`: the boundary that scatters (sea floor in water under 200 m, else the
/// surface, rougher with the sea state) over the area one pulse and one beam cover:
/// RL = SL − 2·TL + S + 10·log₁₀(r·θ·c·τ/2).
#[must_use]
pub fn reverberation_db(
    sl_db: f64,
    tl_db: f64,
    range_m: f64,
    sea_state: u8,
    water_depth_m: Option<f64>,
) -> f64 {
    let scattering = match water_depth_m {
        Some(d) if d < 200.0 => -30.0,
        _ => -50.0 + 3.0 * f64::from(sea_state),
    };
    let area = range_m.max(1.0) * BEAM_RAD * 1500.0 * PULSE_S / 2.0;
    sl_db - 2.0 * tl_db + scattering + 10.0 * libm::log10(area)
}

/// The month (1–12, UTC) of a time in Unix seconds, for seasonal profiles (civil calendar,
/// integer arithmetic).
#[must_use]
pub fn month_of(unix_s: i64) -> u8 {
    let days = unix_s.div_euclid(86_400);
    // Howard Hinnant's civil-from-days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    u8::try_from(m).unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A typical deep-water profile: a 60 m isothermal mixed layer, a thermocline to a 1,000 m
    /// channel axis, then the pressure gradient.
    fn deep() -> Svp {
        Svp::new(vec![
            SvpPoint {
                depth_m: 0.0,
                speed_mps: 1520.0,
            },
            SvpPoint {
                depth_m: 60.0,
                speed_mps: 1521.0,
            },
            SvpPoint {
                depth_m: 1000.0,
                speed_mps: 1490.0,
            },
            SvpPoint {
                depth_m: 4000.0,
                speed_mps: 1541.0,
            },
        ])
        .unwrap()
    }

    #[test]
    fn profile_shape() {
        let s = deep();
        assert_eq!(s.mixed_layer_depth(), 60.0);
        assert!((s.speed(530.0) - 1505.5).abs() < 1e-9);
        assert!((s.speed(5000.0) - 1558.0).abs() < 1e-9);
        assert!(validate_svp(&[SvpPoint {
            depth_m: 5.0,
            speed_mps: 1500.0
        }])
        .is_err());
        assert!(Svp::new(vec![
            SvpPoint {
                depth_m: 0.0,
                speed_mps: 1500.0
            },
            SvpPoint {
                depth_m: 0.0,
                speed_mps: 1501.0
            },
        ])
        .is_err());
    }

    /// Hand calculation of the limiting ray. From 60 m (c = a = 1521): down the thermocline
    /// (g = −31/940) to 1,000 m: √(1521² − 1490²)/|g| = 305.4/0.03298 = 9,261 m. Then up the
    /// deep gradient (g = 0.017): it turns where c = 1521, at 1000 + 31/0.017 = 2,823.5 m, a
    /// further 305.4/0.017 = 17,964 m. Half the CZ: 27,225 m; the CZ: 54.4 km.
    #[test]
    fn convergence_zone_matches_the_hand_calculation() {
        let s = deep();
        let (turn, half) = s.turning_point(60.0, 6000.0).unwrap();
        assert!((turn - 2823.5).abs() < 1.0, "{turn}");
        assert!((half - 27_225.0).abs() < 30.0, "{half}");
        let (cz, _) = s.convergence_zone(None).unwrap();
        assert!((cz - 54_450.0).abs() < 60.0, "{cz}");
        // Too shallow a sea: the rays hit the bottom first; no CZ.
        assert!(s.convergence_zone(Some(2000.0)).is_none());
    }

    #[test]
    fn transmission_loss_by_path_matches_the_hand_calculation() {
        let s = deep();
        let f = 1.0;
        let alpha = thorp_absorption_db_per_km(f) / 1000.0;
        // Both in the 60 m layer at 20 km: skip √(8 × 1520/0.0163 × 60) = 6,689 m;
        // TL = 20 log 6689 + 10 log(20000/6689) + α·20000.
        let skip = libm::sqrt(8.0 * 1520.0 / LAYER_GRADIENT_PER_S * 60.0);
        let (tl, path) = transmission_loss(&s, f, 6.0, 30.0, 20_000.0, None);
        assert_eq!(path, AcousticPath::SurfaceDuct);
        let hand =
            20.0 * libm::log10(skip) + 10.0 * libm::log10(20_000.0 / skip) + alpha * 20_000.0;
        assert!((tl - hand).abs() < 1e-9, "{tl} vs {hand}");
        // In the first CZ (54.4 km), a submarine at 200 m heard by a ship at 6 m: CZ gain.
        let (cz, _) = s.convergence_zone(None).unwrap();
        let (tl, path) = transmission_loss(&s, f, 200.0, 6.0, cz, None);
        assert_eq!(path, AcousticPath::ConvergenceZone);
        let hand = 20.0 * libm::log10(cz) + alpha * cz - CZ_GAIN_DB;
        assert!((tl - hand).abs() < 1e-9);
        // At 35 km, between the duct's reach and the CZ, a submarine below the layer is in the
        // shadow (cross-layer and beyond the direct path).
        let (_, path) = transmission_loss(&s, f, 200.0, 6.0, 35_000.0, None);
        assert_eq!(path, AcousticPath::Shadow);
        // Over 1,500 m of water, the bottom bounce lights the shadow: one bounce, slant
        // √(35000² + (3000 − 206)²), bottom loss 10 dB.
        let (tl, path) = transmission_loss(&s, f, 200.0, 6.0, 35_000.0, Some(1500.0));
        assert_eq!(path, AcousticPath::BottomBounce);
        let slant = libm::sqrt(35_000.0f64.powi(2) + 2794.0f64.powi(2));
        let hand = 20.0 * libm::log10(slant) + alpha * slant + BOTTOM_LOSS_DB;
        assert!((tl - hand).abs() < 1e-9);
    }

    #[test]
    fn reverberation_falls_with_range_and_rises_in_shallow_water() {
        let deep_rl = reverberation_db(230.0, 70.0, 10_000.0, 3, Some(3000.0));
        let shallow_rl = reverberation_db(230.0, 70.0, 10_000.0, 3, Some(80.0));
        assert!(shallow_rl > deep_rl);
        // Hand: 230 − 140 + (−50 + 9) + 10 log(10000 × 0.2 × 75) = 49 + 61.76.
        assert!((deep_rl - (49.0 + 10.0 * libm::log10(150_000.0))).abs() < 1e-9);
    }

    #[test]
    fn months_from_unix_time() {
        assert_eq!(month_of(0), 1);
        assert_eq!(month_of(1_794_268_800), 11); // 2026-11-11
        assert_eq!(month_of(1_772_323_200), 3); // 2026-03-01
        assert_eq!(month_of(-86_400), 12);
    }
}
