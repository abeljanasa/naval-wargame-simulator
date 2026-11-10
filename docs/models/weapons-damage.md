# Model: Weapons, engagements, Ph and damage

> Phase 2 slices 7 and 8. Original design using the platform database's weapon and damage-profile figures; **not SME-validated**.

- **Code:** `crates/sim-weapons` (model), `crates/sim-core/src/combat.rs` (engagements, flight, damage)
- **Tests:** unit tests in `sim-weapons` (hand values); engine and duel tests in `crates/sim-core/tests/combat.rs`
- **Design log:** [DL-0016](../design-log/DL-0016-weapons.md) (weapons), [DL-0017](../design-log/DL-0017-ph-damage.md) (Ph and damage)

## Data

| From the database | Used for |
|---|---|
| Magazines and their default contents (aircraft: first loadout) | rounds each unit carries |
| Weapon kind, guidance, min/max range, speed, warhead, base Ph, target domains | everything below |
| Damage profile: damage points, mission-kill fraction, fire and flooding susceptibility | damage |

The scenario loader sends the weapon catalogue (`SetWeaponCatalog`) and each unit's stores and damage profile.

## Engaging (slice 7)

`Engage { shooter, contact, weapon, rounds }` fires at one of the shooter side's own tracks. It is checked when applied, and a refused order costs nothing:

1. The shooter is operational and its weapons are not knocked out.
2. The weapon can be fired at a track (not a mine or decoy) and the shooter carries enough rounds.
3. The track is classified to a domain the weapon can engage.
4. **Track quality:** the track's one-sigma error, grown to now, is within the guidance limit:

   | Guidance (from the database text) | Limit | Seeker range |
   |---|---|---|
   | Active radar seeker | 2 km | 20 km |
   | Acoustic (torpedoes) | 1.5 km | 2 km |
   | Passive IR / RF seeker | 1 km | 8 km |
   | Semi-active | 500 m | — (shooter illuminates) |
   | Command | 300 m | — (shooter steers) |
   | Navigation (GNSS, laser, terrain) and unguided | 100 m | — |

5. The track estimate is within the weapon's minimum and maximum range.
6. The **release rule** ([c2-doctrine.md](c2-doctrine.md)): the shooter's WCS and ROE allow firing at what the side knows of the track. The white cell may override this (adjudication).

**Flying weapons** (SAM, ASM, LACM, torpedo, AAM) then fly at their database speed. Every tick they head for the shooter side's current estimate of the track (midcourse update). They carry fuel for 1.1 × maximum range.
- **Seekers:** the first time the real target is within seeker range, one `Weapons` draw with Pd 0.9 decides acquisition. On acquisition the weapon homes on the real target. Otherwise it reaches the aim point and flies on until its fuel runs out. A poor track therefore misses on its own: the seeker never comes within range of the target.
- **Semi-active and command:** the weapon hits only if the shooter's side still holds the track when it arrives.
- **Navigation:** the target must be within 200 m of the aim point.

**Guns, CIWS, bombs, depth charges and ASW rockets** resolve at once: one hit roll per round.

## Probability of hit (slice 8)

Ph = base Ph × size × manoeuvre × environment, clamped to [0, 0.99]:

| Modifier | Rule |
|---|---|
| Size (surface and land targets) | under 30 m × 0.7; under 60 m × 0.85; over 150 m × 1.1 |
| Manoeuvre | air: over 300 m/s × 0.8, over 200 m/s × 0.9; surface: over 15 m/s × 0.9 |
| Environment | anti-ship missile in sea state 6+ × 0.9; gun or CIWS in sea state 5+ × 0.85 |

Countermeasures join in slice 9 (soft kill). One `Weapons` draw; logged as `HitRoll` with every modifier.

## Damage (slice 8)

