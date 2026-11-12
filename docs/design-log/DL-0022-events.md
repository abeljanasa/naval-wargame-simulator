# DL-0022: Scenario event engine

- **Date:** 2026-11-11
- **Phase:** 3, slice 2 ([plan](../plan/phase-3.md))
- **Sources:**
  - Master prompt §7 ("events: triggers → conditions → actions") and §13 (Phase 3 scope).
  - Original design:
    - the trigger set (time, every, area entry, unit destroyed, contact gained);
    - AND conditions with a lazily drawn probability;
    - actions as script commands with the scenario's authority;
    - reserve units;
    - per-side message logs.
  - No vendor product, database or binary was consulted.
- **Behaviour doc:** [events.md](../models/events.md)

## Decisions

- **A pure model crate (`sim-events`)**, as ADR-0005 requires: definitions, trigger and condition logic, and validation, with no ECS. `sim-core` gathers each tick's facts and carries out the actions.
- **Events are loaded by a command (`SetEvents`)**, so they are in the command log, and a replay or save rebuilds them exactly.
- **Actions reuse the command path** (`Issuer::Script`, now privileged like the scenario). Every rule a command obeys also binds an event: validation, land avoidance and authority.
- **Reserve units replace inline spawns in actions.** The scenario resolves them against the platform database at build time, like any unit, so actions stay small and replays stay independent of later database edits.
- **Edge-triggered** time and area triggers: an event repeats only when its trigger occurs again, never on every tick that a state holds.

## Compatibility

- Save format 14 (older saves load with no events and no messages).
- Scenario format 3 (v1 and v2 load).
- Golden hashes change because event state and messages are hashed. No scenario's behaviour changed:
  - phase 1 `897a23ff0e619af2` → `dd64421cfad317f2`;
  - phase 2 `1b01a1518913d807` → `3930306360d8e447`;
  - new phase 3 demonstrator `fbd8dfd736e8745d`.
