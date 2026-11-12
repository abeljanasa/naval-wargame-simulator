# Behaviour: White-cell adjudication overrides

> Phase 3 slice 1. Implements master prompt §10: "manual adjudication override (force hit/miss, adjust damage, move/add/remove units) — every override logged with umpire ID and reason".

- **Code:** `crates/sim-core/src/adjudication.rs`; desktop white-cell console in `apps/desktop/src/App.svelte`
- **Tests:** `crates/sim-core/tests/combat.rs` (adjudication section); end-to-end check in `apps/desktop/scripts/e2e-smoke.mjs`
- **Design log:** [DL-0021](../design-log/DL-0021-adjudication.md)

## The command

`Adjudicate { reason, action }`:

- It is accepted only from `Issuer::WhiteCell { umpire }`; players, the AI and the scenario are refused.
- A blank reason is refused at submission.
- Because it is a command, it is in the command log, and a replay reproduces every override exactly.
- Units are added with the existing `SpawnUnit` command, which the white cell may already issue.

| Action | Effect |
|---|---|
| `force_weapon_outcome { weapon_id, hit }` | A weapon in flight resolves now. A hit does its full warhead damage, with the usual critical, fire and flooding draws but no Ph roll. A miss removes it (`WeaponMissed: adjudicated miss`). |
| `set_damage { unit, damage_dp, clear_effects }` | Sets damage and reassesses status: mission kill, or destroyed and removed. `clear_effects` also puts out fires, stops flooding and clears critical flags (a repair). |
| `destroy_unit { unit }` | Sinks or shoots the unit down; logged `UnitDestroyed`, so it counts as a loss. |
| `remove_unit { unit }` | Takes the unit out of the exercise without it counting as destroyed. |
| `move_unit { unit, position, altitude_m? }` | Places the unit; its route is cleared. |
| `set_stores { unit, weapon, count }` | Sets one weapon's rounds; 0 removes it. The weapon must be in the catalogue. |

## The log

- Each override writes an `Adjudicated { umpire, reason, action }` event before its effects, so the event log explains everything that follows.
- Each override is also appended to the adjudication log, which is saved (format 13) and hashed.
- The white-cell picture carries the last 20 overrides; sides never see them.

## Desktop

In the white-cell seat, the panel shows a **White-cell console**:
- a reason box, which is logged and must not be blank;
- Repair, Destroy and Remove for the selected unit;
- Hit or Miss for every weapon in flight;
- the override log, newest first, with time, umpire and kind.

## Limitations

- A repair clears critical flags, but sensors lost and speed halved by earlier criticals stay lost. The original values are not kept yet (backlog).
- The umpire ID is the white-cell seat's name ([multi-seat.md](multi-seat.md)); logins come with networked play.
- Moving a unit does not re-plan its mission; the AI picks up from the new position at its next tick.
