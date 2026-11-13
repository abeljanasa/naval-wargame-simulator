# Model: Choke points, depths and shipping lanes (Phase 4 slice 5)

- **Code:**
  - `crates/sim-core/src/lanes.rs`: lanes, transit assessment, choke depths.
  - `planning.rs`: choke transits.
  - `crates/sim-aar`: the transit table.
  - `apps/desktop`: lanes on the map, the Planning tab, the AAR *Transits* tab.
- **Tests:**
  - `sim-core` lanes unit tests;
  - `crates/sim-aar/tests/chokepoints.rs`;
  - `src/lib/lanes.test.ts`.
- **Design log:** [DL-0041](../design-log/DL-0041-choke-points-and-lanes.md)
- **Builds on:** the [planning layer](planning-layer.md) (zones that name a choke point), [terrain packs](terrain-and-bathymetry.md), [shallow water](shallow-water.md)

## A choke point

A choke point is the set of zones that name it (`choke_point` on a zone), as before. It now also has:

- **Its water:** `Simulation::choke_depths` reports, for each choke point:
  - the least, median and deepest water over a 24 × 24 grid of its zones' area, from the terrain packs;
  - a **section** of 120 samples across it, through the centre of its zones' bounding box. The section runs square to its first lane where it has one, or else across its narrower way. Samples outside its zones have no depth.

  The white cell's Planning tab shows these figures and the section against a draft line. It also shows the **navigable width**: the widest unbroken run of the section with at least draft + 1 m of water, for a draft the white cell enters.
- **Its lanes:** the lanes that name it, or whose centreline runs through its zones, and their traffic added up.

## Shipping lanes

A lane is part of the planning layer (`lanes` in the scenario and in `SetPlanning`):
- a centreline of at least two points;
- a width (default 2 km);
- optionally a choke point and a **traffic figure** in ships per day.

Lanes are public: every seat sees them on the map as a translucent band of their width, labelled with the name and the traffic.

**Traffic needs a source.** A lane with a traffic figure must name its `source`, or it is refused. The source is a licensed traffic data set, a chart, or "planner's estimate".
- **Lanes the white cell draws:** in the Planning tab, *Draw on the map*, then *Finish lane*. A traffic figure typed in for one of them is marked as the planner's estimate.
- **Lanes imported from a file:** *Import GeoJSON…* takes LineString or MultiLineString features with `name`, `width_m`, `ships_per_day`, `source` and `choke_point` properties. A traffic figure without a `source` is dropped, and the import says so.
- **Nothing is bundled.** Lanes from open traffic data come in only where the client holds a licence for that data.

Without lanes, the planning layer serialises and hashes exactly as before: the field is left out when empty.

## Transit risk

When another side's unit enters a choke point (`ChokePointTransit`, as before), the transit is assessed if the choke point has lanes or terrain packs are loaded. The assessment is logged as `TransitAssessed`:

| Field | From |
|---|---|
| Lane | The choke point's lane whose band the unit is in, if any |
| Traffic | The choke point's lanes' ships per day, added up |
| Water | Depth under the unit as it enters (terrain packs) |
| Draft | The unit's |
| Risk | The worse of the two ratings below |

| Rating | Low | Moderate | High |
|---|---|---|---|
| Traffic (ships/day) | < 20 | 20–99 | ≥ 100 |
| Under-keel margin (water − draft) | ≥ 5 m or unknown | 2–5 m | < 2 m |

The thresholds are original design, chosen to read as an indicator for TTX discussion. They are not a collision or grounding probability.

**In the AAR:** the AAR has a *Transits* table (and `transits.csv`), with time, choke point, unit, side, lane, traffic, water, draft and risk. Each transit is also in the timeline ("…: transit of Mindoro Strait in lane Main channel, 150 ships a day, 54 m of water for a 4.5 m draft: high risk").

Assessment is analysis only: it does not change movement, detection or any draw.
