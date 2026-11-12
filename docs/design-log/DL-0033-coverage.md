# DL-0033: Sense-zone layers, coverage and gaps

- **Date:** 2026-11-12
- **Phase:** 3, slice 14 ([plan](../plan/phase-3.md))
- **Sources:**
  - Master prompt §7.1: sense zones are layered by depth (outer, middle, inner) and sensor type, coverage and gaps are shown on the map, and in the TTX release players task sensors manually.
  - The existing detection models: radar, ESM, optical, sonar, MAD.
  - Original design: the reference targets and the grid.
  - No vendor product, database or binary was consulted.
- **Behaviour doc:** [planning-layer.md](../models/planning-layer.md) §Coverage

## Decisions

- **Coverage is a query, not state.** It answers "where could we see a typical contact now?" for a facilitator or a player planning sensor tasking. Nothing in the simulation depends on it. It draws no random numbers, is not saved and is not hashed, and computing it cannot change a run.
- **Nominal ranges against reference targets.** Detection is probabilistic and depends on the target. For a picture of coverage we need one range per sensor, so each sensor's range is taken against a stated reference:
  - radar, visual, EO and IR: a 1 000 m² (100 m) ship;
  - ESM: a 15 m emitter;
  - sonar and MAD: a 115 dB submarine at 5 kn.

  The ranges use the same model functions and the current sea state, weather, light and EMCON as detection. The doc says plainly that this is an expectation, not a guarantee.
- **Passive sonar range by bisection.** The sonar equation has no closed-form range with absorption. Transmission loss grows with range, so 60 bisection steps find the signal-excess-zero range to well below a metre. The search is deterministic and needs no table.
- **A 24 × 24 grid.** At most 576 point tests per zone per refresh. That is cheap enough to refresh every 3 s for several zones, and fine enough to show a horizon edge across a choke point. Cells outside the zone are dropped, so polygons look right.
- **Layers are labels; sensor types filter.** A layer (outer, middle, inner) is what staff talk about. Sensor types are what make an outer radar/ESM layer different from an inner sonar layer. Engage zones may carry a layer, but only sense zones list sensors.
- **Players see their own side's coverage only.** A side seat asking for another side's zone is refused. The white cell sees all.
- **Automatic sensor placement and tasking stay in Phase 4.** §7.1 assigns them to the analysis phase.

## Compatibility

- **Zone fields:** `layer`, `sensors`, both optional.
- **Save format 22.**
- **New:** `Simulation::zone_coverage` and `zone_side`; desktop command `zone_coverage`.
- **Golden:** the planning demonstrator's hash changes, because its zones hash with the new fields. Its behaviour is unchanged.
