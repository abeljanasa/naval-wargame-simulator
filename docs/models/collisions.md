# Model: Ship collisions and collision avoidance

> Added 2026-11-13. Original design on public seamanship and ship-collision literature, with generic values; **not SME-validated**. Before this, units were points that passed through each other.

- **Code:**
  - `crates/sim-core/src/collision.rs`: hulls, the rules of the road, avoidance, contact, damage.
  - `crates/sim-core/src/movement.rs`: avoidance steering and contact detection each tick.
  - `crates/sim-core/src/combat.rs`: `apply_damage`, shared with weapon hits.
- **Tests:**
  - Unit tests in `collision.rs` (hand values, rules).
  - Engine tests in `crates/sim-scenario/tests/collisions.rs`.
  - AAR: `crates/sim-aar/tests/collisions.rs`.
- **Design log:** [DL-0045](../design-log/DL-0045-collisions.md)

## Hulls

Every ship and submarine is a **hull**; aircraft and land units are not.

| Property | Source |
|---|---|
| Length | the platform's length, else 100 m (ship) or 70 m (submarine) |
| Mass | the platform's full-load displacement, else 0.0025·L³ t (within about 30 % of frigates, cruisers and carriers) |
| Depth band, ship | 30 m above the water (superstructure) to its draft (6 m if none) |
| Depth band, submarine | 10 m above its depth (the sail) to 6 m below (the keel) |

Two hulls can meet only if their depth bands overlap. A submarine at periscope depth can be run down by a deep-draft ship; one at 200 m passes under everything.

## Collision

- **When:** two hulls whose depth bands overlap come within **half their combined lengths**, having been further apart at the start of the tick. Hulls already touching do not collide again until they have separated.
- **Both stop:** speed and ordered speed go to zero. A new order gets them under way again.
- **Damage:** each hull loses a share of its damage points:

  f = (m_other / (m_self + m_other))² · (v / 14 m/s)²

  Here v is the closing speed, the magnitude of the difference of the two velocities. f is the hull's share of the collision energy (split by the other's mass) over its own kinetic energy at 14 m/s (about 27 kn). So a light hull struck by a heavy one takes nearly all of it, and an equal pair each take a quarter at 27 kn.
- **The damage roll** is the weapon hit's: damage points, then the critical, system, fire and flooding draws in order.
  - With f ≥ 0.1, the hull is holed below the waterline and floods as from a torpedo.
  - Below f = 0.01 it is a dent: no damage roll.
- **Logging:** each collision is logged (`Collision`), with each hull's share. Its damage follows as `DamageTaken`. The AAR timeline names both hulls and the closing speed. A hull sunk by a collision is lost to it ("collision with …"), not credited to any side or weapon.

## Collision avoidance

Each tick, before it moves, every hull under way (above 0.1 m/s) looks at the hulls it knows about within its depth band:

- **What a hull knows about:**
  - A ship knows every ship, and its own side's submarines (water-space management). It does not know another side's submerged boat, which it can run down.
  - A submarine knows every hull (it hears them).
- **The closest point of approach** is worked out on the hull's intended course: its next waypoint, or the heading it was ordered to hold. It is a **threat** if it comes within 360 s and closer than the safe passing distance. That distance is the greater of 500 m and twice the two hulls' combined lengths.
- **The rules of the road** (COLREGS rules 13–15, simplified) decide who gives way:
  - a hull gives way to a stopped hull (under 0.5 m/s);
  - the overtaking hull, coming from more than 22.5° abaft the other's beam, gives way;
  - head-on (courses within 10° of reciprocal, dead ahead), both give way;
  - crossing, the hull with the other on its starboard side gives way; the other stands on.
- **Close quarters:** a threat within 90 s makes every hull act, stand-on or not (rule 17).
- **The give-way action:** the smallest turn to starboard, in 10° steps up to 90°, that clears every known threat. If none does, it turns 90° and slows to a quarter of its top speed.
- **Hysteresis:** a hull keeps giving way to the same hull until the passage is safe by 25 % more than the safe distance, so it does not start and stop as the geometry wavers at a rule's boundary.
- **Afterwards:** once clear, the hull returns to its route, or comes back to the heading it was ordered to hold.
- **Logging:** each start of giving way is logged (`CollisionAvoidance`). It is routine, so it is not raised as a warning to the seat; the course change shows on the map. A collision is raised to each seat whose ship was in it.

The rules apply between all hulls, own side included.

## Worked examples

| Case | Working | Result |
|---|---|---|
| USS *Fitzgerald* (8,300 t) and ACX *Crystal* (29,000 t), about 13 m/s closing | destroyer: (29,000/37,300)² × (13/14)² | **0.52**: a mission kill. The container ship: **0.04** |
| Frigate (5,000 t) at 10 m/s on a hovering SSK (2,500 t) | boat: (5,000/7,500)² × (10/14)² = 0.227; frigate: (1/3)² × 0.51 = 0.057 | the boat takes 4× the frigate's share; both are holed or dented |
| Carrier (100,000 t) at 15 m/s on the same SSK | (100,000/102,500)² × (15/14)² = 1.09 | the boat is lost to the collision |
| Two frigates bumping at 1 m/s | 0.25 × (1/14)² = 0.001 | a dent: no damage roll |

## Effect on the shipped scenarios

No shipped scenario has a collision. In four of them, Blue's ships now give way to each other on the way in, mostly the frigate *Bastion* to the destroyer *Anvil*'s patrol. This moves their tracks a little. The golden replays were re-blessed for that reason. With avoidance switched off, the old golden hashes are reproduced exactly. In the pilot, the Red corvette *Cinder* is now sunk at 02:06:30 instead of 01:52:49.

## Verification

| Test | Checks |
|---|---|
| `damage_shares_follow_mass_and_speed` | the *Fitzgerald* and *Crystal* values; a quarter at 14 m/s; a bump below the roll |
| `default_mass_is_near_real_ships` | 0.0025·L³ for a cruiser and a carrier |
| `rules_of_the_road` | head-on, crossing, overtaking, stopped |
| `head_on_turns_to_starboard_and_open_water_does_not` | the turn; nothing to do when far or diverging |
| `a_ship_does_not_see_another_sides_submarine_but_its_own` | what is known; depth bands |
| `contact_is_counted_once_when_hulls_meet` | contact on meeting, not while touching |
| `own_side_ships_meeting_head_on_both_turn_to_starboard_and_pass` | both give way; no collision |
| `crossing_the_ship_with_the_other_on_its_starboard_side_gives_way` | give-way and stand-on |
| `a_ship_holding_a_heading_gives_way_then_resumes_it` | the ordered heading is resumed |
| `a_ship_runs_down_a_hovering_boat_it_cannot_see_and_both_are_damaged` | collision, mass shares, damage, both stop |
| `a_deep_boat_passes_under_and_an_own_side_boat_is_avoided` | depth bands; water-space management |
| `avoidance_survives_a_save_and_runs_on_identically` | save, restore and hash mid-manoeuvre |
| `a_boat_run_down_by_a_carrier_is_lost_to_the_collision` | the AAR timeline and loss cause |

## Limits

- Hulls are points with a length; heading, beam and the angle of the blow do not change the damage.
- No deliberate ramming order; a player can still steer into an unseen boat.
- No tugs, berthing, replenishment alongside or formation keeping closer than the safe passing distance. Such ships give way to each other.
- Aircraft do not collide.
