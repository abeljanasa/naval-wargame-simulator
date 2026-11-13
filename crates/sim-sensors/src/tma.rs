//! Bearings-only target motion analysis (TMA) from one observer (`docs/models/tma.md`).
//!
//! A single passive sensor gives bearings, not positions. Over several minutes, if the observer
//! manoeuvres, the bearings fix the target's position and velocity. The model is the textbook
//! maximum-likelihood estimate: the target moves at constant velocity, p(t) = p + v·t, with p its
//! position now, and the four unknowns x = (pₑ, pₙ, vₑ, vₙ) minimise
//!
//! Σᵢ ((βᵢ − β̂ᵢ(x)) / σᵢ)²,   β̂ᵢ(x) = atan2(eᵢ, nᵢ), (eᵢ, nᵢ) = p + tᵢ v − oᵢ,
//!
//! over the bearings βᵢ taken at times tᵢ ≤ 0 from oᵢ, in the local east/north plane. It is
//! solved by Levenberg–Marquardt from several starting ranges along the newest bearing (1 to
//! 64 km, at rest), keeping the lowest cost. The covariance of p is the top-left 2 × 2 block of
//! (JᵀWJ)⁻¹, J the gradient of β̂ and W = diag(1/σᵢ²).
//!
//! Bearing residuals are angles, so a solution close to the observer is not favoured; the
//! simpler pseudo-linear estimator, which minimises cross-range distances, collapses toward the
//! observer with real bearing noise and was rejected for that reason (DL-0050).
//!
//! With an observer on a straight leg at constant speed the range is unobservable (JᵀWJ is
//! singular or near it), and the solution is refused. This is the real limitation of
//! single-platform passive ranging, and why submarines manoeuvre to range a contact.

use crate::esm::BearingLine;
use crate::track::{from_local, to_local, Cov2};
use sim_geo::GeoPoint;

/// Fewest bearings a solution needs.
pub const TMA_MIN_BEARINGS: usize = 8;
/// Shortest span of bearing times a solution needs, s.
pub const TMA_MIN_SPAN_S: f64 = 180.0;
/// Solutions faster than this (about 39 kn) are refused as implausible for a ship or submarine.
pub const TMA_MAX_SPEED_MPS: f64 = 20.0;
/// A solution whose one-sigma error exceeds this fraction of its range is refused: it says where
/// the bearing points, not how far.
pub const TMA_MAX_ERROR_FRACTION: f64 = 0.3;

/// One bearing for TMA: the line and how long ago it was taken.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TmaBearing {
    pub age_s: f64,
    pub line: BearingLine,
}

/// A TMA solution: the target's position now, its velocity, and the position covariance.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TmaSolution {
    pub position: GeoPoint,
    /// East and north, m/s.
    pub velocity_mps: [f64; 2],
    pub cov: Cov2,
    /// Range from the newest bearing's observer, m.
    pub range_m: f64,
}

type M4 = [[f64; 4]; 4];

/// Solves A x = b and returns x with A⁻¹ (Gauss–Jordan with partial pivoting). `None` if A is
/// singular to working precision.
fn solve4(a: M4, b: [f64; 4]) -> Option<([f64; 4], M4)> {
    let mut m = [[0.0; 9]; 4];
    let scale = a.iter().flatten().fold(0.0_f64, |s, x| s.max(x.abs()));
    if !(scale.is_finite() && scale > 0.0) {
        return None;
    }
    for r in 0..4 {
        m[r][..4].copy_from_slice(&a[r]);
        m[r][4 + r] = 1.0;
        m[r][8] = b[r];
    }
    for col in 0..4 {
        let pivot = (col..4).max_by(|&i, &j| m[i][col].abs().total_cmp(&m[j][col].abs()))?;
        if m[pivot][col].abs() <= 1e-12 * scale {
            return None;
        }
        m.swap(col, pivot);
        let p = m[col][col];
        for v in &mut m[col] {
            *v /= p;
        }
        let pivot_row = m[col];
        for (r, row) in m.iter_mut().enumerate() {
            if r != col {
                let f = row[col];
                if f != 0.0 {
                    for (v, pv) in row.iter_mut().zip(pivot_row) {
                        *v -= f * pv;
                    }
                }
            }
        }
    }
    let mut x = [0.0; 4];
    let mut inv = [[0.0; 4]; 4];
    for r in 0..4 {
        x[r] = m[r][8];
        inv[r].copy_from_slice(&m[r][4..8]);
    }
    Some((x, inv))
}

/// A bearing in the local plane: (time ≤ 0 s, observer, measured bearing radians, σ radians).
type Row = (f64, [f64; 2], f64, f64);

/// Largest accepted mean squared normalised residual (a 3σ RMS bearing misfit): above it the
/// constant-velocity model does not fit (the target manoeuvred) and the solution is refused.
pub const TMA_MAX_MEAN_SQ_RESIDUAL: f64 = 9.0;

