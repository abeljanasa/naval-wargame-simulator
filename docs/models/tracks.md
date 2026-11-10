# Model: Tracks: measurement error, fusion, error growth, classification

> Phase 2 slice 1. Original design built from textbook tracking (static Kalman update, random-walk growth). The numeric defaults are generic and **not SME-validated**; they are listed under Assumptions.

- **Code:** `crates/sim-sensors/src/{measurement,track,classify}.rs` (models), `crates/sim-core/src/detection.rs` (wiring)
- **Tests:** unit tests in `sim-sensors` (hand values), engine tests in `crates/sim-core/tests/detection.rs`
- **Design log:** [DL-0010](../design-log/DL-0010-tracks.md); architecture in [ADR-0005](../adr/0005-model-crates.md)

## Overview

Each side holds one track per target it has detected (C1, C2, …). A track has an **estimated position**, a **position covariance** P (2×2, metres², in a local east/north plane), a **classification** and, once classified to that level, the target's **domain**. Players see the estimate and its error ellipse, never the truth.

## Measurement

A sensor that detects a target reports a position with range and bearing error.

1. Two more uniform draws from the `Sensors` stream (range first, then bearing), taken only when the detection succeeds, are turned into two standard normals n_r, n_θ by the Box–Muller transform:

   n_r = √(−2 ln(1 − u₁)) · cos(2π u₂), n_θ = √(−2 ln(1 − u₁)) · sin(2π u₂)

2. The reported point is the true range and azimuth from the observer, perturbed:

   r_m = r + n_r σ_r, θ_m = θ + n_θ σ_θ, z = direct(observer, θ_m, r_m) on WGS-84.

3. Its covariance R has σ_r along the line of sight and r · σ_θ (radians) across it. For a line of sight at bearing β (clockwise from north):

   R_ee = σ_r² sin²β + σ_c² cos²β, R_nn = σ_r² cos²β + σ_c² sin²β, R_en = (σ_r² − σ_c²) sin β cos β, with σ_c = r σ_θ.

The errors drawn (metres, degrees) are logged on the `DetectionAttempt` event.

## Error growth

Between updates the track does not move (there is no velocity estimate yet). Instead the target may have gone anywhere within reach v·Δt, which is added as a one-sigma isotropic spread:

P(t) = P(t_last) + (v Δt)² I

v is a speed bound for what the side knows (`domain_speed_bound_mps`): 18 m/s (35 kn) at sea or below it, 30 m/s on land, and 600 m/s for air or an unclassified track.

## Fusion

When a track is detected again, its predicted estimate (x, P) is fused with the measurement (z, R) by a Kalman update with an identity measurement matrix, computed in the east/north plane centred on x:

K = P (P + R)⁻¹, x' = x + K (z − x), P' = (I − K) P

Every sensor of the side updates the same track (the side's picture is fused). Association is perfect: the track is keyed by the true target, so mis-association cannot happen yet.

## Classification ladder

unknown → domain → category → class → identified. A track only climbs. A radar detection supports **domain** (height and position separate surface, air and land returns). Category and above need ESM, visual or IR/EO identification, which arrive in later slices.

## What the player sees

- The estimated position and the **95 % containment ellipse**: the one-sigma ellipse scaled by √χ²₂(0.95) = 2.4477, grown to the snapshot time.
- The domain once known, which sets the symbol frame. Affiliation stays "unknown" until postures exist (slice 2).

## Worked example

The frigate's radar (σ_r = 50 m, σ_θ = 0.5°) detects a stationary ship 20 km due east (β = 90°).

| Step | Along line of sight (east) | Across (north) |
|---|---|---|
| First measurement, σ | 50 m | 20 000 × 0.5° × π/180 = **174.5 m** |
| Growth over 5 s at 18 m/s, added σ | 90 m | 90 m |
| Predicted before the 2nd scan, σ | √(50² + 90²) = 103.0 m | √(174.5² + 90²) = 196.4 m |
| After fusing the 2nd measurement | √(103.0² · 50² / (103.0² + 50²)) = **45.0 m** | √(196.4² · 174.5² / (196.4² + 174.5²)) = **130.5 m** |
| Steady state, scan every 5 s | 44.8 m | **110.3 m** |

The steady state solves x = (x + g) R / (x + g + R) with g = 90² m². The cross-range error settles about a third below a single measurement, because growth between scans limits how much old measurements count.

Shown to the player 1 s after a scan, the steady-state cross-range axis is √(110.3² + 18²) × 2.4477 = **273 m** at 95 %.

## Assumptions (generic defaults, pending SME review)

| Sensor kind | σ range | σ bearing |
|---|---|---|
| Radar | 50 m | 0.5° |
| Hull or dipping sonar (active) | 200 m | 2° |
| Towed array, sonobuoy, ESM | bearing only | 2° |
| IR / EO | bearing only | 0.2° |
| Visual | 300 m | 1° |
| MAD | 100 m | 30° |

Only radar is used in this slice; the other rows take effect with their detection models, and bearing-only sensors need their own fusion (triangulation), which is not built yet. The values move to per-sensor database fields in a later migration.

## Verification

| Test | Checks |
|---|---|
| `polar_covariance_hand_values` | R at 0°, 45°, 90° against the formulas |
| `ellipse_recovers_axes_and_orientation` | eigen-decomposition gives back σ and β |
| `fusion_hand_values` | P = 4, R = 1, 10 m offset: x moves 8 m, P' = 0.8 |
| `growth_adds_reach_squared` | 100 m² + (10 × 5)² = 2600 m² |
| `box_muller_hand_values`, `box_muller_moments` | the transform's exact points and its mean and variance |
| `first_detection_reports_the_single_measurement_error` | engine: 174.5 m by 50 m, grown 1 s, north–south major axis, estimate within 4σ of truth |
| `repeated_detections_shrink_the_track_error` | engine: 31 scans give a smaller error than one |
| `track_error_grows_with_time_since_update` | √(100² + 1800²) = 1802.8 m after 100 s |
| `measurement_errors_are_logged_only_for_detections` | draws are logged with their inputs |

## Limitations

- No velocity estimate: the estimate stays at the last fix while the error grows to cover the target's possible movement.
- Perfect association (keyed by the true target); no false tracks or track swaps.
- One fused picture per side with instant sharing; data links and their delays come with C2.
- The growth bound is the worst case for the domain, so error ellipses on slow targets are pessimistic.
- Bearing-only fusion and triangulation are not built.
