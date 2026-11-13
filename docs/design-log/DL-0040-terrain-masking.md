# DL-0040: Terrain masking and land clutter

- **Date:** 2026-11-13
- **Phase:** 4, slice 4
- **Sources:**
  - Master prompt §6 (terrain).
  - Textbook 4/3-earth path profiling (standard refraction, as in the radar horizon model, DL-0003).
  - The Phase 2 backlog item on land clutter.
  - Original design otherwise.

## Decisions

- **One test for every line of sight.** `Terrain::masks` serves radar, ESM, optical, radio and coverage, so a path is either clear or not for all of them.
- **Masking after the horizon, not instead of it.** The existing horizon checks, and the exact broad phase built on them, stay as they are. Masking is applied only to pairs that pass them, which keeps runs without packs bit-identical and keeps the cost low.
- **Sampling every 500 m, at most 256 points, linear in latitude and longitude.**
  - 500 m is close to the 15″ cell of the regional pack.
  - Linear interpolation errs by metres at line-of-sight ranges, far below the grid.
  - Long paths (over 128 km, mostly aircraft) are sampled more coarsely.
- **Land units and posts ashore stand on the ground.** Their ground height is added to their sensor or antenna height for the masking test.
  - The horizon still uses height above the ground.
  - A hilltop horizon (adding the ground to the horizon height) would change the broad phase, and is noted as a later refinement.
- **Masked = not attempted.** No detection draw and no event, exactly as beyond the horizon, so the random streams of other pairs are unaffected.
- **Clutter by the ground below.** A low aircraft's clutter background is decided from its height above the ground or sea beneath it. It is land clutter when that is land.

## Open

- Hilltop horizons, and masking for weapons and fire control.
- Radar shadowing by intervening terrain for the coverage *map* at sensor resolution. The coverage grid is the zone's 24 × 24 sample.
