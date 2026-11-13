# Model: Air operations: airbases, sorties, loadouts, aircraft fuel and tanking

> Phase 4 slices 9–11. Original design with generic figures; **not SME-validated**.

- **Code:**
  - `crates/sim-core/src/air.rs`: airbases, readiness, sorties, recovery, turnaround, bingo and tanking.
  - `crates/sim-core/src/fuel.rs`: fuel types and the aircraft burn.
  - `crates/sim-scenario/src/lib.rs`: aircraft from the database.
- **Tests:**
  - Unit tests in `air.rs`.
  - Engine tests in `crates/sim-scenario/tests/air.rs`.
  - AAR: `crates/sim-aar/tests/air.rs`.
  - Front end: `apps/desktop/src/lib/air.test.ts`.
- **Design log:** [DL-0046](../design-log/DL-0046-air-operations.md)

## Airbases (slice 9)

An **airbase** is a planning-layer facility of kind `airbase` with an `air` block:

| Field | Meaning | Default |
|---|---|---|
| `runways` | Runways ashore, or launch and recovery spots on a deck | 1 |
| `interval_s` | Seconds between launches or recoveries on one runway | 120 ashore, 60 on a deck |
| `parking` | Aircraft it can hold (0: no limit) | 0 |
| `deck` | A ship's flight deck | no |
| `fixed_wing` | Fixed-wing aircraft can use it | yes ashore, no on a deck |
| `aviation_fuel_t` | Aviation fuel held at the start, t (none: unlimited) | unlimited |

- **Carriers:** an airbase can be **carried by a ship** (`unit`), like a command post. It moves with the ship, and is lost with it, together with the aircraft parked aboard.
  - A frigate's helicopter deck takes helicopters and slow (rotary or tilt-rotor) UAVs.
  - A carrier sets `fixed_wing` to take jets.
- **Aircraft with a `base`** start **parked** there (unless `airborne`), at a **readiness**:

| Readiness | Launch after tasking |
|---|---|
| Ready | at once (when a runway is free) |
| Alert 15 | 15 minutes |
| Alert 60 | 60 minutes |
| Maintenance | not until it ends (`maintenance_s`), or the readiness is changed |

- **Off the map:** a parked aircraft is off the map. Its whole record (damage, fuel, weapons) waits in the base's hangar, so sensors, weapons and AI do not see it.
- **What a seat sees:** each seat sees its side's bases and their aircraft in the **Air** tab; the white cell sees all.

## Sorties and loadouts (slice 10)

- **The launch order:** a **launch order** gives a parked aircraft a mission: patrol (combat air patrol, ASW), strike a contact, hold a station, or fly to a point. It may also name a loadout from the platform database.
- **The loadout for the mission:** without a named loadout, the first the aircraft has in the mission's order of roles:
  - Strike: anti-ship, strike, ASW, air superiority.
  - Patrol, barrier, denial, escort: air superiority, ASW, anti-ship, strike.
  - Others keep what they carry.
- **When it launches:**
  - The readiness delay, any turnaround or maintenance still running, and **rearming**: 2 minutes for each weapon of a new loadout.
  - Then a free runway slot. Each runway launches or recovers one aircraft per interval.
  - Recoveries take slots before launches, and launches go in order of readiness, then unit id.
- **At launch** the aircraft appears over its base at its cruise altitude, at cruise speed, with the loadout's weapons and the mission.
  - Cruise altitudes by category: fighter 8,000 m, patrol 3,000 m, early warning and tanker 9,000 m, helicopter 150 m, others 5,000 m. A scenario altitude overrides them.
- **It returns** to its base when:
  - its mission is complete (a ferry);
  - its strike target is no longer among its side's contacts;
  - it launched armed and its weapons are spent;
  - its fuel reaches bingo;
  - or it is **recalled** (from the unit inspector).

  A recall can name another base, which becomes its home.
- **Another base:** a base that is lost, full, or cannot take the aircraft sends it on to the nearest airbase of its side that can.
- **Landing:** within 3 km of its base and given a runway slot, it lands, refuels from the base's stock and rearms with its loadout. It then **turns round**:
  - 20 minutes (40 for large aircraft: patrol, early warning, tanker);
  - plus 2 minutes a weapon.
- **Cancelling:** a launch not yet made can be cancelled.

## Aircraft fuel and tanking (slice 11)

With the scenario's fuel model on, aircraft carry **aviation fuel**.

- **Capacity** is a share of the maximum take-off mass: fighter 0.28, helicopter 0.15, maritime patrol 0.35, early warning 0.30, tanker 0.45, UAV 0.35. Range and cruise speed come from the database.
- **Burn by phase of flight:**
  - **Takeoff and climb:** 2 % of capacity at launch (1 % for helicopters and VTOL).
  - **Cruise, loiter, dash:** the fuel model's law r(v) = C·(v_c/E)·[h + (1 − h)(v/v_c)³] with h = **0.6**. A hull's h is 0.1. An airframe must keep flying, so loitering saves little, and dashing (afterburner) costs a lot: about 6× at 2.4 × cruise for a fighter.
