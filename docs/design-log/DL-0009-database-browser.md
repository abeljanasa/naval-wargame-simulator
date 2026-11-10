# DL-0009: In-app platform database browser

- **Date:** 2026-11-10
- **Sources:**
  - Master prompt §9 (in-app database editor: search, record diffing, citation entry, validation feedback).
  - ADR-0004 (JSON sources are the system of record; SQLite is a build product).
  - The panel layout and the read-only first cut are original design.

## What it does

A collapsible **Platform database** panel in the desktop app:

- **Search** by id, name or category (case-insensitive substring), sorted by id.
- **Record detail:** category and domain, max and cruise speed, length, RCS, fitted sensors with range, and the provenance fields every record carries: source citation, confidence and classification marking.
- **Validation feedback:** the panel re-runs the 34-rule suite on the loaded database and shows the warning count; a record's own warnings are listed under it. Errors cannot appear: a database with errors refuses to open.

Backend: three Tauri commands, `db_search`, `db_platform` and `db_issues`, over the embedded open database the host already holds. They only read; the simulation is not touched (design principle 3).

## Why read-only first

ADR-0004 makes the JSON files the system of record. An editor that wrote SQLite would fork the data; one that writes JSON needs a source path, file permissions in the app, record diffing and a save/validate/rebuild cycle. That is a self-contained piece of Phase 2 work (backlog). The browser covers what Gate 1 reviewers need: finding a class and checking where its figures come from.

## Tests

- `host::tests::platform_database_is_browsable_with_provenance`: every platform is listed, GEN-FFG has a citation and an UNCLASSIFIED marking, an unknown id returns none, and the open database has no validation errors.
- End-to-end smoke (`npm run e2e`): search filters the list; opening a record shows its citation and its validation warning.
