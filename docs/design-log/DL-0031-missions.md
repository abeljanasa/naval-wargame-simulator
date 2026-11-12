# DL-0031: Support, mining, mine-clearing, cargo, barrier and denial missions

- **Date:** 2026-11-12
- **Phase:** 3, slice 12 ([plan](../plan/phase-3.md))
- **Sources:**
  - Master prompt §7 (missions: patrol, strike, support, escort, ferry, mining, mine-clearing, cargo; P1), §7.1 (a choke point is a mission object for barrier patrol, mining, escort or denial), §8 (mines: laying, fields, transit risk, mine-countermeasures missions).
  - The textbook simple-initial-threat formula already used for transit risk (DL-0020).
  - Original design: mission behaviours, the sweep width, the risk factor, the sweep speed and the handling time.
  - No vendor product, database or binary was consulted.
- **Behaviour doc:** [missions.md](../models/missions.md)

## Decisions

- **Missions stay one enum in `sim-ai`.** Each new mission is a variant with its own validation and code. The AI system gains one arm per mission, so a reviewer can read every mission's behaviour in one place.
- **Progress is a phase and a start tick on the mission state.** Barrier legs and cargo loading need memory. Two small fields serve every multi-step mission. They are hashed only when used, so existing golden hashes are unchanged.
- **Who engages on their own.** Patrol, ferry, escort, strike, barrier patrol and denial units engage. Support, mining, mine-clearing and cargo units are logistic or specialist tasks, so they defend themselves (point defence) but do not go hunting. This matches what a commander expects of an oiler, a minelayer or a minehunter, and stops them firing off weapons they were not tasked to use.
- **Mining reuses `LayMines`.** The mission lays what it carries, up to the number ordered. With nothing to lay it fails visibly (`MissionFailed`) rather than silently.
- **Mine clearing is the transit-risk formula run the other way.** The same 1 − exp(−ρ·W·d) gives the chance to sweep a mine, with a 200 m sweep width. The sweeper meets the rest at a tenth of a normal ship's rate. This is one line of reasoning a mine-warfare officer can check by hand, and it uses the same draw stream and check period as transit risk.
- **Mine clearing never completes on its own.** The engine knows when a field is empty, but the side does not. Completing the mission would leak ground truth into the side's picture. Staff end the mission when they judge the area clear, as in a real clearance operation.
- **Denial uses the choke point's zones.** §7.1 makes a choke point a mission object. The choke point is the set of the side's zones carrying its name, so denial patrols inside them and engages only contacts believed inside. "Believed" means the track estimate, never the truth (as DL-0028). Points in polygons use area-weighted triangles from two logged draws, so every waypoint is explainable. A point outside a concave polygon is discarded rather than snapped, so the unit never patrols outside the zone.
- **Barrier patrol takes two points**, not a choke-point geometry. Without bathymetry and shipping lanes (Phase 4), the planner draws the barrier line. A future barrier tied to choke-point geometry can build on the same leg logic.
- **Cargo is tonnage.** Delivered tonnage in the AAR timeline is what a TTX needs (was the resupply delivered, and when?). Cargo types and capacities can come with the database.

## Compatibility

- **Missions:** new variants `support`, `mining`, `mine_clearing`, `cargo`, `barrier_patrol`, `denial`. Older files and saves load unchanged.
- **Save format 20:** mission phase and start tick.
- **New events:** `MissionMinesLaid`, `MissionFailed`, `MineSwept`, `CargoLoaded`, `CargoDelivered`.
- **Golden hashes:** unchanged. No shipped scenario uses the new missions, and mission progress is hashed only when used.
