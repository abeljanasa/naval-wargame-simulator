# Model: Basic missions and automated tasking

> Phase 2 slice 10. Original design; **not SME-validated**. Basic by intent: the full mission set (support, mining, mine-clearing, cargo), tactics and air operations come later (§7, Phase 3–4).

- **Code:** `crates/sim-ai` (model), `crates/sim-core/src/ai.rs` (AI system)
- **Tests:** unit tests in `sim-ai` (hand values); engine tests in `crates/sim-core/tests/combat.rs` (missions section)
- **Design log:** [DL-0019](../design-log/DL-0019-missions.md)

## Missions

A unit carries at most one mission, set by the scenario (`mission` on a unit), the white cell or its own side's player (`AssignMission`; `None` clears it).

| Mission | Behaviour, every 10 s |
|---|---|
| **Patrol** (centre, radius) | When its route is done, pick a point uniformly over the circle: bearing 360° × u₁, distance radius × √u₂ (two `Ai` draws, logged as `MissionWaypoint`). Moves at half speed if it was stopped. |
| **Ferry** (destination) | Steer to the destination; within 1 km the mission is complete (`MissionComplete`). |
| **Escort** (escorted unit, bearing off its heading, range) | Steer to the station; full speed when more than 500 m off station, else the escorted unit's speed. |
| **Strike** (contact) | Steer to the side's estimate of the contact; engage only that contact. |

Routes are planned round land like any other order. A mission-killed or destroyed unit does nothing.

## Automated engagement

After moving, a tasked unit engages:
- **Target:** the nearest of its side's contacts (a strike engages only its target) that no weapon of the side is already flying at.
- **Weapon:** the first carried weapon, in id order, that passes every `Engage` check: domain, track quality, range and the WCS/ROE release rule ([c2-doctrine.md](c2-doctrine.md), [weapons-damage.md](weapons-damage.md)).
- **Rate:** one round, at most one engagement per unit per AI tick. Logged as `MissionEngage`, followed by the launch or hit roll.

The AI uses the same checks as a player, with `Issuer::Ai`, so it can never break the release rule: under weapons tight it will not fire at a track it has not identified as hostile.

## Determinism

The AI runs after detection on ticks divisible by 10, units in id order. Its only randomness is the patrol draws on the `Ai` stream. Missions are saved and hashed (save format 11).

## Worked examples

| Case | Working | Result |
|---|---|---|
| Patrol point, u₁ = 0.25, u₂ = 0.25, radius 20 km | bearing 90°, 20 × √0.25 | **10 km due east** of the centre |
| Escort station, escorted heading 090°, bearing 45°, 2 km | 090 + 45 | station **2 km on bearing 135°** |
| Weapons-free patrol, contact 30 km, carries an ASM | nearest contact, first passing weapon | **one ASM**, none more while it flies |
| Weapons-tight strike on an unidentified contact | release rule refuses | **no engagement** |

## Verification

| Test | Checks |
|---|---|
| `patrol_points_hand_values`, `escort_station_and_speed`, `nearest_target_first_with_stable_ties`, `mission_json_and_validation` | model hand values |
| `a_patrol_picks_points_inside_its_area_and_keeps_moving` | engine: points inside the circle; unit moving |
| `a_ferry_completes_on_arrival_and_an_escort_holds_station` | engine: ferry complete; escort within 600 m of station |
| `automated_engagement_follows_the_release_rule` | engine: one ASM under weapons free; none under weapons tight |
| `players_task_only_their_own_units` | authorisation and validation |

## Limitations

- No mission planning (timing, coordination, return to base), no evasion or tactics, no formations beyond single-unit escort stations.
- Target choice is nearest-first, one round at a time; no salvo sizing or weapon-target pairing by Ph.
- Patrols are circles; mission areas as polygons come with the scenario editor (Phase 3).
