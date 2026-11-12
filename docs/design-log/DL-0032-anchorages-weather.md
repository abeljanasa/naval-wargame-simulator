# DL-0032: Anchorage areas, depth and holding limits, staging, sheltering from weather

- **Date:** 2026-11-12
- **Phase:** 3, slice 13 ([plan](../plan/phase-3.md))
- **Sources:**
  - Master prompt §7.1:
    - sheltering ports: draft limit; damaged or weather-threatened units divert there;
    - anchorage areas: polygons with depth and holding limits; units hold there between missions; staging for sorties and convoys.
  - Platform database field `draft_m`.
  - Original design: weather limits by length, under-keel clearance, staging rule.
  - No vendor product, database or binary was consulted.
- **Behaviour doc:** [planning-layer.md](../models/planning-layer.md)

## Decisions

- **Depth is the planner's number, not bathymetry.** §7.1's draft limit does not need a depth grid. A planner knows a port's or anchorage's depth and enters it. Bathymetry (GEBCO, Phase 4) can later supply a default for the same field without changing the rule.
- **One metre of under-keel clearance** is a generic planning margin. It is a constant in one place.
- **Holding limit as a maximum sea state.** "Holding" in §7.1 is how well an anchorage holds a ship in bad weather. With one scenario-wide sea state, a facility's limit is the highest sea state units stay in. Above it, units do not go there, and staging units leave to shelter.
- **Weather limits by length:** sea state 5, 7 and 8 for under 60 m, 60–120 m and over. Staff can reason about this rule, and it uses a figure every unit has. It is a function in one place, ready to become a platform-database field.
- **Only AI-run units divert,** as for repair and fuel (DL-0028, DL-0030). Players see the sea state and decide for their own ships.
- **Staging means "mission complete, hold here".** §7.1 says units hold at anchorages between missions. A unit whose mission is complete goes to the nearest staging anchorage it fits, and stays until tasked again. Its completed mission is left in place, so the picture still shows what it last did.
- **A new mission ends any diversion.** Before this slice, a mission given to a unit on its way to a repair port was overwritten when the old mission resumed. Assigning a mission now clears the diversion, so the newest order wins, as staff expect.
- **Resuming plans afresh.** A unit leaving a port or an oiler clears its route and speed, so its mission sets them again.
- **Areas reuse the event engine's `Area`** (circles and polygons, already used by zones). The facility's point stays the place units steer for, which keeps routes deterministic. The scenario check warns when that point is outside the drawn area.
- **Draft is not hashed.** Like the loadout (DL-0028), it comes from the spawn command and never changes. Adding it to the hash would change every golden hash for no information.

## Compatibility

- **Facility fields:** `area`, `depth_m`, `max_sea_state`, `staging`, all optional. Older scenarios load unchanged.
- **Spawn:** `draft_m`. Scenarios fill it from the platform database.
- **Save format 21.**
- **New events:** `ShelteringFromWeather`, `WeatherEased`, `HoldingAtAnchorage`.
- **Golden:** the planning demonstrator's hash changes, because its facilities hash with the new fields. It has no diversions, so its behaviour is unchanged. The other golden hashes are unchanged.
