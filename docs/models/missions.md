# Model: Basic missions and automated tasking

> Phase 2 slice 10; support, mining, mine clearing, cargo, barrier patrol and denial added in Phase 3 slice 12 ([DL-0031](../design-log/DL-0031-missions.md)). Original design; **not SME-validated**. Tactics beyond the release rule and air operations come later (Phases 4–5).

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
| **Support** (station, radius) | Go to the station; hold (stop) once within the radius. Engages nothing on its own. For an oiler on station, a picket or a relay. |
| **Mining** (centre, radius, mines) | Go to the centre; within 1 km, stop and lay `mines` (or all it carries, if fewer) in a circle of the radius (`MissionMinesLaid`), then complete. A unit with no mines logs `MissionFailed` and completes. |
| **Mine clearing** (centre, radius) | Go to the area, then patrol random points inside it (as a patrol, logged `MissionWaypoint`) at 8 kn. While inside the area it sweeps (below). It never completes on its own: the side cannot know the area is clear. |
| **Cargo** (origin optional, destination, tonnes) | Go to the origin and stop 30 minutes (`CargoLoaded`); go to the destination and stop 30 minutes (`CargoDelivered`, with the tonnes, in the AAR timeline); complete. Without an origin it is already loaded. |
| **Barrier patrol** (start, end) | Steam to the start, then the end, then the start again (within 1 km counts as reached), at half speed if it was stopped. |
| **Denial** (choke point) | Patrol random points inside the side's zones that carry the choke point's name; engage only contacts believed inside those zones. |

Routes are planned round land like any other order. A mission-killed or destroyed unit does nothing.

**Random points in a zone** (denial): u₁ picks one of the n zones (zone ⌊n·u₁⌋, in zone-id order) and its fractional part n·u₁ − ⌊n·u₁⌋ is reused; u₂ is the second draw.
- **Circle:** the patrol formula.
- **Polygon:** a fan of triangles from the first vertex, chosen by area. With s = √r₁, the point is (1 − s)·a + s·(1 − r₂)·b + s·r₂·c. This is exact for convex polygons. A point that falls outside a concave polygon is discarded, and the unit tries again at the next AI period.

## Mine clearing

Every 10 s (with transit risk, `docs/models/weapons-damage.md` §Mines), a ship on a mine-clearing mission that is inside its area **and** inside a field of another side, having moved d metres:
1. **Sweep:** P = 1 − exp(−ρ·W_s·d), with ρ the field's mine density and W_s = 200 m. One `Weapons` draw; on success one mine is removed (`MineSwept`, logged with P and the draw).
2. **Risk:** the remaining mines are met at a tenth of a normal ship's swept width (W = 0.1 × the mine's swept width): one `Weapons` draw, as for any ship.

**Example:** a field of 40 mines in a 2 km circle (ρ = 3.18 × 10⁻⁶ /m²); the sweeper moves 40 m in 10 s at 8 kn. P_sweep = 1 − exp(−3.18 × 10⁻⁶ × 200 × 40) = 2.5 % per check, about 9 mines an hour.

## Automated engagement

Units on patrol, ferry, escort, strike, barrier patrol and denial engage. Units on support, mining, mine-clearing and cargo missions only defend themselves through point defence.

After moving, a tasked unit engages:
- **Target:** the nearest of its side's contacts (a strike engages only its target) that no weapon of the side is already flying at.
- **Weapon:** the first carried weapon, in id order, that passes every `Engage` check: domain, track quality, range and the WCS/ROE release rule ([c2-doctrine.md](c2-doctrine.md), [weapons-damage.md](weapons-damage.md)).
- **Rate:** one round, at most one engagement per unit per AI tick. Logged as `MissionEngage`, followed by the launch or hit roll.

The AI uses the same checks as a player, with `Issuer::Ai`, so it can never break the release rule: under weapons tight it will not fire at a track it has not identified as hostile.

## Determinism

The AI runs after detection on ticks divisible by 10, units in id order. Its only randomness is the patrol, mine-clearing and denial draws on the `Ai` stream; sweeping draws on the `Weapons` stream in the mine check. Missions are saved and hashed (save format 11). A mission's progress (barrier leg, cargo phase and its start tick) is saved and hashed when used (save format 20).

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
| `area_points_hand_values`, `new_missions_validate_and_say_whether_they_engage` | model: triangle and polygon points by hand; validation; which missions engage |
| `a_support_unit_holds_its_station_and_engages_nothing` | engine: holds within the radius, stopped; no engagement under weapons free |
| `a_mining_mission_lays_its_field_at_the_centre_and_completes` | engine: lays all 12 it carries (20 ordered) within 1 km of the centre |
| `mine_clearing_sweeps_mines_at_a_tenth_of_the_risk` | engine: mines swept and removed; risk below the sweep chance each check; 8 kn; never completes |
| `cargo_is_loaded_carried_and_delivered` | engine: 30 minutes at each end; 800 t delivered; complete at the destination |
| `a_barrier_patrol_runs_back_and_forth_between_its_ends` | engine: alternates between the ends |
| `denial_patrols_inside_the_choke_point_and_engages_only_inside_it` | engine: waypoints inside the polygon; engages a contact inside, not one outside |

## Limitations

- No mission planning (timing, coordination, return to base), no evasion or tactics, no formations beyond single-unit escort stations.
- Target choice is nearest-first, one round at a time; no salvo sizing or weapon-target pairing by Ph.
- Patrol and mine-clearing areas are circles; denial uses the choke point's zones, which may be polygons.
- Cargo is tonnage only: no cargo types, no loading capacity, no ports' cargo throughput.
- Mine clearing is one generic sweep; no mine-hunting sonar, influence sweeps or clearance-percentage planning.
