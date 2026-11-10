# Model: Gate 1 interim radar detection and contacts

> **Interim model.** It exists so Gate 1 ("units move and detect") can be demonstrated with platform-database sensors. Phase 2 replaces it with the full sensor suite (radar clutter and jamming, ESM, sonar, IR/EO, visual, MAD) in a `sim-sensors` crate. Treat its outputs as illustrative, not analytical.

- **Code:** `crates/sim-core/src/detection.rs`
- **Tests:** `crates/sim-core/tests/detection.rs` (duel tests), plus unit tests in the module
- **Design log:** [DL-0006](../design-log/DL-0006-gate1-radar-detection.md)

## Cadence and order

The model runs on ticks where `tick % 5 == 0`, after movement. It loops over observers in `UnitId` order, their radars in mount order, and targets of every other side in `UnitId` order. That fixes the order of random draws (ADR-0003).

## Per (observer radar, target) pair

1. **Geometry gates.** None of these cases consumes a random draw.
   - The target is submerged (subsurface with altitude < 0). Radar cannot see it.
   - The observer is a submerged submarine. Its radar is unusable.
   - The target's altitude is above the radar's `max_target_altitude_m`.
   - The range r (WGS-84 geodesic) lies outside [`min_range_m`, `max_range_m`].
   - r > the radar horizon d_h = 4.12 (√h_s + √h_t) km ([radar-horizon.md](radar-horizon.md)).
     - h_s = mount height, plus altitude for aircraft.
     - h_t = 10 m for surface ships, the altitude for aircraft, 5 m for land targets, and 2 m for a surfaced submarine.
2. **R₅₀, the range for 50 % single-scan detection:**

   R₅₀ = min(R_ref · (σ / σ_ref)^¼, R_max)

   R_ref and σ_ref come from the radar's database reference (`ref_detect_range_m` at `ref_target_rcs_m2`). σ is the target's RCS from its platform signature. Without one, a domain default applies: 1000 m² surface, 100 m² subsurface or land, 5 m² air.

   The fourth-root scaling is the radar-range equation at a fixed detection threshold.
3. **Single-scan probability of detection:**

   Pd = 1 / (1 + (r / R₅₀)⁸)

   This is a smooth, monotonic curve that equals 0.5 at R₅₀ and falls from about 0.86 to about 0.19 between 0.8 R₅₀ and 1.2 R₅₀. Choosing it is a modelling assumption. It stands in for a target-fluctuation model (Swerling cases), which arrives in Phase 2.
4. **Draw.** One draw u ~ U[0, 1) from the `Sensors` RNG stream; the target is detected if u < Pd. Every draw is logged as a `DetectionAttempt` event with observer, sensor, target, r, d_h, R₅₀, Pd, u and the outcome.

## Contacts

- **Gained:** a detection creates a contact in the observer side's picture, numbered C1, C2, … per side, and logs `ContactGained`. Later detections refresh its position and last-seen time.
- **Lost:** a contact not re-detected for 300 s is dropped and `ContactLost` is logged.
- **Fog of war:** side snapshots carry contacts (number, position, times, detecting own unit, sensor) but **never the target's unit id**. Only the white cell sees ground truth.
- **Stored but hidden:** the engine keeps the target id internally, for adjudication and AAR.

## Worked example (the Phase 1 demo scenario)

The generic frigate (GEN-FFG) uses its medium-range 3D radar against the generic bulk carrier (GEN-MV-BULK, σ = 40 000 m²).

| Quantity | Value |
|---|---|
| Mount height h_s | 25 m |
| Target height h_t (surface) | 10 m |
| Horizon d_h | 4.12 (√25 + √10) = 4.12 × 8.162 = **33.63 km** |
| R_ref at σ_ref | 120 km at 1 m² |
| R₅₀ before the cap | 120 × (40 000)^¼ = 120 × 14.14 = 1697 km |
| R₅₀ after the cap | min(1697, 250) = **250 km** |
| Pd at the horizon, r = 33.6 km | 1 / (1 + (33.6/250)⁸) ≈ **1.000** |

So in this scenario the horizon, not the radar's power, sets detection range: the frigate detects the merchant almost exactly when the merchant crosses 33.6 km.

The merchant's navigation radar (30 m mount, R_ref 15 km at 100 m²) against the frigate (σ = 5000 m²) works out as:

- R₅₀ = 15 × (50)^¼ = 39.9 km.
- Horizon = 4.12 (√30 + √10) = 35.6 km.
- So the merchant gains its contact at about 35.6 km with Pd ≈ 0.69 per scan, and almost certainly within the first few scans.

## Verification (duel tests)

A stationary observer faces a stationary air target, so the horizon is not the limit. Each test checks that every logged Pd equals the hand value to 10⁻⁶, and that the number of detections falls within 4 standard deviations of the binomial mean.

| r / R₅₀ | Hand Pd | Scans | Test |
|---|---|---|---|
| 0.8 | 0.85633 | 2000 | `duel_at_eight_tenths_r50_matches_hand_value` |
| 1.0 | 0.50000 | 4000 | `duel_at_r50_detects_half_the_time` |
| 1.2 | 0.18869 | 2000 | `duel_at_one_point_two_r50_matches_hand_value` |

Further tests cover:
- the horizon cut-off (a surface target at 40 km gets no draws; at 30 km it is detected)
- submerged targets
- contact gain and loss, with timing to the tick
- the fog-of-war stream check
- save and resume with live contacts
- determinism per seed

## Limitations

These are known and deliberate:
- Radar only. ESM, sonar, visual, EO/IR and MAD are ignored even when fitted.
- No clutter, multipath, jamming, weather, terrain masking, aspect-dependent RCS or EMCON.
- No position error, classification ladder, track correlation or data-link sharing.
- Contact positions are exact.
- Scans are independent draws every 5 s regardless of each radar's own scan period.
