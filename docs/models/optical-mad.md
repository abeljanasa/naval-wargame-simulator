# Model: Visual, EO/IR and MAD

> Phase 2 slice 6. Original-design range laws on top of textbook solar geometry; generic values; **not SME-validated**.

- **Code:** `crates/sim-sensors/src/optical.rs` (model), `crates/sim-core/src/optical.rs` (pass of the detection system)
- **Tests:** unit tests in `sim-sensors` (hand values); engine tests in `crates/sim-core/tests/detection.rs` (visual, EO/IR and MAD section)
- **Design log:** [DL-0015](../design-log/DL-0015-optical-mad.md)

## Daylight

The scenario environment carries a start time (`start_unix_s`, UTC) and a visibility (`visibility_km`, default 40 km). At game time t, the sun's elevation at the observer is:

sin el = sin φ sin δ + cos φ cos δ cos H

- δ = −23.44° · cos(360°/365 · (N + 10)), where N is the day of the year
- H = 15° × (UTC hours + longitude/15 − 12): mean solar time, no equation of time

This is accurate to about a degree. **Day** is el > 0°, **twilight** 0 to −6°, **night** below −6°.

## Visual, EO and IR detection

For each sensor and target in range and within the line-of-sight horizon (the same 4/3-earth horizon as radar, from the mount height to the target height):

R₅₀ = min( 0.7 × R_max × size × light , visibility )

- **Size:** √(length / 100 m), at most 1. Aircraft count as full size to IR (hot engines). Lengths come from the platform database, else 100 m surface, 70 m submarine, 15 m air, 10 m land.
- **Light:**

  | Sensor | Day | Twilight | Night |
  |---|---|---|---|
  | Visual | 1 | 0.5 | 0.1 |
  | EO/IR director (falls back to its IR channel) | 1 | 0.75 | 0.75 |
  | IR (IRST) | 1 | 1 | 1 |

- **Visibility:** IR (including an EO director out of daylight) sees 1.5× through haze.
- **Pd** = 1 / (1 + (r / R₅₀)⁸), one `Sensors` draw.
- Submerged targets cannot be seen. A submarine's periscope works only when surfaced (periscope depth is not modelled).

## Identification by range

A detection identifies as far as the range allows:

| Range | Level |
|---|---|
| ≤ 0.3 R₅₀ | **class**, from the target's platform (which reveals affiliation, [c2-doctrine.md](c2-doctrine.md)) |
| ≤ 0.6 R₅₀ | **category** |
| further | **domain** |

- **Visual** gives range (σ 300 m) and bearing (σ 1°): a position measurement fused into the track.
- **EO and IR** give a precise bearing (σ 0.2°), pooled with ESM and sonar bearings ([esm.md](esm.md)). Their identification raises an existing track, or travels with a fix.

## MAD

A magnetic anomaly detector on an aircraft detects a **submarine** (submerged or not) by slant range s = √(horizontal² + Δheight²):
- R₅₀ = 0.6 × R_max (1.5 km max gives 900 m).
- Pd = 1 / (1 + (s/R₅₀)⁸).
- A detection marks "about here": the submarine's position with a 300 m isotropic error, classified to domain.

## Logged

`OpticalAttempt` (range, horizon, R₅₀, light, Pd, draw, outcome, identification level, errors) and `MadAttempt` (slant range, R₅₀, Pd, draw, outcome).

## Worked examples

| Case | Working | Result |
|---|---|---|
| Lookout (30 km), 100 m ship, noon, clear | 0.7 × 30 × 1 × 1 | R₅₀ = **21 km**; class within 6.3 km, category within 12.6 km |
| Same at midnight | × 0.1 | R₅₀ = **2.1 km** |
| Same in 5 km haze | min(21, 5) | R₅₀ = **5 km** |
| 25 m patrol boat by day | √0.25 = 0.5 | R₅₀ = **10.5 km** |
| IRST (80 km) against an aircraft at night | 0.7 × 80 × 1 × 1 | R₅₀ = **56 km** |
| MAD: aircraft at 150 m, submarine at −100 m, 600 m off | s = √(600² + 250²) = 650 m; R₅₀ 900 m | Pd = 1/(1 + 0.722⁸) = **0.93** |
| Demo start, 2026-11-10 00:00 UTC, Bass Strait | sun at 53.5° | day: lookouts identify the other ship as they pass |

## Verification

| Test | Checks |
|---|---|
| `sun_elevation_hand_values` | 53.5° in Bass Strait at 00Z; ±72° at the equator at midnight and noon; night at 12Z |
| `optical_r50_hand_values` | 21 km, 2.1 km, 56 km, haze caps 5 and 7.5 km, size factors |
| `identification_bands_and_mad` | class, category, domain; MAD R₅₀ 900 m; slant 650 m |
| `a_lookout_sees_far_by_day_short_at_night_and_is_capped_by_haze` | engine: light codes and R₅₀ |
| `a_close_look_identifies_the_class_and_reveals_affiliation` | engine: class, category, affiliation; domain only at 15 km |
| `eo_bearing_identifies_an_existing_radar_track` | engine: radar track raised to class by an EO bearing |
| `mad_finds_a_submerged_submarine_that_the_eye_cannot` | engine: slant 650 m, Pd by hand; no optical attempt on a submerged target |

## Limitations

- One visibility and one start time for the whole scenario; no cloud, moon phase or weather zones (Phase 4).
- Identification by range alone: no aspect, contrast or crew training.
- EO directors have no laser rangefinder in the model (bearing only).
- No periscope depth; no MAD noise from geology or own aircraft.
