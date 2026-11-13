# Model: Automatic sense-zone tasking

> Phase 4 slice 12 (master prompt §7.1). Original design; **not SME-validated**. It builds on the Phase 3 coverage model ([planning-layer.md](planning-layer.md) §Coverage), terrain masking, acoustics and air operations.

- **Code:** `crates/sim-core/src/tasking.rs`, with `coverage.rs` for coverage and reach.
- **Tests:**
  - Unit tests in `tasking.rs`.
  - Engine tests in `crates/sim-scenario/tests/tasking.rs`.
  - AAR: `crates/sim-aar/tests/tasking.rs`.
- **Design log:** [DL-0047](../design-log/DL-0047-sensor-tasking.md)

## The goal

A sense zone may carry a **coverage goal**: the share of its area the side wants watched by the zone's sensor types (`coverage_goal`, above 0 and at most 1). It may also carry **assets**: the units the engine may task for it. With no assets, any free unit of the side may be tasked. Without a goal, players task sensors themselves, as before.

## The pass

Every **5 minutes** (300 s, on the minute), for each zone with a goal, in zone-id order:

1. **Release:** units whose task has ended are released. A task ends when the unit has another mission (a player's order), its mission ended, it was recalled or went home at bingo, it was lost, or its zone no longer has a goal.
2. **Coverage now:** the coverage model gives the zone's coverage, terrain masking included. At or above the goal, nothing is done. A zone that was short logs that it is back at its goal.
3. **Free units of the zone's side** (of its assets, if it lists any) are counted, each with its **reach**: the longest nominal range of its sensors of the zone's types (coverage model).
   - Ships and submarines with no mission (or a completed one), operational, not in an action group. Their reach is worked out where they are.
   - Aircraft parked at the side's airbases, not in maintenance and not damaged. Their reach is worked out at their cruise altitude, so a patrol aircraft's radar horizon counts.
4. **Greedy choice:** among the free units and the zone's uncovered cells (as stations), the pair whose station puts the most uncovered cells within the unit's reach. Ties go to the nearer unit, then the lower unit id. The unit is **tasked**, the cells it would cover count as covered, and the step repeats until the projected coverage meets the goal, nothing more can be gained, or four units have been tasked to the zone in this pass.
   - A ship or submarine gets a **Support** mission at the station: it goes there and holds within 2 km.
   - A parked aircraft is **launched** on a **patrol** sortie round the station, within a quarter of its reach (5–30 km). It flies the sortie under air operations: readiness delay, runway, bingo, return.
5. **Shortfall:** a zone still short with nothing tasked in this pass logs that it is **short** of its goal, once, until it is met again.

When a tasked aircraft goes home at bingo, it is released, and the next pass sends a relief.

## Records

- **Events:**
  - `SensorTasked`: zone, unit, station, reach, coverage before and projected after.
  - `CoverageShortfall`: zone, coverage, goal.
  - `CoverageGoalMet`: zone, coverage.
- **The AAR timeline** (kind `tasking`) names the zone, the unit and the coverage.
- **Saves:** the tasking state is saved (format 27) and hashed only when a scenario uses goals. Scenarios without goals keep their results and golden replays.
- **Desktop:**
  - The **Coverage** tab shows each zone's goal, the units tasked to it, and *short* when nothing is left to send.
  - The white cell sets goals in the Planning tab (*Coverage goal %*, a logged planning order).
  - The scenario editor sets them for sense zones.

## Worked example

A 25 km sense zone for radar, with a goal of 60 %. One frigate is idle 55 km west, another is on patrol elsewhere.

- At 0 s the zone has 0 % coverage. The idle frigate is the only free unit. The station that would cover the most of the zone is chosen, and the projected coverage is at least 60 %. The frigate gets a Support mission there.
- The busy frigate is left alone.
- About 1.5 hours later the frigate is on station and the zone's coverage is at or above 60 %. No further tasking is needed.

## Verification

| Test | Checks |
|---|---|
| `the_widest_reach_goes_where_it_covers_most` | the greedy choice, by hand |
| `ties_go_to_the_nearer_unit_and_nothing_to_gain_is_no_task` | tie-break; no gain, no task |
| `reviews_every_five_minutes` | the period |
| `a_zone_short_of_its_goal_gets_a_free_ship_which_covers_it` | tasking, busy units left alone, coverage reached |
| `only_the_zones_assets_are_used_and_a_shortfall_is_logged_once` | assets; one shortfall log |
| `a_released_unit_is_replaced_by_another` | release on a player's order; relief |
| `a_parked_patrol_aircraft_is_launched_to_watch_the_zone` | an aircraft sortie |
| `tasking_survives_a_save_and_runs_on_identically` | save, restore, determinism |
| `tasking_and_shortfall_are_on_the_timeline` | the AAR |

## Limits

- **Greedy, not optimal:** each step takes the best single addition. Its choices are explainable and logged, which matters more in a TTX than the last few per cent.
- **Approximate reach:** reach is worked out where the unit is now, not at the station (light, local weather), and the search ignores terrain masking. The coverage the engine reports and acts on is the full model's.
- **No moving units off station:** a zone over its goal keeps the units it has. Free units are only ships, submarines and parked aircraft; units with missions are never taken.
- **Not modelled yet:** sonobuoy fields and towed-array streaming are not ordered by the engine.
