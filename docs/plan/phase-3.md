# Phase 3 plan: TTX release 1.0 (Gate 3: navy TTX pilot on the release build)

- **Started:** 2026-11-11, on branch `phase-3/ttx-release`, after Phase 2 merged (PR #2). The Gate 2 review pack recommends going ahead with three conditions; the formal panel review has not been held yet. Anything it changes feeds back into this plan.
- **Scope (master prompt §13):** scenario editor, events + Lua, white cell, injects, adjudication override, multi-seat, replay and AAR, operational planning layer (§7.1).
- **Order:** the client chose white cell and injects first (2026-11-11): what a facilitator needs to run an exercise.
- **Gate 3 definition of done (§16):**
  - All P1 features in §7 and the planning layer in §7.1 work.
  - A white cell can run a multi-seat exercise with injects, overrides and AAR.
  - Signed installers exist for Windows, macOS and Linux, and run air-gapped.
  - CI is green and the manuals are complete.
  - A navy staff has run a TTX pilot on the release build.

## Slices

Each slice: behaviour spec, design-log entry, tests, docs, CHANGELOG, green CI (§15, §16).

| # | Slice | Delivers | Depends on |
|---|---|---|---|
| 1 ✅ | **White-cell adjudication** | Force a weapon's hit or miss, set or repair damage, destroy or remove a unit, move a unit, set stores. Every override is logged with umpire ID and reason (§10). White-cell console in the desktop | — |
| 2 ✅ | **Event engine** (`sim-events`) | Scenario events: triggers (time, area entry, unit destroyed, contact gained) → conditions → actions (message, posture, doctrine, environment, spawn, mission). Evaluated deterministically each tick, every firing logged | — |
| 3 ✅ | **Injects** | Inject library in the scenario; scheduled (by event) and manual (white cell) injects; message log per side in the desktop | 2 |
| 4 ✅ | **Lua scripting** | `mlua` (Lua 5.4) conditions and actions, sandboxed (no io/os/package/debug/load), instruction and memory limits, sandbox-escape tests | 2 |
| 5 ✅ | **Multi-seat play** | Seats per side; white-cell control of time for all seats; seat management | — |
| 6 ✅ | **Replay and AAR** | Replay scrubber from the command log, event timeline, loss tables, weapons expended vs effective, detection timeline, CSV export | — |
| 7 ✅ | **Scenario editor** | Units, sides, postures, missions, minefields, reference points, events; validation on save | 2 |
| 8 ✅ | **Planning layer (§7.1)** | Command posts, refuelling points, sheltering ports, anchorages, choke points with sense and engage zones (per-zone ROE/WCS), surface action groups | 2, 7 |
| 9 ✅ | **Gate 3 pack** | User and white-cell manuals, scripting reference, signed installers (with the client's certificates), macOS re-enabled, TTX pilot plan | all |

**After slice 9** ([Gate 3 review pack](../gate3/README.md)), three conditions remain open:
- signing, which needs the client's certificates as Actions secrets;
- the air-gapped run of the signed release build;
- the navy TTX pilot ([plan](../gate3/ttx-pilot-plan.md)).

The P1 features of §7 and §7.1 that are still partial are listed in the pack and the backlog, for the panel to accept or schedule.

Out of phase (backlog): Monte Carlo batch statistics, SVP acoustics, weather zones, air operations (Phase 4, drafted in [phase-4.md](phase-4.md)); networked multiplayer (Phase 5).

## Slices 10–17: closing the P1 gaps

The client asked (2026-11-12) to close the P1 gaps in the Gate 3 pack, in phase. These need outside data or a Phase 4 subsystem, so they move to Phase 4:

- satellite imagery (needs a licensed imagery source);
- terrain elevation and bathymetry packs (GEBCO/SRTM), and what depends on them:
  - radar terrain masking;
  - choke points that inherit depth and shipping lanes;
  - shallow-water routing;
- airbases, air operations, tanking and aircraft loadout choice.

Draft limits do not wait for bathymetry: a port's or anchorage's depth is entered by the planner and checked against the platform's draft.

| # | Slice | Delivers |
|---|---|---|
| 10 ✅ | **Comms links and disruption** | Orders reach a unit over a link from its command post: radio horizon (line of sight) unless the post has a beyond-line-of-sight link, range, jamming by another side's radiating jammer (J/S at the receiver), relay units, no reception deep submerged, comms critical hits. A jammed post is down (alternate takes over) and comes back when the jamming stops. Airborne posts gain range from altitude. Link status in the snapshot and the desktop |
| 11 ✅ | **Fuel and refuelling** | Fuel for ships and submarines from database endurance (opt-in per scenario, so older scenarios are unchanged); burn by speed (cube law); out of fuel = no propulsion. Units below a threshold on a mission go to the nearest refuelling point of their side with fuel; oilers carry moving refuelling points with a finite capacity; berths make queues. Transfer rate per point. Resume the mission when full |
| 12 ✅ | **More missions** | Support (hold station), mining (lay a field in an area), mine clearing (sweep fields of other sides, at reduced risk), cargo (load, carry, unload; delivered tonnage in the AAR), barrier patrol (back and forth across a line), denial (patrol a choke point and engage inside it) |
| 13 ✅ | **Anchorages and weather** | Anchorage and port areas as polygons; depth and holding limits (draft, sea state); units on a staging anchorage hold there between missions; weather-threatened units (sea state above their limit) shelter in port and resume when it eases |
| 14 ✅ | **Sense-zone layers and coverage** | Zones carry a layer (outer, middle, inner) and sensor types; coverage and gaps of the side's sensors over each sense zone, on demand, drawn on the map |
| 15 ✅ | **Weapons in flight as sensor targets** | Each side holds tracks on other sides' missiles and torpedoes from radar, EO/IR, visual, ESM and sonar (deterministic geometry, no new draws); the desktop shows them; point defence fires on a track held by the defender |
| 16 ✅ | **Surface action groups in the core** | Action groups as an engine object: one shared mission run by the leader, succession when the leader is lost, detach and join in play, a shared air-defence picture (point defence fires on a track held by any member) and fire deconfliction (one area-defence shot per threat at a time across the group) |
| 17 ✅ | **Desktop: editing and facilitation** | Forms for event triggers, conditions and actions; polygon drawing for zones and areas; white-cell editing of the planning layer during play; day/night shading on the map |
| 18 ✅ | **Gate 3 pack update** | Manuals, the Gate 3 pack and the backlog brought up to date |
| 19 ✅ | **Pilot scenario** | `phase3-pilot.json`: the whole planning layer in one TTX (comms jamming and handover, fuel and an oiler, mining and mine clearing, barrier, denial and cargo missions, an anchorage, dusk and a gale), with its golden replay and a test that every decision point in the pilot plan is reached |
