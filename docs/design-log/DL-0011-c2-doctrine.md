# DL-0011: Postures, doctrine inheritance, WCS, EMCON and ROE

- **Date:** 2026-11-10
- **Phase:** 2, slice 2 ([plan](../plan/phase-2.md))
- **Sources:**
  - Master prompt §8 ("C2: sides and postures (friendly/neutral/unfriendly/hostile); doctrine inheritance side → group → unit; ROE; WCS (hold/tight/free); EMCON states").
  - Common, publicly documented naval usage of weapons control status. At the client's direction (2026-11-10) the states are named safe, tight and free; the master prompt's "hold" is accepted as an alias for safe.
  - The release-rule table, the "identified to class level" condition for knowing affiliation, the defaults and the two-state EMCON are original design.
  - No vendor product, database or binary was consulted.
- **Model doc:** [c2-doctrine.md](../models/c2-doctrine.md)

## Behaviour

- Sides hold one-directional postures toward each other; the scenario and white cell set them.
- Doctrine (WCS, EMCON, engage-unfriendly ROE) resolves per field: unit, group, side, default.
- EMCON silent switches a unit's radars off: no detections, no draws.
- The weapons release rule is a tested pure function, ready for the engagement model (slice 7).
- The desktop shows each own unit's doctrine and lets the player set WCS and EMCON per unit.
- Contacts carry an affiliation once identified to class level. Radar alone never gets there, so in this slice contacts stay "unknown".

## Compatibility

- Save format 4; older saves load with no postures and default doctrine.
- Scenario format: optional `postures`, `doctrine`, `groups`, unit `group`. Existing files load unchanged.
- The demo scenario now states its peacetime set-up: both sides neutral, Blue weapons safe.
- Golden hash `8a1f96e397c0a0b7` → `d46bae3f7ea350a8`: doctrine is part of the state hash. Detection results are unchanged (radars stay active).
