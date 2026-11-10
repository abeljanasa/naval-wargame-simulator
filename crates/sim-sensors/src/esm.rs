//! ESM: passive intercept of radiating emitters, bearing-line triangulation and emitter
//! identification (`docs/models/esm.md`).

use crate::classify::Classification;
use crate::track::{from_local, to_local, Cov2};
use serde::{Deserialize, Serialize};
use sim_geo::GeoPoint;

/// An ESM receiver intercepts a radar out to this multiple of the radar's own reference
/// detection range (one-way against two-way propagation; original design).
pub const ESM_RANGE_FACTOR: f64 = 2.5;
/// Bearing lines must cross at least this steeply (and at most 180° minus it) to give a fix.
pub const MIN_CROSSING_DEG: f64 = 10.0;

/// The intercept range (Pd 0.5) of an ESM receiver against an emitter. A radar is given by its
/// reference detection range; a noise jammer (`None`) is loud enough to be heard out to the
/// receiver's maximum range.
#[must_use]
pub fn intercept_range_m(emitter_ref_range_m: Option<f64>, esm_max_range_m: f64) -> f64 {
    emitter_ref_range_m.map_or(esm_max_range_m, |r| {
        (ESM_RANGE_FACTOR * r).min(esm_max_range_m)
    })
}

/// A measured bearing from an observer.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct BearingLine {
    pub from: GeoPoint,
    /// Measured bearing, degrees clockwise from north.
    pub bearing_deg: f64,
    /// One-sigma bearing error, degrees.
    pub sigma_deg: f64,
}

fn crossing_ok(a: f64, b: f64) -> bool {
    let d = (a - b).abs() % 180.0;
    (MIN_CROSSING_DEG..=180.0 - MIN_CROSSING_DEG).contains(&d)
}

/// A line in the local plane: (point, direction, normal, σ radians).
type LineGeom = ([f64; 2], [f64; 2], [f64; 2], f64);

/// Weighted least-squares intersection of two or more bearing lines.
///
/// Each line i, through p_i with unit normal n_i, contributes w_i (n_i · (x − p_i))² with
/// w_i = 1 / (d_i σ_i)², where d_i is the distance from the observer to the fix: a bearing error
/// moves the line sideways by d·σ. The normal equations A x = b with A = Σ w n nᵀ and
/// b = Σ w n (n·p) are solved twice: first with equal weights to find the d_i, then weighted.
/// The fix covariance is A⁻¹.
///
/// Returns `None` if fewer than two lines, no pair crosses by at least [`MIN_CROSSING_DEG`], or
/// the fix lies behind any observer.
#[must_use]
pub fn triangulate(lines: &[BearingLine]) -> Option<(GeoPoint, Cov2)> {
    if lines.len() < 2 {
        return None;
    }
    let crosses = lines.iter().enumerate().any(|(i, a)| {
        lines[i + 1..]
            .iter()
            .any(|b| crossing_ok(a.bearing_deg, b.bearing_deg))
    });
    if !crosses {
        return None;
    }
    let origin = lines[0].from;
    let geom: Vec<LineGeom> = lines
        .iter()
        .map(|l| {
            let b = l.bearing_deg.to_radians();
            let (s, c) = (libm::sin(b), libm::cos(b));
            (
                to_local(origin, l.from),
                [s, c],
                [c, -s],
                l.sigma_deg.to_radians(),
            )
        })
        .collect();
    let solve = |weights: &[f64]| -> Option<([f64; 2], [[f64; 2]; 2])> {
        let (mut a, mut b) = ([[0.0; 2]; 2], [0.0; 2]);
        for ((p, _, n, _), w) in geom.iter().zip(weights) {
            let np = n[0] * p[0] + n[1] * p[1];
            for r in 0..2 {
                b[r] += w * n[r] * np;
                for c in 0..2 {
                    a[r][c] += w * n[r] * n[c];
                }
            }
        }
        let det = a[0][0] * a[1][1] - a[0][1] * a[1][0];
        if !(det.is_finite() && det > 0.0) {
            return None;
        }
        let inv = [
            [a[1][1] / det, -a[0][1] / det],
            [-a[1][0] / det, a[0][0] / det],
        ];
        let x = [
            inv[0][0] * b[0] + inv[0][1] * b[1],
            inv[1][0] * b[0] + inv[1][1] * b[1],
        ];
        Some((x, inv))
    };
    let (x0, _) = solve(&vec![1.0; geom.len()])?;
    let weights: Vec<f64> = geom
        .iter()
        .map(|(p, _, _, sigma)| {
            let d = libm::hypot(x0[0] - p[0], x0[1] - p[1]).max(100.0);
            1.0 / ((d * sigma) * (d * sigma))
        })
        .collect();
    let (x, cov) = solve(&weights)?;
    // The fix must be in front of every observer.
    let in_front = geom
        .iter()
        .all(|(p, u, _, _)| u[0] * (x[0] - p[0]) + u[1] * (x[1] - p[1]) > 0.0);
    if !in_front {
        return None;
    }
    Some((
        from_local(origin, x),
        Cov2 {
            ee: cov[0][0],
            en: 0.5 * (cov[0][1] + cov[1][0]),
            nn: cov[1][1],
        },
    ))
}

/// A platform class that carries an emitter, from the platform database.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Carrier {
    pub platform: String,
    pub category: String,
    pub domain: String,
}

