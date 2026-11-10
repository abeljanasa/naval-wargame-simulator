# DL-0021: White-cell adjudication overrides

- **Date:** 2026-11-11
- **Phase:** 3, slice 1 ([plan](../plan/phase-3.md))
- **Sources:**
  - Master prompt §10 (white-cell console: "manual adjudication override (force hit/miss, adjust damage, move/add/remove units) — every override logged with umpire ID and reason").
  - Original design: one `Adjudicate` command with six actions, white-cell-only authority, a mandatory reason, the record written before the effects, and the last-20 view.
  - No vendor product, database or binary was consulted.
- **Behaviour doc:** [adjudication.md](../models/adjudication.md)

## Behaviour

- The white cell can force a weapon in flight to hit or miss, set or repair damage, destroy or remove a unit, move it, and set its stores.
- Every override carries the umpire and a reason, is logged before it takes effect, is saved, and replays exactly.
- The desktop's white-cell console offers these with a reason box and shows the override log.

## Compatibility

- Save format 13 (older saves load with an empty log).
- Golden hashes: phase 1 `1f07e67a956cb372` → `897a23ff0e619af2`; phase 2 `4787a52f5ddf3427` → `1b01a1518913d807` (the log is hashed; neither scenario adjudicates).
