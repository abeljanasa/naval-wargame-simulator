# DL-0014: Sonar: passive and active sonar equations

- **Date:** 2026-11-10
- **Phase:** 2, slice 5 ([plan](../plan/phase-2.md))
- **Sources:**
  - Master prompt §8 ("Sonar: passive sonar equation SE = SL − TL − (NL − DI) − DT; speed-dependent source level; transmission loss with spreading, absorption …; active sonar reveals the emitter; sonobuoys, dipping sonar, towed arrays").
  - Textbook underwater acoustics: the passive and active sonar equations, spherical and cylindrical spreading, Thorp's absorption formula, and the use of a normal distribution of signal excess for detection probability.
  - Original design: the 5 km spreading transition, the 40 dB per decade speed law with a 2 kn floor, the ambient-noise line, the 5 dB SE spread, and every generic level, index and threshold.
  - No vendor product, database or binary was consulted.
- **Model doc:** [sonar.md](../models/sonar.md)

## Behaviour

- Hull, towed and dipping sonars detect surface ships and submarines by the passive or active sonar equation, one logged draw each, with every term logged.
- Active-capable sonars transmit under EMCON active and listen under EMCON silent. A pinging ship is heard at its ping level.
- Active detections feed tracks (range and bearing). Passive detections give bearings that share the ESM pool, so mixed sonar and ESM bearings triangulate into fixes.
- Dipping sonar needs a low hover; sonobuoys wait for the missions slice.

## Compatibility

- Save format 7; older saves load with passive-only sensors and the domain noise defaults.
- `UnitSpawn` gains `radiated_noise_db`; `SensorSpec` gains `active`. The scenario loader fills both from the database and category defaults.
- Golden hash `e32c602bbebfedbb` → `66e6a556f1ac1068`: the sonar flag and noise figures are hashed. The demo's ships carry no sonar.
- Benchmarks: 1,000 units 377×, 10,000 units 4.5× (budgets 60× and 1×).
