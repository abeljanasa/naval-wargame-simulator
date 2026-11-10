# DL-0018: Point defence and soft kill

- **Date:** 2026-11-10
- **Phase:** 2, slice 9 ([plan](../plan/phase-2.md))
- **Sources:**
  - Master prompt §8 ("point defence gets engagement opportunities based on reaction time, range, and fire channels (saturation emerges naturally); soft kill (chaff, flares, decoys, ECM, torpedo countermeasures)").
  - Platform database decoy figures (generic; `base_ph` 0.3 used as the seduction effect).
  - Original design: deterministic detection of missiles at a 0.1 m² RCS, reaction times, channel counts, area versus self defence, head-on intercept timing, one decoy per threat with trigger ranges, and the 0.8 ECM factor.
  - No vendor product, database or binary was consulted.
- **Model doc:** [weapons-damage.md](../models/weapons-damage.md) §Point defence and soft kill

## Behaviour

- Defenders with a radiating radar detect incoming enemy missiles and, after a reaction time, engage them with SAMs (area), guns and CIWS (own ship), one per free fire channel; interceptors resolve at the intercept time with a logged Ph draw.
- Saturation emerges from the channel and reaction limits; nothing is scripted.
- Targets fire matched decoys, and a target's own radiating jammer degrades active radar seekers; both lower the threat's Ph through the countermeasure modifier.

## Compatibility

- Save format 10: point-defence state and decoys on weapons in flight are saved and hashed.
- The golden hash is unchanged (`1e874e39cf5d2dbe`): nothing fires in the demo, so the new state stays empty.
