# DL-0028: Operational planning layer

- **Date:** 2026-11-11
- **Phase:** 3, slice 8 ([plan](../plan/phase-3.md))
- **Sources:**
  - Master prompt §7.1 (operational planning layer: command posts, refuelling points, sheltering ports, anchorages, choke points with sense and engage zones and per-zone ROE/WCS, surface action groups).
  - Original design:
    - zone precedence;
    - the service rates;
    - the command-post rule.
  - No vendor product, database or binary was consulted.
- **Behaviour doc:** [planning-layer.md](../models/planning-layer.md)

## Decisions

- **Zones change doctrine; they add no new rules.** A zone is a doctrine level above the unit's own. A sense zone applies to the side's units inside it; an engage zone applies to targets inside it. The existing WCS/ROE logic (`may_engage`) and EMCON handling then do the rest, for players and the AI alike. This also gives per-zone ROE/WCS with no new engagement code.
- **Engage zones look at the track's estimated position**, not the target's true position. The shooter can only know where it believes the target is. Using truth would leak ground truth into a side's decision.
- **Zone overrides unit.** In a TTX, a zone is the commander's rule for an area. A unit set weapons free does not fire into a weapons-safe zone. Lowest id wins where zones of one kind overlap: simple, and visible in the file.
- **Sense alerts go to the side's message log.** That is where players already look. `ZoneAlert` in the event log feeds the AAR timeline.
- **Command posts limit players only.** The rule models orders that cannot reach a unit, which affects players. The white cell, scenario scripts and the AI on the unit itself are not "ordering through" a post. A side with no posts is unlimited, so older scenarios behave as before.
- **Services act on units alongside** (within the radius, nearly stopped), at fixed rates:
  - repair takes 2 % of damage points a minute;
  - rearming comes after 30 minutes, once per visit;
  - berths limit how many units are served at once.

  These are planning-level rates for a TTX, not dockyard engineering. They are constants in one place, ready for database-driven facility attributes later.
- **A loadout component** (sensors, maximum speed, stores at spawn) makes repair and rearming restore what the unit started with. It also resolves the backlog item *"Repair restores sensors and speed lost to earlier criticals"*. It is derived from the spawn command, so it is not hashed.
- **Action groups are a scenario construct.** Members become escort missions on the leader at build time, reusing the mission AI that already keeps station. The core needs no group entity.
- **Hashed only when present.** The planning layer is hashed when it has content, so every existing golden hash is unchanged. The new demonstrator has its own golden.

## Compatibility

- **Scenario format 5:** `facilities`, `zones`, `action_groups`.
- **Save format 17:** planning layer and loadouts; older saves load.
- **New command:** `SetPlanning`.
- **New events:** `ZoneAlert`, `UnitRepaired`, `UnitRearmed`.
- **`sim-events`:** `Area::validate` is now public.
- **Desktop:** `SessionInfo` gains `facilities`, `zones` and `action_groups`, filtered by seat.
- **New demonstrator:** `phase3-planning-demo.json`, with its golden hash; the existing golden hashes are unchanged.