- **Damage points:** 1 DP per kilogram of warhead.
- **Each hit** takes four `Damage` draws, in fixed order:
  1. Critical hit? (P 0.25)
  2. Which system (equal fifths): sensors, propulsion, weapons, flight deck or comms
  3. Fire? (P = 0.6 × fire susceptibility)
  4. Flooding? (P = flooding susceptibility for a torpedo; a fifth of it otherwise)
- **Critical effects:**
  - **Sensors:** all sensors lost.
  - **Propulsion:** maximum speed halved.
  - **Weapons:** cannot engage.
  - **Flight deck and comms:** recorded; their effects come with air operations and comms links.
- **Every 60 s:**
  - Each fire costs 2 DP, and flooding costs 3 DP.
  - Damage control draws once per fire (puts it out at P 0.25) and once for flooding (stops it at P 0.2).
- **Status:**
  - **Mission-killed** at the profile's mission-kill fraction, or with propulsion and weapons both out: afloat but unable to fight.
  - **Destroyed** at full damage points: the unit is removed (`UnitDestroyed`).

Logged: `WeaponLaunched`, `SeekerAcquisition`, `HitRoll`, `WeaponMissed` (with the reason), `DamageTaken`, `DamageControl` and `UnitDestroyed`, each with its draws and inputs.

## Worked examples

| Case | Working | Result |
|---|---|---|
| Subsonic ASM at a stationary 100 m ship, calm sea | 0.9 acquisition × 0.8 Ph | **0.72** hit rate |
| Same at a 25 m boat at 40 kn, sea state 6 | Ph = 0.8 × 0.7 × 0.9 × 0.9 | Ph **0.454** |
| SAM (0.75) at a 320 m/s fighter | × 0.8 | Ph **0.60** |
| 220 kg hit on a frigate (450 DP, mission kill at 45 %) | 220 / 450 = 48.9 % | **mission-killed**; a second hit sinks it |
| Fire on board | 2 DP a minute; out with P 0.25 each minute | expected 4 minutes, 8 DP |
| ASM launched at 30 km, seeker range 20 km | 10 km at 270 m/s | seeker looks after **≈ 37 s**; impact after ≈ 111 s |

## Verification

| Test | Checks |
|---|---|
| `database_guidance_groups`, `kinds_fly_or_resolve_at_once` | database text to guidance; flying versus at-once weapons |
| `ph_hand_values` | 0.8, 0.4536, 0.6, the 0.99 cap |
| `frigate_hand_values`, `critical_system_fifths` | mission kill and destruction thresholds; fire and flooding chances; critical table |
| `engage_orders_are_checked_before_anything_is_fired` | engine: WCS, domain, range, unknown weapon, short magazine; nothing spent when refused |
| `a_missile_flies_acquires_and_rolls_its_hand_calculated_ph` | engine: seeker look at ≈ 20 km after 35–40 s; Ph 0.8 with no modifiers |
| **`duel_hit_rate_matches_acquisition_times_ph`** | **duel: 300 engagements hit at 0.72 ± 4σ** |
| `a_hit_mission_kills_a_frigate_and_destroys_a_small_ship` | engine: 220 DP, mission-killed; destroyed and removed |
| `fires_burn_until_damage_control_puts_them_out` | engine: 2 DP a minute until put out; damage total matches |
| `guns_resolve_at_once_and_the_white_cell_may_override_wcs` | engine: five rolls at once; white-cell override |
| `save_and_resume_with_missiles_in_flight_is_exact` | determinism with weapons in flight |

## Limitations

- Weapons in flight cannot yet be detected or engaged: point defence and soft kill are slice 9.
- No salvo deconfliction, re-attack or retargeting; a seeker that misses does not search again.
- Weapons fly great-circle legs at constant speed; no flight profiles, pop-ups or terminal manoeuvres.
- Aircraft carry their first database loadout; loadout choice comes with air operations (Phase 4).
- Damage has no location, compartments or crew. The DP-per-kg rule and critical table are generic.
