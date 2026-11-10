# Model: Radar horizon

- **Code:** `crates/sim-geo/src/horizon.rs`
- **Design log:** [DL-0003](../design-log/DL-0003-radar-horizon.md)

## Equation

Standard atmosphere refraction is represented by an effective Earth radius k·R, with k = 4/3 and R = 6 371 km:

d = √(2kR·h_s) + √(2kR·h_t) ≈ 4.12 (√h_s + √h_t) km   (heights in metres)

√(2 · 4/3 · 6 371 000) = 4 121.8 m/√m. We use the conventional rounded **4 120 m/√m**, as the project spec states (0.04 % shorter). A test checks the rounding stays within 0.1 % of the exact derivation.

## Worked examples (unit tests)

| Sensor height | Target height | Horizon |
|---|---|---|
| 25 m | 0 m (surface) | 4.12 × 5 = **20.60 km** |
| 16 m | 9 m | 4.12 × (4 + 3) = **28.84 km** |
| 20 m | 5 m (sea-skimmer) | 4.12 × (4.472 + 2.236) = **27.64 km** |
| 10 000 m | 0 m | 4.12 × 100 = **412 km** |

## Assumptions and limitations

- The atmosphere is a uniform 4/3 Earth. Ducting and sub- and super-refraction are not modelled (Phase 4, weather).
- Negative heights (submerged) are clamped to 0.
- No terrain masking yet. That needs SRTM sampling along the path (Phase 2, radar).
- This is a geometric line-of-sight limit only. Detection range (RCS^¼ scaling, clutter, Pd per scan) is a separate Phase 2 radar model, capped by this horizon.
