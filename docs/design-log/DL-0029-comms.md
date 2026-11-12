# DL-0029: Comms links and their disruption

- **Date:** 2026-11-12
- **Phase:** 3, slice 10 ([plan](../plan/phase-3.md))
- **Sources:**
  - Master prompt §7 (C2: comms links and disruption, P1), §7.1 (orders route through the post's comms links; a destroyed or jammed post hands over to its alternate), §8 (comms links with range, jamming and disruption affecting order delivery).
  - Textbook free-space propagation (inverse-square spreading) and the 4/3-earth horizon already used for radar (DL-0003).
  - Original design: the hop rule, the constants, and the post-disruption rule.
  - No vendor product, database or binary was consulted.
- **Behaviour doc:** [comms.md](../models/comms.md)

## Decisions

- **A link is a yes/no rule, not a probability.** A facilitator must be able to say why an order did not get through. Every check has a named reason, which goes into the refusal and the picture.
- **Default links are beyond line of sight.** Posts already in scenarios (format 5) commanded units out to their range regardless of the horizon. Keeping HF/satellite as the default keeps their behaviour. `line_of_sight` opts a post into VHF/UHF, and the horizon follows from antenna heights. An airborne post gains range from its altitude, which is the §7.1 airborne case with no special code.
- **Jamming reuses the jammer already on the unit.** One figure (`jn_db_at_1km`) drives both radar and comms jamming. A barrage jammer covers both bands. We avoid a second jammer type until the database carries ECM equipment (backlog). The link's 60 dB signal at 1 km is set so that a jammer much nearer the receiver than the post wins, and one at a similar distance does not. The 10 dB anti-jam default is the post's own margin, and planners can raise it.
- **Two-way hops.** An order needs an acknowledgement, so a hop is jammed if either end is. This makes jamming a post affect every unit it commands, without a separate rule.
- **A jammed post is down, and comes back.** §7.1 says a destroyed or jammed post hands over to its alternate. The post is jammed when it cannot hear a unit at the edge of its own range. The disruption is state: saved, hashed, logged as `CommandPostDisrupted`, and cleared with `CommandPostRestored`. A lost post never comes back.
- **The comms critical hit now has an effect.** A unit whose comms are knocked out cannot receive. A post on that unit is disrupted until a full repair clears the critical.
- **Submarines below 20 m cannot receive.** This is the operational reason players bring submarines shallow. VLF is out of scope.
- **Relays are named per post.** The planner chooses who relays, in order. An automatic search over every unit of the side would make results depend on unit ids the planner never thought about.
- **Pure functions in `sim-c2`.** The hop rule has hand-checked unit tests. `sim-core` only gathers positions, antennas and radiating jammers, in unit-id order, so summation order is fixed (ADR-0003).

## Compatibility

- **Facility fields:** `line_of_sight`, `antenna_m`, `anti_jam_db` and `relays` on command posts, all optional. Scenario format 5 is unchanged; older files load with the defaults.
- **Save format 18:** the new facility fields and the disrupted posts. Older saves load.
- **New events:** `CommandPostDisrupted` and `CommandPostRestored`, both in the AAR timeline.
- **Snapshot:** new `comms` and `posts` lists.
- **Refusal text:** *"is out of range of every command post"* became *"cannot be reached from {post}: out of range"*.
- **Golden:** the planning demonstrator's hash changed. Its facilities are hashed in serialised form, and that form gained the new fields. It has no jammers and no player orders, so its behaviour is unchanged. The other golden hashes are unchanged.
