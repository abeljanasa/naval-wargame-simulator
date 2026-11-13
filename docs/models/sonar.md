# Model: Sonar: passive and active sonar equations

> Phase 2 slice 5. The textbook sonar equations with original-design simplifications and generic values; **not SME-validated**. With a sound-velocity profile in force, transmission loss, depths, reverberation, towed arrays and sonobuoys follow [acoustics.md](acoustics.md) (Phase 4 slice 8); without one, this page holds unchanged.

- **Code:** `crates/sim-sensors/src/sonar.rs` (equations), `crates/sim-core/src/sonar.rs` (sonar pass of the detection system)
- **Tests:** unit tests in `sim-sensors` (hand values); engine tests in `crates/sim-core/tests/detection.rs` (sonar section)
- **Design log:** [DL-0014](../design-log/DL-0014-sonar.md)

## Equations

All terms are in dB.

- **Passive:** SE = SL − TL − (NL − DI) − DT
- **Active** (noise-limited; reverberation is not modelled): SE = SL_s − 2·TL + TS − (NL − DI) − DT
- **Detection:** Pd = Φ(SE / 5 dB), the normal distribution function. Pd is 0.5 at SE = 0 (the same anchor as R₅₀ for radar), 0.84 at +5 dB and 0.16 at −5 dB. One `Sensors` draw per attempt.

## Terms

| Term | Model |
|---|---|
| **TL**, transmission loss | 20·log₁₀ r out to 5 km (spherical), then 20·log₁₀ 5000 + 10·log₁₀(r / 5 km) (cylindrical), plus Thorp absorption α(f)·r. α = 0.11f²/(1+f²) + 44f²/(4100+f²) + 2.75×10⁻⁴f² + 0.003 dB/km, f in kHz |
| **SL**, target radiated noise | SL₁₀ + 40·log₁₀(v / 10 kn), speeds below 2 kn count as 2 kn. SL₁₀ by category: submarine 115, surface combatant 140, auxiliary 145, merchant 150 dB |
| A pinging target | its radiated noise power-summed with its active source level: **active sonar reveals the emitter** |
| **NL**, noise | ambient 45 + 5 × sea state, power-summed with own self-noise at own speed (same 40·log₁₀ law) |
| **TS**, target strength | submarine 15 dB, surface ship 20 dB |

Generic figures by sensor kind (all original design):

| Kind | f (kHz) | DI | DT | Active SL | Self-noise at 10 kn |
|---|---|---|---|---|---|
| Hull, active-capable | 3.5 | 20 | 10 | 220 | 50 |
| Hull, passive (submarine bow arrays) | 1.0 | 20 | 10 | — | 50 |
| Towed array | 0.5 | 25 | 10 | — | 35 (streamed astern) |
| Dipping | 10 | 15 | 10 | 215 | 35 |

## Modes and rules

- An active-capable sonar (database `is_active`) **transmits while the unit's EMCON is active**, and listens passively when EMCON is silent.
- **Active** detections give range (σ 200 m) and bearing (σ 2°): a position measurement fused into the track, classified to domain.
- **Passive** detections give a bearing (σ 2°). It goes into the same pool as ESM bearings, so bearings from two or more of a side's units in one scan triangulate into a fix ([esm.md](esm.md)). Passive sonar classifies to domain: it tells surface from submerged.
- Sonars work from ships and submarines. A **dipping sonar** works only with its helicopter at or below 100 m and 2 m/s (hovering). **Sonobuoys** listen once laid ([acoustics.md](acoustics.md)).
- Targets are surface ships and submarines at any depth.
- Every attempt logs SL, TL, NL, DI, DT, SE, Pd, the draw and the measurement errors (`SonarAttempt`).

## Worked examples

| Case | Working | Result |
|---|---|---|
| Towed array on a ship at 10 kn, sea state 3; submarine at 10 kn, 20 km | NL = 60 ⊕ 35 = 60.01; TL = 80.56; SE = 115 − 80.56 − 35.01 − 10 | SE = **−10.57 dB**, Pd = **0.017** per scan |
| MF hull sonar, own ship 15 kn, sea state 3; submarine at 10 km | NL = 60 ⊕ 57.04 = 61.78; TL = 79.38; SE = 220 − 158.76 + 15 − 41.78 − 10 | SE = **+24.46 dB**, Pd ≈ 1 |
| Frigate pinging, heard by a submarine bow array at 60 km | SL = 112.0 ⊕ 220 ≈ 220 dB | detected far beyond its radiated-noise range |

The first two examples show the trade-off: a quiet submarine is hard to hear even with a towed array, while active sonar finds it at close range but gives the hunter away.

## Verification

| Test | Checks |
|---|---|
| `thorp_and_transmission_loss_hand_values` | α at 1 and 3.5 kHz; TL at 2 km and 20 km |
| `noise_rises_forty_db_per_decade_of_speed` | +12.04 dB at 20 kn; 2 kn floor; power sum; ambient |
| `passive_towed_array_against_a_quiet_submarine` | SE −10.569, Pd 0.0173 |
| `active_hull_sonar_at_ten_km` | SE +24.459 |
| `pd_curve_is_the_normal_distribution` | 0.5, 0.841, 0.159 |
| `passive_towed_array_matches_the_sonar_equation` | engine: logged terms reproduce the equation, SE ≈ −10.57 |
| `active_hull_sonar_tracks_a_submarine_and_emcon_makes_it_listen_instead` | engine: active track at domain; EMCON silent switches to passive |
| `a_pinging_ship_is_heard_at_its_ping_level` | engine: SL ≈ 220 dB at 60 km; acoustic bearing line |
| `dipping_sonar_needs_a_low_hover_and_sonobuoys_need_laying` | usability rules |

## Limitations

- No sound-velocity profile, thermal layers, convergence zones, bottom bounce or depth dependence (Phase 4).
- No reverberation, so active sonar is noise-limited at every range.
- One band per sensor; target noise has no spectrum (no tonals), so passive classification stops at domain.
- Generic acoustic figures in code; they move to database fields in a later migration.
- Sonobuoys and towed-array deployment states are not modelled until missions.
