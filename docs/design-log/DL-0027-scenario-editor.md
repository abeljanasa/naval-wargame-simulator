# DL-0027: Scenario editor

- **Date:** 2026-11-11
- **Phase:** 3, slice 7 ([plan](../plan/phase-3.md))
- **Sources:**
  - Master prompt §13 (scenario editor: units, sides, postures, missions, minefields, reference points, events; validation on save) and §7.
  - Original design.
  - No vendor product, database or binary was consulted.
- **Behaviour doc:** [scenario-editor.md](../models/scenario-editor.md)

## Decisions

- **The engine is the only validator.** The editor never decides validity itself. It sends the draft to `editor_check`, which runs the loader's structural checks, platform resolution, minefield mine types and a trial build. Save and Play use the same check, so a file the editor saves always loads.
- **Every issue at once.** `Scenario::validate` (used when loading) stops at the first problem; the editor needs the whole list. The checks were rewritten to collect issues, with `validate` returning the first, so the two cannot drift apart.
- **Warnings are separate from errors.** A side with no units, duplicate names, or a ship placed on land are probably mistakes, but they are legal and are sometimes intended.
- **Forms where they pay, JSON where they do not (yet).**
  - The things set up in every scenario (sides, postures, units, missions, minefields, points) have forms and map picking.
  - Events, injects, seats and groups are nested and varied, so they are edited as JSON, with templates and the engine's checks.
  - A template file shared by the UI and a Rust test guarantees the templates are valid events.
- **The draft is plain JSON** in the UI (`DraftScenario`), not the generated binding types. Unknown fields pass through unchanged, so editing a file never drops what the forms do not show.
- **The map shows the draft while editing,** with the live picture hidden. This avoids confusing draft and live units, and lets map clicks place things.
- **Reference points (format 4)** are scenario data with no simulation effect. They travel in `SessionInfo`, filtered by seat, rather than in every frame.
- **Typed scenario bindings:** `sim-scenario` gains a `specta` feature, so `editor_current` returns a typed scenario.

## Compatibility

- **Scenario format 4** (`reference_points`); formats 1–3 still load.
- **New public API:**
  - `Scenario::check`;
  - `ScenarioIssue`, `Severity`, `ReferencePoint`;
  - `PlatformDb::weapons`.
- **Desktop:**
  - New commands: `editor_check`, `editor_current`, `db_weapons`.
  - `SessionInfo` gains `reference_points`.
- Save format and golden hashes are unchanged.
