# Model: Shallow water and depths (Phase 4 slice 3)

- **Code:**
  - `crates/sim-geo/src/land.rs`: `DepthLimit`, `plan_route_for`, shallow cells.
  - `crates/sim-core/src/sim.rs`: `plan_sea_route`, `route_depths`.
  - `crates/sim-core/src/movement.rs`: the shoal guard.
  - `crates/sim-core/src/planning.rs`: `chart_depth_m`, facility depths.
  - `crates/sim-core/src/depths.rs`: depth along a route.
  - `apps/desktop`: warnings, the *Water* row and profile, Planning-tab depths.
- **Tests:**
  - `sim-geo` land tests: `deep_draft_routes_round_a_shallow_bank`, `no_deep_route_falls_back_and_says_so`, `a_leg_may_start_and_end_in_shallow_water`;
  - `crates/sim-scenario/tests/shallow.rs`;
  - `src/lib/terrain.test.ts` (water text and profile).
- **Design log:** [DL-0039](../design-log/DL-0039-shallow-water.md)
- **Builds on:** [terrain and bathymetry packs](terrain-and-bathymetry.md) and [land avoidance](land-avoidance.md)

Everything here applies only when terrain packs are loaded and the unit's draft is known; the draft comes from the platform database. Without packs, routes, movement and facilities behave exactly as before, and the golden replays are unchanged.

## Routing by draft

A ship or submarine's route is planned round land as before. It is also kept out of water shallower than its **draft plus 1 m under-keel clearance**.

- **Grid:** the land planner's grid, 1/30° (about 3.7 km).
- **Shallow cells:** a cell is too shallow if any of 3 × 3 points across it is shallower than that. Land on the terrain counts as shallow, so small islands the 1:50m coastline misses are avoided too. Where no pack covers, a cell counts as deep enough.
- **Search:** cells are only worked out as the search reaches them, and are cached for the leg.
- **Clearance:** as with the coast, cells next to shallow water cost 4× and straight legs avoid them. Turns and dead reckoning so keep a margin.
- **Harbour ends:** cells within 2 of a leg's ends (about 7 km) are exempt, so a leg can start in a harbour or end at an inshore anchorage.
  - The bathymetry is coarsest at the coast (15″ is about 460 m; the 4′ world layer is about 7.4 km). A harbour approach is the planner's call, made with the waypoints they place.
- **No deep route:** when no route keeps the depth, the leg is planned round land only, and `RouteShallow` names the unit and the number of legs. The order is not refused. The seat sees: "*X*: no route deep enough for its draft. It is routed round land only and may meet shoal water".

**The navigable graph:** a route planned deep enough on every leg is **charted** (`Route::charted`, saved from format 26). A unit following a charted route is on the graph.

## Grounding risk off the graph: the shoal guard

A ship or submarine **off the graph** is checked every tick. That means it has no route, its route is finished, or its route is uncharted (planned without packs or draft, a shallow leg, or a point set by the RAS logic).

- **The check:** would this tick's move take it into water shallower than its draft, and shallower than where it was?
- **If so:** it stays at its last position, comes to all-stop, and logs `ShoalAhead` with the depth there. The seat sees "*X* stopped: 5 m of water ahead, less than its draft".
- **Leaving shallow water:** it can still move from shallow water it is already in towards deeper water.
- **On a charted route:** the guard is not applied, including in the exempt harbour ends.
- **Land:** the same message comes now for the existing land guard ("*X* stopped: land ahead").

**Why a stop, not a grounding:** the bathymetry is too coarse to adjudicate a real grounding (hull damage, refloating) fairly. The engine stops the unit and tells the seat. The white cell can adjudicate damage with an override if the exercise needs it.

## Ports, anchorages and choke points take their depth from the chart

Each fixed facility gets a **chart depth**: the deepest water among a 9 × 9 grid over its area (its polygon or circle, else `radius_m` round its position) and its position.
- **Why the deepest:** a port or anchorage has its berths in its best water, and a coastal cell on a coarse grid can read as land.
- **When it is used:** whenever the planner has not set `depth_m`, for the draft check on diversions (repair, fuel, weather, staging) and on service alongside.
- **Override:** the planner's `depth_m` (scenario editor, or the white cell's Planning tab) always wins.
- **Where it shows:** the Planning tab shows the chart depth as the box's placeholder, and whether the depth in force is "set by the planner", "from the chart", or "no limit".
- **Facilities on a unit** (an oiler's RAS point) have no chart depth.

Choke-point depth profiles and shipping lanes come with slice 5. In this slice, a choke point's zones and its facilities use the chart through the routing and the facility depths above.

## Depth along the route (the planner's view)

`Simulation::route_depths(unit, viewer)` samples the rest of a unit's route, from its position through its remaining waypoints.
- **Samples:** every 250 m, at most 200 points.
- **It returns:** the depth here, the profile, the least depth ahead and how far, and whether the route is charted.
- **Who sees it:** a side sees it for its own units, the white cell for any unit. It is display only.

The unit panel shows it as a *Water* row with a profile:
- the sea bed against a dashed draft line;
- depths scaled to the larger of 4 × draft and 50 m, so shallows have room;
- the row is shown in the warning colour when the least water ahead is less than draft plus clearance.

## Batch runs

`nws-batch run`, `mc` and `aar` take `--terrain <pack>…`. With the packs:
- the PAR pilot (`phase3-pilot-mindoro`) plans all routes charted. One shoal stop occurs: the supply ship slowing past the end of its route at Coron, off the graph;
- the Kent passage pilot behaves as without packs, and only its hash differs (by the terrain id).

The AAR replay verifies with `--terrain`.
