# DL-0017: Probability of hit and damage

- **Date:** 2026-11-10
- **Phase:** 2, slice 8 ([plan](../plan/phase-2.md))
- **Sources:**
  - Master prompt §8 ("Ph and damage: probability of hit from base value × modifiers (target size, manoeuvre, countermeasures, environment); damage points plus critical hits on sensors, propulsion, weapons, flight deck, comms; fire and flooding progression with damage control; mission-kill distinct from loss").
  - Platform database damage profiles (damage points, mission-kill fraction, fire and flooding susceptibility).
  - Original design: the modifier tables, 1 DP per kg of warhead, the 0.25 critical chance and equal-fifths critical table, fire and flooding start chances, burn and flood rates, and damage-control probabilities.
  - No vendor product, database or binary was consulted.
- **Model doc:** [weapons-damage.md](../models/weapons-damage.md)

## Behaviour

- Each arriving weapon rolls Ph = base × size × manoeuvre × environment; every modifier and the draw are logged.
- Hits do damage, may cause a critical hit (with an effect on the unit), and may start fires or flooding that cost damage every minute until damage control wins.
- Units become mission-killed (afloat but unable to fight) or destroyed (removed).
- Gate 2 duel test: 300 one-missile engagements match the hand-calculated 0.72 hit rate within 4σ.

## Compatibility

- Save format 9: magazines, health, the weapon catalogue and weapons in flight are saved and hashed.
- Golden hash `a6d33e91f79e97ea` → `1e874e39cf5d2dbe`: units now carry stores and damage state. Nothing fires in the demo (Blue is weapons safe).
- Benchmarks: 1,000 units 339×, 10,000 units 3.9× (budgets 60× and 1×).