/// What an intercepted emitter type says about its carrier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identification {
    pub classification: Classification,
    /// Present at class level.
    pub platform: Option<String>,
    /// Present at category level and above.
    pub category: Option<String>,
}

/// Identifies an emitter's carrier from the classes known to carry it: one class gives the class;
/// a shared category gives the category; a shared domain gives the domain; otherwise unknown.
/// An emitter not in the library (or a jammer) gives nothing.
#[must_use]
pub fn identify(carriers: &[Carrier]) -> Identification {
    let none = Identification {
        classification: Classification::Unknown,
        platform: None,
        category: None,
    };
    let Some(first) = carriers.first() else {
        return none;
    };
    if carriers.iter().all(|c| c.platform == first.platform) {
        return Identification {
            classification: Classification::Class,
            platform: Some(first.platform.clone()),
            category: Some(first.category.clone()),
        };
    }
    if carriers.iter().all(|c| c.category == first.category) {
        return Identification {
            classification: Classification::Category,
            platform: None,
            category: Some(first.category.clone()),
        };
    }
    if carriers.iter().all(|c| c.domain == first.domain) {
        return Identification {
            classification: Classification::Domain,
            platform: None,
            category: None,
        };
    }
    none
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim_geo::{direct, inverse};

    #[test]
    fn intercept_range_hand_values() {
        // A 120 km radar is intercepted at 300 km by a 400 km receiver, capped by a 150 km one.
        assert_eq!(intercept_range_m(Some(120_000.0), 400_000.0), 300_000.0);
        assert_eq!(intercept_range_m(Some(120_000.0), 150_000.0), 150_000.0);
        // A navigation radar (15 km) at 37.5 km; a jammer at the receiver's full range.
        assert_eq!(intercept_range_m(Some(15_000.0), 400_000.0), 37_500.0);
        assert_eq!(intercept_range_m(None, 400_000.0), 400_000.0);
    }

    #[test]
    fn two_perpendicular_bearings_fix_hand_value() {
        // Observers 10 km apart on an east–west line; bearings 045° and 315° cross 5 km north of
        // the midpoint, 7071 m from each. σ = 2°: each line is ±7071 × 0.0349 = 246.8 m sideways,
        // and the lines are perpendicular, so the fix covariance is 246.8² m² on both axes.
        let a = GeoPoint::new(0.0, 0.0);
        let b = direct(a, 90.0, 10_000.0).point;
        let target = direct(direct(a, 90.0, 5_000.0).point, 0.0, 5_000.0).point;
        let la = BearingLine {
            from: a,
            bearing_deg: inverse(a, target).azimuth1_deg,
            sigma_deg: 2.0,
        };
        let lb = BearingLine {
            from: b,
            bearing_deg: inverse(b, target).azimuth1_deg,
            sigma_deg: 2.0,
        };
        let (fix, cov) = triangulate(&[la, lb]).unwrap();
        // The local plane is flat; at 7 km the error from that is centimetres.
        assert!(inverse(fix, target).distance_m < 1.0, "{fix:?}");
        let sigma = 7_071.07 * 2.0_f64.to_radians();
        let e = cov.ellipse();
        assert!((e.semi_major_m - sigma).abs() < 2.0, "{e:?} vs {sigma}");
        assert!((e.semi_minor_m - sigma).abs() < 2.0, "{e:?} vs {sigma}");
    }

    #[test]
    fn no_fix_from_parallel_single_or_diverging_bearings() {
        let a = GeoPoint::new(0.0, 0.0);
        let b = direct(a, 90.0, 10_000.0).point;
        let line = |from, bearing_deg| BearingLine {
            from,
            bearing_deg,
            sigma_deg: 2.0,
        };
        assert!(triangulate(&[line(a, 45.0)]).is_none());
        // 5° apart: below the 10° crossing minimum.
        assert!(triangulate(&[line(a, 0.0), line(b, 355.0)]).is_none());
        // These lines meet south of the baseline, behind both observers.
        assert!(triangulate(&[line(a, 315.0), line(b, 45.0)]).is_none());
    }

    #[test]
    fn identification_ladder() {
        let c = |p: &str, cat: &str, dom: &str| Carrier {
            platform: p.into(),
            category: cat.into(),
            domain: dom.into(),
        };
        let one = identify(&[c("GEN-FFG", "surface_combatant", "surface")]);
        assert_eq!(one.classification, Classification::Class);
        assert_eq!(one.platform.as_deref(), Some("GEN-FFG"));
        let cat = identify(&[
            c("GEN-FFG", "surface_combatant", "surface"),
            c("GEN-DDG", "surface_combatant", "surface"),
        ]);
        assert_eq!(cat.classification, Classification::Category);
        assert_eq!(cat.category.as_deref(), Some("surface_combatant"));
        let dom = identify(&[
            c("GEN-FFG", "surface_combatant", "surface"),
            c("GEN-MV-BULK", "merchant", "surface"),
        ]);
        assert_eq!(dom.classification, Classification::Domain);
        let mixed = identify(&[
            c("GEN-FFG", "surface_combatant", "surface"),
            c("GEN-SITE", "sensor_site", "land"),
        ]);
        assert_eq!(mixed.classification, Classification::Unknown);
        assert_eq!(identify(&[]).classification, Classification::Unknown);
    }
}
