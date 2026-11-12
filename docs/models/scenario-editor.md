# Behaviour: Scenario editor

> Phase 3 slice 7. Master prompt §13 (scenario editor) and §7. Original design.

- **Code:**
  - `crates/sim-scenario/src/lib.rs` (`Scenario::check`, reference points, format 4)
  - `apps/desktop/src-tauri/src/lib.rs` (`editor_check`, `editor_current`, `db_weapons`)
  - `apps/desktop/src/lib/ScenarioEditor.svelte`, `editor.svelte.ts`, `scenario-edit.ts`
  - `apps/desktop/src/lib/map.ts` (`editorLayers`, `referenceLayers`)
  - `apps/desktop/src/lib/event-templates.json`
- **Tests:**
  - `sim-scenario` (`check_lists_every_error_at_once_errors_first`, `a_clean_scenario_has_only_warnings_and_builds`, `reference_points_load_round_trip_and_are_checked`)
  - desktop (`editor_check_reports_parse_errors_and_every_issue`, `event_templates_are_valid`)
  - Vitest `scenario-edit.test.ts`
  - e2e: new scenario, place a unit, add an event, check, play
- **Design log:** [DL-0027](../design-log/DL-0027-scenario-editor.md)

## Opening

The editor is offered to windows that control time (the control station and white-cell seats), under **Scenario**:

- **Edit this scenario:** the scenario now loaded, as the engine holds it.
- **New scenario:** two sides (Blue and Red, hostile to each other), no units, four hours.
- **Edit a scenario file:** any `.json` file. The webview reads it; nothing is written to disk by the app.

While the editor is open, the map shows the draft instead of the live picture:
- units as dots in their side's colour (reserves faint), with names;
- routes, patrol areas and ferry legs;
- minefields;
- reference points.

Clicking a unit selects it. The running exercise is not affected until **Play**.

## Tabs

| Tab | Edits |
|---|---|
| **Overview** | Name, description, seed, duration, start time (UTC), sea state, weather loss, visibility, the fuel model ([fuel.md](fuel.md)) |
| **Sides** | Names; how each side regards every other (friendly, neutral, unfriendly, hostile; neutral is not written); add a side; remove a side, with its units, minefields, points and the postures toward it |
| **Units** | List; **Add unit on the map**; per unit: name, side, platform (from the database), position (typed or **Move on the map**), heading, speed in knots (blank: the platform's cruise speed), starting fuel (with the fuel model), **Draw route** (click waypoints, then Done), mission (patrol, ferry, escort, strike, support, mining, mine clearing, cargo, barrier patrol, denial: [missions.md](missions.md)), reserve; Duplicate, Delete |
| **Mines** | Side, mine type (database weapons of kind `mine`), number of mines, radius, centre on the map |
| **Points** | Reference points: name, position on the map, shown to every side or to one side |
| **Planning** | Facilities (command-post links, relays and subordinates; refuelling carrier, stock and rate; depth, holding limit, staging; **Draw area**), zones (layer, sensor types, **Draw polygon** or circle), surface action groups ([planning-layer.md](planning-layer.md)) |
| **Events** | **Forms** for each event (slice 17): id, name, on at the start, repeatable; the trigger (time, every, area entered with a circle or a drawn polygon, unit destroyed, contact gained); conditions (event fired or not, unit alive or gone, units alive, time window, probability, Lua); actions (message, inject, posture, doctrine, weather, speed, route drawn on the map, patrol or clear a mission, reserve, turn an event on or off, Lua). **Add an event** blank or from a template. Injects, seats and groups as JSON (format in [events.md](events.md), [injects.md](injects.md), [multi-seat.md](multi-seat.md)) |

- Positions picked on the map are rounded to 10⁻⁵ degree (about a metre).
- Fields the forms do not cover (unit doctrine, jammers, inline performance overrides, side doctrine) are kept unchanged from the file.

## Validation

**Check** (and every Save and Play) sends the draft to the engine, which returns every problem at once: errors first, then warnings.

**Errors** (block Save and Play):
- Not JSON, not a scenario, or a newer format.
- Everything the loader refuses:
  - duplicate side or unit ids;
  - undeclared sides or groups;
  - bad postures, minefields, environment, seats or reference points;
  - events naming undeclared sides or units, or spawning a unit that is not a reserve;
  - script syntax errors;
  - a duration of zero.
- A platform that is not in the database, or that contradicts the unit's domain.
- A minefield weapon that is not a mine in the database.
- Anything the trial build refuses (the commands' own checks).

**Warnings** (allowed):
- A side that starts with no units.
- Two units with the same name.
- A ship or submarine that starts on land (against the bundled coastline).

**Save file** writes the draft as formatted JSON at the current format (6), named after the scenario. **Play** loads it into the session, replacing the running exercise (as loading a file does), and closes the editor.

## Scenario format 4

Adds `reference_points`, a list of:
- `name`: unique, not blank;
- `position`;
- `side` (optional): shown only to that side and the white cell.

Reference points have no effect on the simulation. Seat windows show the points their seat may see. Format 1–3 files still load.

## Limitations

- Injects, seats and groups are edited as JSON, with the engine's checks. Event fields the forms do not cover (an event action's full mission set, group doctrine targets) are kept as written.
- No undo; reopen the file or **Edit this scenario** to start again.
- Unit doctrine, jammers and inline performance overrides are kept, but have no form.
