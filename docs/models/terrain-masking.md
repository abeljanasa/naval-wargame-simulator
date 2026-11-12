# Model: Terrain masking (Phase 4 slice 4)

- **Code:**
  - `crates/sim-geo/src/terrain.rs`: `Terrain::masks`.
  - `crates/sim-core/src/detection.rs`: radar, land clutter, `terrain_masks`, `ground_m`.
  - `esm.rs`, `optical.rs`: ESM and optical.
  - `comms.rs`: radio links.
  - `coverage.rs`: sense coverage.
- **Tests:**
  - `sim-geo` terrain: `a_ridge_masks_low_paths_but_not_high_ones`, `the_earth_bulge_counts_on_a_4_3_earth`;
  - `crates/sim-scenario/tests/masking.rs`.
- **Design log:** [DL-0040](../design-log/DL-0040-terrain-masking.md)
- **Builds on:** [terrain and bathymetry packs](terrain-and-bathymetry.md), [radar horizon](radar-horizon.md)

Everything here needs terrain packs. Without them, every check below is skipped, and runs and golden replays are unchanged.

## The line-of-sight test

`Terrain::masks(a, h_a, b, h_b, d)` asks whether land rises into the straight path between two points. The heights are above sea level, and `d` is the surface distance.

- **Samples:** the path is sampled every 500 m, at most 256 points, interpolated in latitude and longitude. That is close enough at line-of-sight ranges of tens of kilometres.
- **The ray:** at distance `x` it is at `h_a + (h_b − h_a)·x/d − x·(d − x)/(2·R')`, with `R'` = 4/3 × 6,371 km, the same standard refraction as the radar horizon.
- **What blocks:** a sample blocks if the land there is higher than the ray. Sea never blocks; the horizon already covers the sea surface. Points no pack covers never block.
- **Determinism:** plain arithmetic, so the answer is identical on every OS.
- **Heights:** each end uses its height as the model already has it: mast or sensor height above the surface, or an aircraft's altitude. A **land unit** (site, battery) and a **command post ashore** stand on the ground, so the ground height under them is added. A radar on a hill sees over the lower land in front of it.

The horizon checks themselves are unchanged, using height above the surface. Masking is an extra test applied after them.

## Where it applies

| Model | Effect when masked |
|---|---|
| Radar | No detection attempt (as beyond the horizon): no draw, no event |
| ESM | No intercept of that emitter |
| Visual, EO, IR | No detection attempt |
| Comms (line-of-sight links) | The hop counts as beyond the horizon, so the link is down or goes through a relay. Beyond-line-of-sight links (HF, satellite) are unaffected |
| Sense coverage | A cell the sensor's nominal range reaches is uncovered if terrain hides a reference target there: a 10 m ship for radar and optical, a 15 m emitter for ESM. Sonar and MAD are not masked |

Weapons, point defence and weapon tracks are not masked in this slice.

## Land clutter for low air targets

An aircraft at or below the clutter ceiling (150 m) **above the ground below it**, over land on the packs, is seen against **land clutter**. It used to be treated as over the sea. Over the sea, low aircraft keep sea clutter. This closes the Phase 2 backlog item.

## Cost

The Mindoro pilot (4 h, 9 units) runs in 1.9 s with both packs against 1.65 s without, including loading the packs. Only pairs that pass every other range and horizon check are sampled.
