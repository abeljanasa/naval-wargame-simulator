# DL-0005: Platform database (schema, migrations, validation, generic types)

- **Date:** 2026-11-10
- **Sources:**
  - Master prompt §3 (data security) and §9 (platform database): tables, provenance columns, validation checks, the `DbSource::Accredited` extension point, and the list of Phase 1 generic categories.
  - Schema layout, JSON authoring format, rule set and confidence scale: original design ([ADR-0004](../adr/0004-platform-database-format.md), [data dictionary](../db/data-dictionary.md)).
  - Generic values: representative open-literature class ranges, confidence D. No vendor database was consulted (clean-room rule 2).
- **Code:** `crates/sim-db`
- **Data:** `data/db/open`
