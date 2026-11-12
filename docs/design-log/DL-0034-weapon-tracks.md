# DL-0034: Weapons in flight as sensor targets

- **Date:** 2026-11-12
- **Phase:** 3, slice 15 ([plan](../plan/phase-3.md))
- **Sources:**
  - Master prompt §7 (weapons in flight are engaged by point defence; sensors: radar, sonar, ESM, IR/EO, visual).
  - §8: missiles and torpedoes are simulated as entities; point defence gets engagement opportunities based on reaction time, range and fire channels.
  - The existing radar, optical and sonar models.
  - Original design: reference figures (0.1 m² RCS, 5 m body, 150 dB torpedo) and the track rule.
  - No vendor product, database or binary was consulted.
- **Behaviour doc:** [weapons-damage.md](../models/weapons-damage.md) §Weapons as sensor targets

## Decisions

- **Every sensor that physically could see a weapon does.** Before this slice, only a radiating radar inside point defence noticed missiles, and the side's picture never showed them.
  - Missiles are seen by radar, IR, EO and visual sensors, and by ESM when an active radar seeker is homing.
  - Torpedoes are seen by sonar.
- **No draws.** Weapons cross a sensor's range in seconds, so a per-tick probability would add noise to every golden run without adding insight. Each sensor uses a nominal range from its own model, the same way point defence already used R₅₀ against 0.1 m². A facilitator can say exactly when, and by what, a missile was seen.
- **Tracks are side state.** A side's track records when it was first and last seen, where it was, and who saw it. It is saved and hashed, and logged once as `WeaponDetected` so the AAR shows when each side first saw each incoming weapon. Tracks carry no position error, because point defence is not limited by track accuracy at these ranges.
- **Point defence also fires on its own non-radar tracks.** A ship keeping radar silent to avoid ESM can still defend itself with IR or EO. This is a real tactical choice that the earlier radar-only rule could not express. Sharing tracks across a group comes with action groups in the core (slice 16).
- **The defender's own sensors only, for now.** A track held by another ship of the side does not let a defender fire. Fire control needs the shooter's own sensor or a cooperative-engagement link, which this slice does not model. Slice 16 adds the action-group link.
- **Golden hashes unchanged.** Tracks exist only while weapons fly, and no weapon is in flight at the end of any shipped scenario. No defender in them gains a shot it did not have before.

## Compatibility

- **Save format 23:** weapon tracks.
- **New event:** `WeaponDetected`.
- **Snapshot:** `weapon_tracks`.
- **Point defence:** defenders no longer need a radiating radar if their own sensors track the threat.