/// The angle wrapped to (−π, π].
fn wrap(a: f64) -> f64 {
    let tau = std::f64::consts::TAU;
    let mut a = a % tau;
    if a > std::f64::consts::PI {
        a -= tau;
    } else if a <= -std::f64::consts::PI {
        a += tau;
    }
    a
}

/// Weighted squared residuals Σ (r/σ)², the normal matrix JᵀWJ and the gradient JᵀWr at `x`.
fn normal(rows: &[Row], x: &[f64; 4]) -> (f64, M4, [f64; 4]) {
    let (mut cost, mut a, mut g) = (0.0, [[0.0; 4]; 4], [0.0; 4]);
    for (t, o, beta, sigma) in rows {
        let de = x[0] + t * x[2] - o[0];
        let dn = x[1] + t * x[3] - o[1];
        let r2 = (de * de + dn * dn).max(1.0);
        // ∂β̂/∂e = n / r², ∂β̂/∂n = −e / r².
        let (ge, gn) = (dn / r2, -de / r2);
        let j = [ge, gn, t * ge, t * gn];
        let w = 1.0 / (sigma * sigma);
        let res = wrap(beta - libm::atan2(de, dn));
        cost += w * res * res;
        for i in 0..4 {
            g[i] += w * j[i] * res;
            for k in 0..4 {
                a[i][k] += w * j[i] * j[k];
            }
        }
    }
    (cost, a, g)
}

/// Levenberg–Marquardt from `x`: the converged state, its cost and JᵀWJ.
fn refine(rows: &[Row], mut x: [f64; 4]) -> ([f64; 4], f64, M4) {
    let (mut cost, mut a, mut g) = normal(rows, &x);
    let mut lambda = 1e-3;
    for _ in 0..60 {
        let mut damped = a;
        for (i, row) in damped.iter_mut().enumerate() {
            row[i] += lambda * a[i][i].max(1e-18);
        }
        let Some((step, _)) = solve4(damped, g) else {
            lambda *= 10.0;
            if lambda > 1e12 {
                break;
            }
            continue;
        };
        let trial = [
            x[0] + step[0],
            x[1] + step[1],
            x[2] + step[2],
            x[3] + step[3],
        ];
        let (c, a2, g2) = normal(rows, &trial);
        if c < cost {
            let gain = cost - c;
            (x, cost, a, g) = (trial, c, a2, g2);
            lambda = (lambda * 0.3).max(1e-12);
            if gain <= 1e-10 * cost.max(1e-12) {
                break;
            }
        } else {
            lambda *= 10.0;
            if lambda > 1e12 {
                break;
            }
        }
    }
    (x, cost, a)
}

