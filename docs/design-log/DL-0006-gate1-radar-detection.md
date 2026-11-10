# DL-0006: Gate 1 interim radar detection and contacts

- **Date:** 2026-11-10
- **Decision:** the user approved a minimal, clearly labelled detection model so Gate 1 ("units move and detect") can be shown before Phase 2's full sensors.
- **Sources:**
  - Master prompt §8, the radar bullet: range scales as RCS^¼ from a reference range, capped by the radar horizon, with a per-scan probability of detection.
  - Master prompt §6.2: sensors every 5 s.
  - Design principles 4 and 5: real fog of war, and every draw explainable.
  - The fourth-root scaling is the textbook radar-range equation (e.g. Skolnik, *Introduction to Radar Systems*).
  - The Pd curve 1/(1+(r/R₅₀)⁸) and the 300 s contact timeout are **original design**, chosen for being monotonic, hand-checkable and easy to explain. Phase 2 must revisit them against a fluctuation-model behaviour spec.
- **Model doc:** [radar-detection-gate1.md](../models/radar-detection-gate1.md)
