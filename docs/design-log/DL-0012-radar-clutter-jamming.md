# DL-0012: Radar clutter, weather loss and noise jamming

- **Date:** 2026-11-10
- **Phase:** 2, slice 3 ([plan](../plan/phase-2.md))
- **Sources:**
  - Master prompt §8 ("Radar: … sea/land clutter for low-altitude targets; per-scan probability of detection; degraded by jamming and weather").
  - Textbook radar relations: range ∝ SNR^¼ from the radar range equation, noise-jamming J/N falling as 1/r² against signal as 1/r⁴, and the resulting burn-through range.
  - Original design: clutter as a flat loss per sea-state step below a 150 m ceiling, land as sea state 5, the 30 dB sidelobe figure, the main-beam test from beamwidth, the scenario-wide weather loss, and all generic database values.
  - No vendor product, database or binary was consulted.
- **Model doc:** [radar.md](../models/radar.md)

## Behaviour

- Low targets lose detection range to sea or land clutter; the loss grows with sea state.
- A scenario-wide weather loss applies to every radar.
- Units can carry noise jammers that radiate while EMCON is active. Self-screening and stand-off jamming both raise the radar's noise; burn-through emerges as range closes.
- The scenario or white cell sets sea state and weather (`SetEnvironment`); every side sees them.
- Every detection attempt logs its clutter loss, weather loss and J/N.

## Data and compatibility

- Database migration 0002: `clutter_db_per_sea_state` and `beamwidth_deg` on radars, with values for all 15 generic radars. New rules `sensor.radar_fields` (error) and `sensor.radar_defaults` (warning).
- Save format 5; older saves load with a calm sea, no weather and no jammers.
- Scenario format: optional `environment` and per-unit `jammer`.
- The demo scenario now runs in sea state 3. Golden hash `d46bae3f7ea350a8` → `271dd129bbb9548d`.
- 1,000-unit benchmark: 204× real time (budget 60×).
