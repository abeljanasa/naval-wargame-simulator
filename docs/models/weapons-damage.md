# Model: Weapons, engagements, Ph and damage

> Phase 2 slices 7, 8, 9 and 11. Original design using the platform database's weapon and damage-profile figures; **not SME-validated**.

- **Code:** `crates/sim-weapons` (model), `crates/sim-core/src/combat.rs` (engagements, flight, damage)
- **Tests:** unit tests in `sim-weapons` (hand values); engine and duel tests in `crates/sim-core/tests/combat.rs`
- **Design log:** [DL-0016](../design-log/DL-0016-weapons.md) (weapons), [DL-0017](../design-log/DL-0017-ph-damage.md) (Ph and damage), [DL-0018](../design-log/DL-0018-point-defence.md) (point defence and soft kill), [DL-0020](../design-log/DL-0020-mines.md) (mines)

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

Countermeasures (slice 9) multiply in: × (1 − decoy effect) when the target fired a matched decoy at this weapon, and × 0.8 against an active radar seeker when the target's own jammer is radiating. One `Weapons` draw; logged as `HitRoll` with every modifier.

## Point defence and soft kill (slice 9)

Every tick, before weapons move:

1. **Interceptors in the air** that are due resolve with Ph = interceptor base Ph × the threat's manoeuvre modifier (air rules). A kill removes the threat (`InterceptRoll`, `WeaponMissed: shot down`).
2. **Defenders** are operational units with sensors, in unit-id order. A defender sees an incoming enemy missile when either of these holds; detection here takes no draw:
   - its **radiating radar** (EMCON active) has the missile inside both the radar horizon (to the missile's altitude) and the radar's R₅₀ against a **0.1 m²** missile;
   - the side holds a **track** on the missile this tick from one of the defender's own sensors (below), so a radar-silent ship can still defend itself with IR, EO or visual sighting; or, in a surface action group, from any unit of the group (shared picture, [planning-layer.md](planning-layer.md)).
   - In an action group, a defender does not fire a SAM at a threat that another member's SAM is already engaging (one area-defence shot per threat at a time across the group).
3. **Firing:**
   - After the **reaction time** (SAM 8 s, gun 5 s, CIWS 3 s), with the threat inside the weapon's range, the defender fires one round per free **fire channel** (SAM 2, gun 1, CIWS 1), one engagement per threat at a time.
   - **SAMs defend the area**: any incoming enemy missile. **Guns and CIWS defend only their own ship.**
   - A shot resolves after the head-on intercept time, range / (threat speed + interceptor speed), with the channel busy until then.
   - If the threat survives, the next tick can shoot again (shoot-look-shoot).
   - **Saturation** emerges when threats arrive faster than channels free up.
   - Point defence is self-defence: it works under any weapons control status.
4. **Soft kill:** when an incoming weapon closes inside 10 km (3 km for a torpedo), its target fires one matched decoy at it, if it carries one:
   - chaff or IR decoys against radar and IR seekers;
   - torpedo countermeasures against acoustic homing.

   The decoy's database `base_ph` (0.3 for the generic decoys) is its seduction effect (`DecoyLaunched`).

| Case | Working | Result |
|---|---|---|
| Ship SAM (0.75) against a 270 m/s ASM | × 0.9 manoeuvre | intercept Ph **0.675** |
| SAM (900 m/s) at an ASM 20 km out | 20 000 / (270 + 900) | intercept after **18 s** |
| Radar R₅₀ 120 km at 1 m² against a 0.1 m² missile | 120 × 0.1^¼ | **67.5 km** (then the horizon limits a sea-skimmer to ≈ 34 km) |
| Chaff plus jamming against an active radar ASM (0.8) | 0.8 × 0.7 × 0.8 | Ph **0.448** |
| 20 mines, 2 km radius, W 100 m, crossing the 4 km diameter | λ = 20/(π·2000²) × 100 × 4000 = 0.637 | P(encounter) **0.471**; × Ph 0.5 → hit 0.236 for the first encounter |
| Same field, one 10 s step at 10 m/s | λ = 0.0159 | P **0.0158** |

## Weapons as sensor targets (Phase 3 slice 15)

Every tick, before point defence, each unit (unit-id order) looks with each sensor (mount order) at every other side's weapon in flight (weapon-id order). No draws ([DL-0034](../design-log/DL-0034-weapon-tracks.md)):

| Sensor | Sees | When |
|---|---|---|
| Radar, EMCON active | missiles | within R₅₀ against 0.1 m², the radar horizon to the missile's height, and the radar's altitude cover |
| IR | missiles | within the optical R₅₀ against the hot plume (full size, [optical-mad.md](optical-mad.md)), the visibility (× 1.5 for IR) and the horizon |
| EO, visual | missiles | within the optical R₅₀ against a 5 m body, in the light at the sensor, the visibility and the horizon |
| ESM | missiles whose active radar seeker is homing | within the horizon and the ESM's range |
| Hull, towed or dipping sonar (usable) | torpedoes | within the passive range (signal excess 0) against a 150 dB torpedo |

The side's **track** on a weapon records:
- the first and last tick seen;
- the weapon's latest position;
- the unit and the sensor that saw it last (the highest unit id when several do).

A new track is logged as `WeaponDetected`, and shows in the AAR timeline. A track is dropped 2 s after the last sighting, or when the weapon is gone. Tracks are saved (save format 23) and hashed when any exist.

**In the picture:** a side sees its tracks on other sides' weapons (`weapon_tracks`), and the desktop draws them as rings: red for missiles, blue for torpedoes. The white cell sees every side's.

**Worked example:** a radar-silent frigate with a 20 km IR sensor, at night. The R₅₀ against a missile's plume is 0.7 × 20 km = 14 km. A missile launched from 30 km is tracked at 14 km, and after the SAM's 8 s reaction time the frigate fires at about 11.8 km (270 m/s × 8 s nearer). Without IR, the silent frigate never sees the missile and does not fire.

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

## Mines (slice 11, basic)

- **Fields:** a minefield is a circle (centre, radius) with a number of mines of one database type. It is created by the scenario or white cell (`CreateMinefield`), or laid by a ship or submarine from its stores (`LayMines`, centred on its position).
- **Transit risk:** every 10 s, each ship or submarine that moved since the last check, and is inside a field of another side, takes one `Weapons` draw against the simple initial threat:

  P = 1 − exp(−ρ · W · d)

  - ρ is the field's mines per m², over π r².
  - W is the swept width, twice the mine's actuation radius (its database maximum range: 50 m for the generic bottom mine, so W = 100 m).
  - d is the distance moved since the last check.
- **An encounter** uses up a mine and rolls the mine's Ph (database base 0.5, with the usual modifiers). A hit does warhead damage, and floods like a torpedo (below the waterline).
- **Safe lanes:** the owning side is not at risk in its own fields. A side sees only its own fields; the white cell sees all.
- **Logged:** `MineEncounter`, with distance, P, the draw and the outcome.
- Mine countermeasures and mine-hunting missions are P2 (full mines) and not in this slice.

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
| `point_defence_rules_hand_values`, `soft_kill_hand_values` | intercept time, reaction, channels; decoy matching; × 0.7 and × 0.56 |
| `sams_engage_incoming_missiles_with_the_hand_calculated_ph` | engine: engaged after the reaction time; intercept Ph 0.675 |
| `two_sam_channels_limit_a_four_missile_raid` | engine: never more than two SAMs in the air from one ship |
| `decoys_and_jamming_lower_ph_at_impact` | engine: decoy effect 0.3; countermeasures 0.56; Ph 0.448 |
| `simple_initial_threat_hand_values` | P 0.4710 crossing, 0.01579 per step |
| **`mine_transit_risk_matches_the_simple_initial_threat`** | **duel: 200 transits encounter at 0.471 ± 4σ**; per-step P logged |
| `a_ship_lays_its_own_field_and_is_not_at_risk_in_it` | engine: laying from stores; owner immune; fog of war on fields |

## Limitations

- Incoming weapons are seen deterministically at nominal ranges (0.1 m² missile, 5 m body, 150 dB torpedo). Tracks carry no position error. Torpedoes are countered only by decoys: no hard-kill anti-torpedo weapons.
- No salvo deconfliction, re-attack or retargeting; a seeker that misses does not search again.
- Weapons fly great-circle legs at constant speed; no flight profiles, pop-ups or terminal manoeuvres.
- Aircraft carry their first database loadout; loadout choice comes with air operations (Phase 4).
- Damage has no location, compartments or crew. The DP-per-kg rule and critical table are generic.
