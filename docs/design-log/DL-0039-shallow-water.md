# DL-0039: Shallow water and depths

- **Date:** 2026-11-13
- **Phase:** 4, slice 3
- **Sources:**
  - Master prompt §6 (terrain and bathymetry).
  - The [Phase 4 plan](../plan/phase-4.md), slice 3.
  - [DL-0038](DL-0038-terrain-packs.md) (packs), DL-0008 (land avoidance).
  - Ordinary seamanship (under-keel clearance, berths in a harbour's best water).
  - Original design otherwise.

## Decisions

- **Same planner, one more obstacle.**
  - Shallow water joins land as an obstacle on the existing 1/30° A* grid, with the same coastal penalty and straight-leg clearance.
  - A finer routing grid was rejected for now. Choke points at TTX scale are kilometres wide, and the 3 × 3 sampling per cell catches banks narrower than a cell.
- **Harbour ends exempt (2 cells).** Coastal bathymetry is the least reliable, and ports often read as land on the coarse layer. Without the exemption, most port departures would fail; with it, the planner's waypoints decide the approach.
- **No deep route is a warning, not a refusal.** Scenarios that ran before packs were installed must still run. The unit is routed round land only, the seat is told, and it meets the shoal guard if it gets there.
- **"The graph" is the charted route.** A route planned deep enough on every leg is trusted, harbour ends included, and the guard applies only off it.
  - A first version applied the guard everywhere. On real data it stopped the Mindoro pilot's supply ship leaving Coron on its own charted route, because the planner and the guard disagreed about the exempt harbour end.
- **A stop, not a grounding.** The data (460 m to 7.4 km cells) cannot support adjudicating damage or refloating, so the engine stops the unit at the last safe point and says so. The white cell can adjudicate the rest. Probabilistic grounding with damage is left until a finer harbour layer exists.
- **Chart depth of a facility = deepest water in its area.** The planner's explicit depth always wins.
- **Save format 26** records `Route::charted`. Format 25 saves still load, with uncharted routes.
- **Display through the engine.** The *Water* profile is computed by `Simulation::route_depths` with the same deterministic sampling, so what the planner sees is what the engine uses.

## Open

- Choke-point depth profiles and lanes: slice 5.
- Submarines use their surface draft. Depth to operate submerged, and bottom bounce, come with acoustics (slice 8).