- **Bingo** is the fuel to reach the nearest base that will take it, at cruise, and land with a **10 % reserve**:

  bingo = C · (distance / E + 0.1)
- **At bingo:** the aircraft turns for home. If a **tanker** of its side is on station nearer than the base, with fuel to give, it goes to the tanker instead.
- **Out of fuel:** an aircraft is **lost** (`AircraftLost`, "out of fuel"). With no base to reach, it flies on until then.
- **Tankers** (platform category `tanker`): 2 hoses, 1 t a minute each.
  - A receiver within 2 km takes a free hose (unit-id order).
  - It keeps station on the tanker and fills to 98 %, or until the tanker has only its own bingo and a 5 % margin left. Then it resumes its mission.
  - A receiver more than 10 km off loses the hose.
- **Fuel types:** the fuel model gains a **fuel type**. Ships burn marine fuel, which refuelling points give. Aircraft burn aviation fuel, which airbases (and carriers) hold.
  - Ships never divert to airbases, and aircraft never divert to refuelling points.
  - A base's stock goes down as it refuels aircraft. When it is empty, aircraft land without fuel and fly what they have left.

## Records

- **Events:**
  - `SortieOrdered`, `SortieLaunched`, `AircraftReturning`, `AircraftRecovered`;
  - `BingoFuel`, `TankingStarted`, `TankingComplete`;
  - `AircraftLost`, `ReadinessChanged`.
- **The AAR** has a **Sorties** table: launch, aircraft, base, mission, loadout, flight time, outcome, fuel from tankers. Air events appear on the timeline (kind `air`).
- **Saves:** air operations are saved (format 27) and hashed only when a scenario has aircraft. Scenarios without aircraft keep their results and golden replays.

## Worked examples

| Case | Working | Result |
|---|---|---|
| Two ready fighters and one at alert 15, one runway, all tasked at 10 s | slots 120 s apart; alert adds 900 s | launches at 10, 130 and 910 s |
| Fighter (25 t, 1,500 nm) | 0.28 × 25 | 7 t of fuel; at 300 km from base bingo is 7 × (300/2,778 + 0.1) = 1.46 t |
| Fighter back from a CAP sortie (6 missiles) | 1,200 + 6 × 120 | turns round in 32 minutes |
| CAP fighter tasked to strike | rearm with the anti-ship loadout | 2 min a weapon before launch |

## Verification

| Test | Checks |
|---|---|
| `readiness_delays`, `the_mission_picks_the_loadout`, `turnaround_grows_with_the_weapons_loaded`, `bingo_is_the_fuel_home_plus_the_reserve`, `decks_take_helicopters_and_carriers_take_jets` | the rules, by hand |
| `parked_aircraft_are_off_the_map_and_listed_at_their_base` | the hangar; what each side sees |
| `launches_wait_for_readiness_and_a_runway_and_take_the_missions_loadout` | readiness, runway slots, loadout, launch state |
| `a_strike_takes_the_anti_ship_loadout_and_a_change_costs_rearming_time` | loadout by role, rearm time, target gone |
| `a_ferry_completes_returns_lands_and_turns_round` | the whole cycle; tasking during turnaround |
| `maintenance_holds_an_aircraft_until_it_ends` | maintenance refuses launches, then ends |
| `at_bingo_an_aircraft_returns_and_takes_fuel_from_the_base` | bingo, recovery, the base's stock |
| `an_aircraft_with_nowhere_to_land_is_lost_when_its_fuel_runs_out` | fuel exhaustion |
| `a_fighter_at_bingo_near_a_tanker_refuels_from_it_and_carries_on` | tanking, resuming the mission |
| `a_helicopter_deck_takes_helicopters_and_aircraft_are_lost_with_their_ship` | decks; loss with the carrier |
| `air_operations_survive_a_save_and_run_on_identically` | save, restore, determinism |
| `a_ferry_sortie_is_a_row_from_launch_to_landing` | the AAR table, timeline and CSV |

## Limits

- **No taxi, climb or descent profile:** an aircraft appears at cruise altitude and lands from it. Climb fuel is a fixed allowance.
- **No crew duty limits, spares or munitions stocks at bases:** rearming always succeeds.
- **No cyclic carrier operations:** launches and recoveries share the deck spots.
- **No automatic sortie generation (an air tasking order):** each sortie is ordered. Automatic tasking is slice 12.
- **Tankers do not refuel tankers, and receivers do not plan tanking ahead:** they go only at bingo.
- **Runways cannot be damaged.**
