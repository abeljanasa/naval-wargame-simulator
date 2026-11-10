# DL-0010: Tracks with position error and classification

- **Date:** 2026-11-10
- **Phase:** 2, slice 1 ([plan](../plan/phase-2.md))
- **Sources:**
  - Master prompt §8 ("Tracks & contacts: position error ellipses that grow with time since update; classification ladder … track correlation/fusion within a side").
  - Textbook estimation: the Kalman update with an identity measurement matrix and random-walk error growth, as found in any introductory target-tracking text. The Box–Muller transform for normal draws.
  - The per-sensor error values, the domain speed bounds, the 95 % display ellipse and the rule that radar reaches "domain" are original design.
  - No vendor product, database or binary was consulted.
- **Model doc:** [tracks.md](../models/tracks.md)
- **Architecture:** [ADR-0005](../adr/0005-model-crates.md): the models live in the new `sim-sensors` crate.

## Behaviour

- A detection reports a position with drawn range and bearing error; the draws are logged.
- Each side keeps one track per target, fused across all its sensors by a Kalman update.
- Between updates the error grows by the fastest speed the side's knowledge allows.
- Radar classifies a track to its domain. Higher levels wait for ESM, visual and IR/EO.
- Players see the estimated position, the 95 % error ellipse and the domain; never the truth.

## Compatibility

- Save format 3. A v2 save loads with zero covariance and `unknown` classification on its contacts.
- The golden replay hash changes (two more draws per detection): `7ad884a980341eb1` → `8a1f96e397c0a0b7`.
- The 1,000-unit benchmark still runs at 212× real time (budget 60×).
