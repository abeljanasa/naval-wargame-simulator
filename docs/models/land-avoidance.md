# Model: Land avoidance (Phase 1)

- **Code:** `crates/sim-geo/src/land.rs` (mask and planner), `crates/sim-core/src/sim.rs` (planning when orders apply), `crates/sim-core/src/movement.rs` (grounding guard)
- **Tests:** `sim-geo` land unit tests, `crates/sim-scenario/tests/land.rs`, and the Gate 1 demo test
- **Design log:** [DL-0008](../design-log/DL-0008-land-avoidance.md)

## Land mask

The bundled coastline is Natural Earth 1:50m land: public domain, 1,420 polygons, 60,669 vertices (`data/maps/ne_50m_land.geojson`, the same file the desktop map draws).

- **Grid:** it is rasterised at load into a global grid of 1/30° cells: 10,800 × 5,400, about 3.7 km at the equator.
- **Which cells are land:** a cell is land if its centre lies inside a land polygon, using the even–odd scanline rule, so lakes cut out of land stay water.
- **Near-land layer:** a second layer marks water cells next to land (land grown by one cell).
- **Determinism:** only `+ − × ÷`, floor and ceil are used, so the mask is identical on every OS (ADR-0003).
- **In the state hash:** the mask's id (`ne_50m_land@30`) is hashed, so a run that forgets to attach it after loading a save cannot silently match one that has it.

## Planning (ships and submarines only)

When a route is ordered (`SetRoute`, or a scenario unit's starting route), each leg is checked separately.

1. **A clear leg is kept as given.** A leg is clear if its straight cell line stays in water and away from the coast, except within 2 cells of either end, so legs can start and finish inshore.
2. **A leg that is not clear is replanned:**
   - A* runs over water cells: 8-connected, octile integer costs of 10 and 14, no corner-cutting between land cells.
   - Cells next to land cost 4× to keep clearance.
   - The search region is the leg's bounding box plus a margin; it is retried once with a larger margin, and limited to 9 million cells.
   - The cell path is then shortened by string-pulling into the fewest straight legs that are clear.
3. **The order is refused** (logged as `CommandRejected`) if the unit or a waypoint is on land, or no sea route exists.
   - A scenario route that can't be planned is dropped, and the unit spawns holding its course (`RouteBlocked`).
   - A route that was planned round land logs `RouteReplanned` with the number of legs replanned.

## Grounding guard

After each tick's movement, a ship or submarine that would move from a water cell into a land cell stays at its last water position, comes to all-stop, and logs `LandAhead`. This catches dead reckoning towards a coast, and corners cut while turning.

## Verification

- **Points:** known land and sea points are correct, including New South Wales, Tasmania, Bass Strait, Anatolia and open oceans.
- **Replanning:** a leg from west to east of Tasmania is replanned. Every new leg, sampled at 201 points, stays at sea, and the engine sails it to completion without a grounding stop.
- **Grounding:** a ship steaming south across Bass Strait stops before Tasmania.
- **Refusal:** a route to a point on land is refused.
- **Antimeridian:** a leg across the antimeridian in open ocean stays direct.
- **Demo:** the demo scenario's ships never ground and no route is refused.
- **Performance:** 1,000 units at 216× real time and 10,000 at 2.4× (budgets 60× and 1×).

## Limitations

- **Resolution:** 3.7 km. Channels narrower than about one cell may be closed, and small islets missing from the 1:50m data are not obstacles.
- **No shallow water or draft limits.** These need bathymetry (GEBCO), which is not bundled yet.
- **Aircraft and land units ignore the mask.**
- **Not optimal:** routes are near-shortest on the grid with a coastal penalty, not guaranteed shortest at sea.
