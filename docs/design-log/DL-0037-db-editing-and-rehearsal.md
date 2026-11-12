# DL-0037: Database record editing, symbol function codes, and fixes from the pilot rehearsal

- **Date:** 2026-11-12
- **Phase:** 3 (after slice 20)
- **Sources:**
  - Master prompt §9 (in-app database editor with record diffing, citation entry and validation feedback), §10 (APP-6-style symbology), §15.
  - ADR-0004 (the JSON source is the truth; a GUI editor edits it).
  - Public APP-6 / MIL-STD-2525 conventions and hull abbreviations ([symbology spec](../behaviour-specs/symbology.md)).
  - Original design otherwise. No vendor product, database or binary was consulted.

## Decisions

### Database record editing

- **The editor edits the JSON source, not the built database** (ADR-0004).
  - A platform record is found by id and replaced as text (`sim_db::edit`), so the rest of its file keeps its layout and a review sees a one-record diff.
  - Provenance inputs (citation, confidence, date) edit only the record's `provenance` object, written on one line.
  - Re-serialising the record would reformat it and bury the change in a whole-record diff.
- **Checks are the real ones.** Every edit is built in memory with the whole database and run through the same validation suite as `sim-db validate`, about 300 ms after typing stops.
  - A record that does not load (bad JSON, an unknown field, a wrong type) shows the loader's message, with its line.
  - Validation errors anywhere are shown, plus warnings on the edited record. Saving is refused while there are errors.
- **Saving hands over the file.** The app has no filesystem access (scenarios and records are also saved as downloads). *Save to source file* downloads the whole edited source file, which goes back into `data/db/open/` through ordinary review.
  - The running app keeps its built-in database until it is rebuilt with the file. The editor says so, because silently changing the database of a running exercise would change its results.
- **Scope:** platform records, which are where planners make most edits. Sensors and weapons are still edited in the JSON files and checked with `sim-db validate`.

### Symbol function codes

- **A text code inside the frame,** following APP-6's practice of short hull or role abbreviations for sea units and role letters for air.
- **The code shows only what the side knows:** the class once identified, else the category, else none. A surface combatant of unknown type shows no code, because no code is better than a wrong one.
- **Symbols grew** from 28 to 36 px (units) and from 24 to 30 px (contacts), so the codes are readable.

### Fixes found by rehearsing the run sheet in the packaged app

- **White-cell actions while paused.** A command applies when its tick runs, so injects and overrides sent during a pause did nothing until the clock ran. The host now runs one second when the white cell acts while paused. Player orders still wait, as in any paused wargame. Records and replays are unchanged, because this is the next tick of the run.
- **Map framing.** The map opened at a fixed zoom. It now frames the units a seat sees, once per scenario and seat. A seat change drops the old picture and asks for the new one in full.
- **Keys typed into text areas** reached the run/pause and compression shortcuts. A space paused the clock, a digit changed the speed, and the character was lost. Shortcuts now ignore every text entry.
- **The review scrubber** overflowed its panel.

## Tests

- `sim-db` `edit` tests:
  - spans ignore braces inside strings;
  - a record is found as authored;
  - an edit changes only its record and checks clean;
  - bad JSON, a non-record, a missing id, an unknown field and a dangling reference are each refused or reported.
- Desktop `db_edit_tests`, and `ui.test.ts` / `db-edit.test.ts`:
  - codes by class and category, and every generic class has one;
  - line diff;
  - provenance editing in place.
- Host test: `white_cell_actions_take_effect_while_paused_and_player_orders_wait`.
- Packaged-app test (`npm run e2e:app`), covering the record editor and the white-cell run sheet:
  - **record editor:** clean edit with diff, typing keeps every character, a load error blocks saving;
  - **run sheet:** tabs, events, inject, command posts, seat window, override log, replay.
