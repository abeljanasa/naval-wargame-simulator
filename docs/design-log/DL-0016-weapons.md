# DL-0016: Weapons as entities and engagements

- **Date:** 2026-11-10
- **Phase:** 2, slice 7 ([plan](../plan/phase-2.md))
- **Sources:**
  - Master prompt §8 ("Engagements: firing requires a track of the quality the weapon needs …; missiles/torpedoes simulated as entities with seeker acquisition").
  - Platform database weapon figures (generic, confidence D).
  - Original design: the guidance groups and their track-quality limits and seeker ranges, the 0.9 acquisition probability, midcourse updates from the side's track, the rule that semi-active and command weapons need the track held to impact, 1.1× fuel, and resolving guns, CIWS, bombs, depth charges and rockets at once.
  - No vendor product, database or binary was consulted.
- **Model doc:** [weapons-damage.md](../models/weapons-damage.md); architecture [ADR-0005](../adr/0005-model-crates.md) (new `sim-weapons` crate)

## Behaviour

- `Engage` checks condition, magazine, domain, track quality, range and the WCS/ROE release rule, and costs nothing if refused. The white cell may override WCS.
- Missiles and torpedoes fly toward the side's current track estimate, get one seeker draw when the real target is in seeker range, and home or fly on and run out of fuel.
- The desktop has an Engage form (target, weapon, rounds), shows weapons in flight on the map, and shows why an order was refused.

## Compatibility

- Save format 9 (weapons and damage). New commands `SetWeaponCatalog` and `Engage`.
- The scenario loader fills each unit's stores from its database magazines (or first loadout).
