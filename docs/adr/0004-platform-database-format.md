# ADR-0004: Platform database: JSON source of truth, SQLite build artefact

- **Status:** Accepted
- **Date:** 2026-11-10

## Context

Master prompt §9 asks for a single-file SQLite database that is versioned, migratable and **diffable**, with every record carrying provenance. A binary `.sqlite` file in git cannot be reviewed line by line. Data review is the main control against bad values and against classified data leaking into the open repository.

## Decision

1. **Source of truth:** JSON files under `data/db/open/`.
   - `reference.json`, `sensors.json`, `weapons.json`, and `platforms/*.json` by category.
   - Every file has a `defaults` provenance block, and any record can override it.
   - Values are authored in naval units (kn, nm, km) and converted to SI on build.
   - Unknown fields are rejected (`deny_unknown_fields`), so a typo cannot silently drop a value.
2. **Build artefact:** `sim-db build --out <file>` compiles the JSON into SQLite and validates it. It refuses to write the file if there are validation errors. The `.sqlite` file is never committed.
3. **Schema:** forward-only, numbered SQL migrations (`crates/sim-db/migrations/NNNN_*.sql`).
   - The version lives in SQLite's `user_version`.
   - Migrations are append-only; a merged migration is never edited.
   - Loading refuses a newer schema (`FutureSchema`) and an older one (`OutdatedSchema`, which `sim-db migrate` fixes).
4. **Validation runs twice:** in CI (`cargo run -p sim-db -- validate`), and again inside `PlatformDb::open`, which refuses a database with any error. Rule ids are stable and listed in the data dictionary.
5. **Foreign keys** are not enforced while building. Validation reports every dangling reference at once, with context, instead of failing on the first insert.
6. **Accredited data:** `DbSource::Accredited` uses the same schema and loader, but only compiles with the `accredited` cargo feature. Only the separate accredited pipeline enables that feature. The open build returns `AccreditedUnavailable`. Tests for this path use synthetic markings only.

## Consequences

- Data reviews are ordinary PR diffs.
- Building the database is a step in the build (sub-second today).
- The JSON layout is part of the format and needs its own versioning if it changes incompatibly. Today it is tied to migration 0001.
- A GUI database editor (§9) will edit the JSON source, not the built file.
