# DL-0019: Basic missions and automated tasking

- **Date:** 2026-11-10
- **Phase:** 2, slice 10 ([plan](../plan/phase-2.md))
- **Sources:**
  - Master prompt §8 and §13 ("basic missions"), §7 ("Missions and AI: patrol, strike, support, escort, ferry …; automated tasking"), §6.2 (AI every 10–30 s).
  - Original design: the four basic missions and their steering rules, uniform-disc patrol points, escort station keeping, nearest-first automated engagement through the ordinary `Engage` checks, and the 10 s AI cadence.
  - No vendor product, database or binary was consulted.
- **Model doc:** [missions.md](../models/missions.md); architecture [ADR-0005](../adr/0005-model-crates.md) (new `sim-ai` crate)

## Behaviour

- Units can patrol an area, ferry to a point, escort another unit or strike a contact. Missions come from the scenario, the white cell or the unit's own player.
- Tasked units engage automatically, nearest contact first, with the first weapon that passes every check; the release rule binds the AI exactly as it binds players.
- The desktop shows each unit's mission and offers "Patrol here" and "Clear".

## Compatibility

- Save format 11. New command `AssignMission`; optional `mission` on scenario units.
- Golden hash `1e874e39cf5d2dbe` → `b296e935e3375df2` (missions are hashed; the demo assigns none).
- Benchmarks: 1,000 units 343×, 10,000 units 3.8× (budgets 60× and 1×).
