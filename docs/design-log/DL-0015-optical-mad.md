# DL-0015: Visual, EO/IR and MAD

- **Date:** 2026-11-10
- **Phase:** 2, slice 6 ([plan](../plan/phase-2.md))
- **Sources:**
  - Master prompt §8 ("IR/EO, visual, MAD: short-range, weather and daylight dependent").
  - Textbook solar geometry (declination and hour angle) for daylight.
  - Original design: R₅₀ as 0.7 of the instrumented range scaled by √(length/100 m), the light and haze factors, the identification bands (0.3 and 0.6 R₅₀), MAD as a slant-range Pd curve with a 300 m position error, and all generic values.
  - No vendor product, database or binary was consulted.
- **Model doc:** [optical-mad.md](../models/optical-mad.md)

## Behaviour

- Lookouts, EO/IR directors, IRST and periscopes detect surfaced targets within the horizon, limited by daylight, visibility and target size.
- A close look identifies the target to category or class, and class reveals affiliation. This is the first identification that does not depend on the target emitting.
- Visual gives positions; EO and IR give precise bearings that join the bearing pool.
- MAD on aircraft finds submarines within about a kilometre of slant range.

## Compatibility

- Save format 8; older saves load with a 1970 start, 40 km visibility and the domain lengths.
- Environment gains `start_unix_s` and `visibility_km`; units gain `category` and `length_m` (filled by the scenario loader from the database).
- The demo starts at 2026-11-10 00:00 UTC (mid-morning in Bass Strait). Golden hash `66e6a556f1ac1068` → `a6d33e91f79e97ea`.
- Benchmarks: 1,000 units 334×, 10,000 units 3.8× (budgets 60× and 1×).
