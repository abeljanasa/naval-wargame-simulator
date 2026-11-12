# DL-0035: Surface action groups in the core

- **Date:** 2026-11-12
- **Phase:** 3, slice 16 ([plan](../plan/phase-3.md))
- **Sources:**
  - Master prompt §7.1: a SAG moves and fights as one, with a shared picture, coordinated air defence and a single mission; ships can be detached or added in play.
  - Original design: the succession rule and the deconfliction rule.
  - No vendor product, database or binary was consulted.
- **Behaviour doc:** [planning-layer.md](../models/planning-layer.md) §Surface action groups

## Decisions

- **The group lives in the engine.** DL-0028 built groups as escort missions at scenario build time, and the core never knew a group existed. A single mission, succession, and detaching and joining in play all need the engine to know who is in which group. Groups therefore move into the planning layer, and the scenario format is unchanged. `ActionGroup` now lives in `sim-core` and is re-exported by `sim-scenario`.
- **Stations are enforced every AI period.** A member that is not diverted keeps (or regains) its escort mission on the current leader. This is what makes succession and joining work with no special cases. It also means a member cannot be given a separate mission while it is in the group: it must detach first. That matches "moves and fights as one".
- **Succession follows list order.** The first member becomes leader, which the planner controls by ordering the members. The group's mission passes with it, so the group carries on rather than stopping.
- **The group's mission is an order to the leader.** It passes through command posts and comms like any order to the leader, so a SAG beyond its post's link cannot be retasked.
- **Shared picture for air defence only.** §7.1's shared picture is, in practice, the force's air picture for self-defence. Members fire on a missile any member tracks this tick. This is a cooperative engagement within the group. Sharing contact tracks generally is already side-wide, and data links with comms effects are Phase 5.
- **Deconfliction is one SAM per threat at a time.** It is the simplest rule a warfare officer recognises. It keeps channels free against a raid, still allows shoot-look-shoot (a second shot after the first resolves), and leaves guns and CIWS as each ship's last line.

## Compatibility

- **`SetPlanning`:** gains `action_groups` (optional; omitted keeps the groups in play). The desktop's zone-rule override sends none, so it never disturbs the groups.
- **New commands:** `AssignGroupMission`, `DetachFromGroup`, `JoinGroup`.
- **New events:** `GroupLeaderChanged`, `GroupDissolved`, `UnitDetached`, `UnitJoined`.
- **Save format 24:** groups in the planning layer.
- **Snapshot:** `action_groups`.
- **Golden:** the planning demonstrator, which has a SAG, changes hash: groups are now part of the hashed planning layer. Its member already kept station through the same escort mission. Its AAR timeline was checked: 456 entries as before, plus the 16 weapon-detection notes slice 15 adds, with the same 2 losses.
