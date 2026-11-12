# DL-0036: Event forms, polygon drawing, live planning edits, day/night shading

- **Date:** 2026-11-12
- **Phase:** 3, slice 17 ([plan](../plan/phase-3.md))
- **Sources:**
  - Master prompt §7 (world and map: day/night), §10 (scenario editor with events, triggers, conditions and actions; white cell with per-zone overrides), §7.1 (the white cell can override per zone).
  - The engine's own solar model, `sim_sensors::sun_elevation_deg` (DL-0015).
  - Original design.
  - No vendor product, database or binary was consulted.
- **Behaviour doc:** [scenario-editor.md](../models/scenario-editor.md); [planning-layer.md](../models/planning-layer.md)

## Decisions

- **Event forms edit the JSON objects in place.** Each trigger, condition and action stays the plain object the engine reads, and the form only sets the fields it shows. Anything it does not cover survives untouched, such as a full mission object in an `assign_mission` action. The engine's check on every change stays the single source of truth for validity, as for the rest of the editor.
- **Lua stays text.** A form cannot describe a script. Conditions and actions of type `lua` get a script box, and the engine's syntax check reports errors.
- **Areas and routes are drawn on the map,** with the editor's existing pick mechanism. Polygon areas use the engine's `Area` (as zones do). Clicks append through the reactive draft, so each corner shows at once.
- **Live planning edits are one logged command.** A white-cell change in play sends the whole facilities-and-zones layer as one `SetPlanning`. That is attributable to the umpire seat, replays exactly, and appears in the AAR, like the per-zone weapons control override (DL-0028). Action groups are left out of the command (`None`), so a planning edit never disturbs the groups in play. The host validates the layer first, so an edit that would leave a dangling alternate post is refused with the reason.
- **Night is drawn from the engine's solar model,** so the map's terminator matches the light the optical sensors use. The terminator is computed in the browser for each minute of game time, from the scenario's start time. Only the night side (sun below the horizon) is shaded. Twilight bands would need a second, numerically solved boundary for a small visual gain.

## Compatibility

- **New desktop command:** `order_planning` (white cell only).
- **Session:** `facilities` now shows the facilities in play (with the white cell's edits), not the scenario's.
- **e2e smoke:** expects scenario format 6, and checks the event form.
