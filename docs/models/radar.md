# Model: Radar (Phase 2): clutter, weather and noise jamming

> Phase 2 slice 3. Extends the Gate 1 radar model ([radar-detection-gate1.md](radar-detection-gate1.md)), which still supplies the geometry gates, R₅₀ from RCS and the Pd curve. Textbook radar relations with original-design simplifications and generic values; **not SME-validated**.

- **Code:** `crates/sim-sensors/src/radar.rs` (model), `crates/sim-core/src/detection.rs` (wiring)
- **Tests:** unit tests in `sim-sensors` (hand values); engine tests in `crates/sim-core/tests/detection.rs` (Phase 2 radar section)
- **Design log:** [DL-0012](../design-log/DL-0012-radar-clutter-jamming.md)
- **Data:** migration 0002 adds `clutter_db_per_sea_state` and `beamwidth_deg` to radars

## One rule for every loss

Each effect is a change in signal-to-noise ratio (SNR). From the radar range equation, detection range scales with SNR^¼, so a loss of L dB scales range by **10^(−L/40)**. This is the same rule as the Gate 1 RCS scaling (σ^¼).

R₅₀,eff = min( R_ref · (σ/σ_ref)^¼ · 10^(−(L_clutter + L_weather)/40), R_max )

The loss applies before the instrumented maximum range cap.

## Clutter

A target at or below **150 m** competes with surface clutter. Surface ships, surfaced submarines and low aircraft are seen against the sea; land units against land.

- Sea: L_clutter = c × sea state, where c is the radar's `clutter_db_per_sea_state` (default 1.5 dB).
- Land: treated as sea state 5 (original design).
- Above 150 m: no clutter loss.

Low aircraft are always treated as over the sea (no land-mask lookup yet).

## Weather

`weather_loss_db` is one scenario-wide two-way loss in dB, applied to every radar. It is a hook: Phase 4 weather zones replace it with rain and cloud by area and by band.

## Noise jamming

A unit can carry a noise jammer, specified by the jamming-to-noise ratio it puts into a radar's main beam at 1 km, J₁ (dB). It radiates while the unit's EMCON is active. Only jammers of other sides affect a radar, and only within the radar horizon between the two.

- One jammer at distance d (one-way propagation): J/N = 10^((J₁ − S)/10) / (d / 1 km)², where S = 0 in the main beam and **30 dB** in the sidelobes.
- Main beam: the jammer is the target itself (self-screening), or its bearing is within half the radar's `beamwidth_deg` (default 2°) of the target's bearing.
- J/N from all jammers is summed, in unit-id order.
- Jamming raises the noise floor by (1 + J/N), so range shrinks by (1 + J/N)^¼:

  x = (r / R₅₀,eff) · (1 + J/N)^¼, Pd = 1 / (1 + x⁸)

**Burn-through.** Signal grows as 1/r⁴ and self-screening jamming as 1/r², so the radar wins at short range. Pd returns to 0.5 where (r/R₅₀)⁴ (1 + J₁(1 km/r)²) = 1, which is a quadratic in u = r²:

u² + J₁·(1 km)²·u − R₅₀⁴ = 0 → r_bt = √u

When J₁ is large, r_bt ≈ R₅₀² / (1 km · √J₁).

## Logged per attempt

`DetectionAttempt` now also carries `clutter_db`, `weather_db` and the total linear `jn`. R₅₀ in the event is the effective value after losses.

## Worked examples

| Case | Working | Result |
|---|---|---|
| Sea state 3, c = 1.5 dB | L = 4.5 dB; × 10^(−4.5/40) | R₅₀ × **0.7718** |
| Sea state 4, low aircraft, R_ref 120 km at 1 m² | L = 6 dB; 120 km × 10^(−0.15) | R₅₀ = **84.95 km** |
| Self-screening 60 dB jammer at 60 km, R₅₀ 120 km | J/N = 10⁶/60² = 277.8; x = 0.5 × 278.8^¼ = 2.043 | Pd = **0.0033** (unjammed: 0.996) |
| Burn-through, 60 dB jammer, R₅₀ 50 km | u² + 10¹²·u − 6.25×10¹⁸ = 0 | r_bt = **2.50 km** |
| Stand-off 60 dB jammer 10 km away, 90° off the beam | 10^((60 − 30)/10) / 10² | J/N = **10** |

In the demo scenario (sea state 3), the frigate's radar against the bulk carrier is still capped at 250 km and horizon-limited. The merchant's navigation radar (c = 1 dB) loses 3 dB: R₅₀ goes from 39.9 km to 33.6 km, just inside its 35.6 km horizon, so it gains its contact a few kilometres later than before.

## Generic database values (confidence D, original design)

| Radar | Band | Clutter dB per sea state | Beamwidth ° |
|---|---|---|---|
| Long-range 2D air search | L | 2.0 | 3.0 |
| Medium-range 3D search | S | 1.5 | 2.0 |
| Multifunction phased array | S | 1.5 | 1.5 |
| Surface search | X | 1.0 | 1.2 |
| Navigation | X | 1.0 | 1.0 |
| Fire control | X | 1.5 | 1.0 |
| CIWS | Ku | 1.0 | 2.0 |
| Airborne early warning | S | 1.5 | 2.0 |
| Airborne maritime surveillance | X | 0.8 | 2.0 |
| Fighter fire control | X | 1.5 | 3.0 |
| Coastal surveillance | X | 1.0 | 0.8 |
| SAM engagement, long / medium range | C / X | 1.5 | 1.5 |
| Airfield surveillance | S | 2.0 | 1.5 |
| Submarine mast | X | 1.0 | 2.0 |

The pattern is that surface-search and maritime radars are built to work in clutter (lower figures), and long-wavelength air-search radars suffer more. All figures are illustrative.

## Verification

| Test | Checks |
|---|---|
| `clutter_and_weather_scale_range_by_tenth_power_over_forty` | 4.5 dB gives ×0.7718; 40 dB gives ÷10; loss applied before the cap; land equivalent |
| `jamming_hand_values` | sidelobe and main-beam J/N; J/N = 15 halves range |
| `burn_through_hand_value` | 2 499.99 m, and Pd = 0.5 there |
| `main_beam_test_wraps_round_north` | bearing difference across 0° |
| `sea_clutter_shortens_r50_for_low_targets_only` | engine: 84.95 km at 100 m altitude, none at 10 km, weather applied |
| `self_screening_jammer_raises_noise_until_emcon_silences_it` | engine: J/N 277.8, Pd 0.0033; silent jammer gives 0 |
| `stand_off_jammer_in_the_sidelobe_and_own_side_jammers` | engine: sidelobe 10, self-screening 10⁴, own side ignored |
| `environment_is_set_by_the_white_cell_and_shown_to_every_side` | players cannot set it; sea state above 9 refused |

## Limitations

- No terrain masking (needs elevation data), multipath, ducting or aspect-dependent RCS.
- Clutter has no Doppler or MTI processing benefit, and no range dependence; it is a flat loss for low targets.
- One scenario-wide sea state and weather loss.
- Jammers are specified per unit in the scenario; ECM equipment joins the database with soft kill (slice 9). No deception jamming, no frequency matching between jammer and radar band.
- Scans are still independent draws every 5 s.
