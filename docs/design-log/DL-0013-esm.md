# DL-0013: ESM: intercept, bearings, fixes and identification

- **Date:** 2026-11-10
- **Phase:** 2, slice 4 ([plan](../plan/phase-2.md))
- **Sources:**
  - Master prompt §8 ("ESM: detects active emitters beyond their own detection range; bearing-only lines; multi-platform triangulation produces fixes; emitter library identification with confidence").
  - Textbook geometry: one-way versus two-way propagation; weighted least-squares intersection of bearing lines, with the covariance from the normal equations.
  - Original design: the 2.5× intercept factor, the 2° bearing accuracy, the 10° crossing minimum, the same-scan rule for fixes, the identification ladder from carrier sets, and the 60 s display life of bearings.
  - No vendor product, database or binary was consulted.
- **Model doc:** [esm.md](../models/esm.md)

## Behaviour

- ESM intercepts radiating radars and jammers of other sides out to 2.5× the radar's reference range, within the horizon, with one logged draw and a logged bearing error.
- Bearings from two or more of a side's platforms in the same scan triangulate into a fix, which feeds (or creates) the track.
- The emitter library from the platform database identifies the carrier: class, category or domain, depending on how many classes carry the emitter. Class-level identification reveals affiliation.
- The desktop draws bearing lines and names contacts by class, category or domain.

## Performance

ESM roughly doubled the pairs examined, and the 10,000-unit benchmark fell to 0.90× (budget 1×). Two optimisations fixed it, and both give identical results (the golden hash did not change when they went in):

- **Chord pre-check:** an exact test in both the radar and ESM passes. The straight-line distance between Earth-surface positions is never longer than the geodesic, so pairs whose chord exceeds the range bound by more than 1 m are skipped without solving the geodesic.
- **Geodesic reuse:** one geodesic per (observer, emitter unit), shared by that unit's radars.

| Benchmark | Before ESM | ESM, unoptimised | ESM, optimised | Budget |
|---|---|---|---|---|
| 1,000 units | 204× | 86.6× | **451×** | 60× |
| 10,000 units | 2.4× | 0.90× | **5.0×** | 1× |

## Compatibility

- Save format 6; older saves load with no bearings, an empty library and no category or class on contacts.
- New `SetEmitterLibrary` command (scenario and white cell); the scenario loader issues it.
- Golden hash `271dd129bbb9548d` → `e32c602bbebfedbb`: ESM draws, and the library is hashed.