/// The target's position now and velocity from one observer's bearings, or `None` when the
/// bearings are too few, too short in time, or do not fix the range (see the module notes).
#[must_use]
pub fn tma_solve(bearings: &[TmaBearing]) -> Option<TmaSolution> {
    if bearings.len() < TMA_MIN_BEARINGS {
        return None;
    }
    let newest = bearings.iter().min_by(|a, b| a.age_s.total_cmp(&b.age_s))?;
    let oldest = bearings.iter().map(|b| b.age_s).fold(f64::MIN, f64::max);
    if oldest - newest.age_s < TMA_MIN_SPAN_S {
        return None;
    }
    let origin = newest.line.from;
    let rows: Vec<Row> = bearings
        .iter()
        .map(|b| {
            (
                -b.age_s,
                to_local(origin, b.line.from),
                b.line.bearing_deg.to_radians(),
                b.line.sigma_deg.to_radians().max(1e-6),
            )
        })
        .collect();
    // Start at rest at several ranges along the newest bearing; keep the lowest cost.
    let (t_k, o_k, beta_k, _) = rows.iter().max_by(|a, b| a.0.total_cmp(&b.0)).copied()?;
    let u_k = [libm::sin(beta_k), libm::cos(beta_k)];
    let (x, cost, a) = [
        1_000.0, 2_000.0, 4_000.0, 8_000.0, 16_000.0, 32_000.0, 64_000.0,
    ]
    .iter()
    .map(|r| refine(&rows, [o_k[0] + r * u_k[0], o_k[1] + r * u_k[1], 0.0, 0.0]))
    .min_by(|a, b| a.1.total_cmp(&b.1))?;
    #[allow(clippy::cast_precision_loss)]
    let dof = (rows.len() - 4) as f64;
    if !(cost.is_finite() && cost / dof <= TMA_MAX_MEAN_SQ_RESIDUAL) {
        return None;
    }
    let (_, inv) = solve4(a, [0.0; 4])?;
    // In front of the newest observer, at a plausible speed, and tight enough to range.
    let rel = [x[0] + t_k * x[2] - o_k[0], x[1] + t_k * x[3] - o_k[1]];
    if u_k[0] * rel[0] + u_k[1] * rel[1] <= 0.0 {
        return None;
    }
    let range_m = libm::hypot(rel[0], rel[1]);
    if range_m < 100.0 || libm::hypot(x[2], x[3]) > TMA_MAX_SPEED_MPS {
        return None;
    }
    let cov = Cov2 {
        ee: inv[0][0],
        en: 0.5 * (inv[0][1] + inv[1][0]),
        nn: inv[1][1],
    };
    let e = cov.ellipse();
    if !e.semi_major_m.is_finite() || e.semi_major_m > TMA_MAX_ERROR_FRACTION * range_m {
        return None;
    }
    Some(TmaSolution {
        position: from_local(origin, [x[0], x[1]]),
        velocity_mps: [x[2], x[3]],
        cov,
        range_m,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim_geo::{direct, inverse};

    /// Bearings every 30 s, ending now, from an observer on `legs` (heading, speed, duration s)
    /// to a target at constant velocity.
    fn bearings(
        start: GeoPoint,
        legs: &[(f64, f64, f64)],
        target_start: GeoPoint,
        target_course: f64,
        target_speed: f64,
        sigma_deg: f64,
    ) -> Vec<TmaBearing> {
        let total: f64 = legs.iter().map(|l| l.2).sum();
        let mut out = Vec::new();
        let mut t = 0.0;
        while t <= total + 1e-9 {
            let (mut pos, mut left) = (start, t);
            for (h, v, d) in legs {
                let dt = left.min(*d);
                pos = direct(pos, *h, v * dt).point;
                left -= dt;
                if left <= 0.0 {
                    break;
                }
            }
            let tgt = direct(target_start, target_course, target_speed * t).point;
            out.push(TmaBearing {
                age_s: total - t,
                line: BearingLine {
                    from: pos,
                    bearing_deg: inverse(pos, tgt).azimuth1_deg,
                    sigma_deg,
                },
            });
            t += 30.0;
        }
        out
    }

    #[test]
    fn a_manoeuvring_observer_ranges_a_moving_target() {
        // Observer: 6 min north at 3 m/s, then 6 min east. Target 10 km east-north-east, making
        // 6 m/s due west. Exact bearings: the solution is the target's true position now (to the
        // flat-plane approximation, metres at 10 km) and its true velocity.
        let start = GeoPoint::new(20.0, 70.0);
        let target0 = direct(start, 70.0, 10_000.0).point;
        let b = bearings(
            start,
            &[(0.0, 3.0, 360.0), (90.0, 3.0, 360.0)],
            target0,
            270.0,
            6.0,
            1.0,
        );
        let s = tma_solve(&b).expect("observable after the turn");
        let truth = direct(target0, 270.0, 6.0 * 720.0).point;
        let err = inverse(s.position, truth).distance_m;
        assert!(err < 30.0, "position error {err:.1} m");
        assert!(
            (s.velocity_mps[0] + 6.0).abs() < 0.1,
            "{:?}",
            s.velocity_mps
        );
        assert!(s.velocity_mps[1].abs() < 0.1, "{:?}", s.velocity_mps);
    }

    #[test]
    fn a_straight_leg_does_not_range_a_moving_target() {
        // Observer on one leg at constant speed: the range is unobservable, so no solution.
        let start = GeoPoint::new(20.0, 70.0);
        let target0 = direct(start, 70.0, 10_000.0).point;
        let b = bearings(start, &[(0.0, 3.0, 720.0)], target0, 270.0, 6.0, 1.0);
        assert!(tma_solve(&b).is_none());
    }

    #[test]
    fn too_few_or_too_short_is_refused() {
        let start = GeoPoint::new(20.0, 70.0);
        let target0 = direct(start, 70.0, 10_000.0).point;
        let b = bearings(
            start,
            &[(0.0, 3.0, 90.0), (90.0, 3.0, 60.0)],
            target0,
            270.0,
            6.0,
            1.0,
        );
        assert!(b.len() < TMA_MIN_BEARINGS);
        assert!(tma_solve(&b).is_none());
    }

    #[test]
    fn solve4_inverts_by_hand() {
        // diag(2, 4, 5, 10): x = b / diag, inverse = 1 / diag.
        let a = [
            [2.0, 0.0, 0.0, 0.0],
            [0.0, 4.0, 0.0, 0.0],
            [0.0, 0.0, 5.0, 0.0],
            [0.0, 0.0, 0.0, 10.0],
        ];
        let (x, inv) = solve4(a, [2.0, 2.0, 1.0, 5.0]).unwrap();
        assert_eq!(x, [1.0, 0.5, 0.2, 0.5]);
        assert!((inv[1][1] - 0.25).abs() < 1e-15);
        assert!(solve4([[0.0; 4]; 4], [1.0; 4]).is_none());
    }
}
