# Model: ESM: intercept, bearings, fixes and emitter identification

> Phase 2 slice 4. Textbook passive-detection geometry with original-design simplifications and generic values; **not SME-validated**.

- **Code:** `crates/sim-sensors/src/esm.rs` (model), `crates/sim-core/src/esm.rs` (ESM pass of the detection system)
- **Tests:** unit tests in `sim-sensors` (hand values); engine tests in `crates/sim-core/tests/detection.rs` (ESM section)
- **Design log:** [DL-0013](../design-log/DL-0013-esm.md)

## Emitters

Every 5 s, after the radar pass, every unit whose EMCON is active radiates:
- each of its radars, from its mount height
- its noise jammer, if it has one, from the unit's height

A unit under EMCON silent radiates nothing and cannot be intercepted.

## Intercept

An ESM sensor on another side considers an emitter that is within its maximum range and within the radar horizon between the ESM mount height and the emitter's antenna height.

- **Intercept range** (Pd = 0.5): R_int = min(2.5 × R_ref, R_ESM,max), where R_ref is the radar's reference detection range. A jammer is heard out to R_ESM,max.
  - The factor 2.5 is original design. ESM hears the radar one-way (falling as 1/r²), while the radar needs its echo back two-way (1/r⁴), so ESM normally outranges the radar it intercepts.
- **Pd** = 1 / (1 + (r / R_int)⁸): the same curve as radar, with one `Sensors` draw.
- **Bearing**: a detection draws a normal error n·σ, with σ = 2°. The measured bearing is logged with the draw (`EsmIntercept`).

Each side keeps the latest bearing from each observer on each emitting unit, and shows it for 60 s.

## Fixes (triangulation)

When two or more observers of a side intercept the same unit in the same scan, their bearings are intersected by weighted least squares in a local east/north plane:

minimise Σ wᵢ (nᵢ · (x − pᵢ))², with wᵢ = 1 / (dᵢ σᵢ)²

Here nᵢ is the line's normal and dᵢ the distance from observer i to the fix; a bearing error of σ moves the line sideways by d·σ. The normal equations are solved twice, first with equal weights to find dᵢ. The fix covariance is the inverse of the normal matrix Σ w n nᵀ. There is no fix if:
- no two bearings cross by between 10° and 170°, or
- the solution lies behind any observer.

A fix is a position measurement, fused into the side's track like a radar detection ([tracks.md](tracks.md)). It can create a track for a target no radar has seen (`EsmFix`, `ContactGained`).

## Identification

The emitter library, built from the platform database by the scenario loader (`SetEmitterLibrary`), lists every class that carries each radar type in the scenario. An intercept identifies its emitter type, and the ladder ([tracks.md](tracks.md)) climbs:

| Carriers of that emitter type | Result |
|---|---|
| exactly one class | **class** (and its category) |
| several classes, one category | **category** |
| several categories, one domain | **domain** |
| mixed domains, not in the library, or a jammer | nothing |

A fix carries its identification into the track. A single bearing cannot place a target, but it raises the classification of a track the side already holds. Identification to class also reveals affiliation ([c2-doctrine.md](c2-doctrine.md)), which weapons control status uses.

## What the player sees

- Bearing lines, drawn 150 km from the intercepting ship along the measured bearing and fading over 60 s, labelled with the emitter type.
- Contacts named by what is known: class, else category, else domain, else unknown.

## Worked examples

| Case | Working | Result |
|---|---|---|
| 3D radar, R_ref 120 km, ship ESM (400 km) | 2.5 × 120 | R_int = **300 km** (horizon-limited at sea: 41.2 km between 25 m masts) |
| Navigation radar, R_ref 15 km | 2.5 × 15 | R_int = **37.5 km** |
| Two observers 10 km apart, bearings 045° and 315°, σ = 2° | lines cross at 90°; d = 7.07 km each; σ_perp = 7071 × 0.0349 | fix σ = **247 m** on both axes |
| Engine scene: observers 20 km apart, target 26.9 km from each | crossing 43.6°; σ_perp ≈ 939 m | fix within about 1 km; identified to class if its radar type is unique |

## Verification

| Test | Checks |
|---|---|
| `intercept_range_hand_values` | 300 km, cap at 150 km, 37.5 km, jammer at full range |
| `two_perpendicular_bearings_fix_hand_value` | fix within 1 m of truth; σ 246.8 m on both axes |
| `no_fix_from_parallel_single_or_diverging_bearings` | one line, 5° crossing, fix behind the observers |
| `identification_ladder` | class, category, domain, unknown, empty |
| `two_bearings_fix_an_emitter_and_a_unique_emitter_identifies_its_class` | engine: `EsmFix`, track at class level, affiliation known, within 4σ |
| `a_single_bearing_shows_a_line_but_makes_no_track` | engine: line shown, no track; lines expire; a silent emitter is never intercepted |
| `esm_raises_a_radar_track_to_category` | engine: radar track raised to category; no affiliation below class |

## Limitations

- No frequency or band matching between ESM receivers and emitters, no scan coincidence, no pulse analysis: an emitter type is identified perfectly when intercepted.
- Fixes need bearings from the same scan; there is no bearings-only tracking of a moving emitter from a single platform.
- The emitter library holds only the radar types fitted in the scenario.
- One generic bearing accuracy (2°) for every ESM set; per-sensor accuracy is a later database field.
