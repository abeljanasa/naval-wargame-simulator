# Model: Fuel and refuelling

> Phase 3 slice 11. Original design; **not SME-validated**. A planning-level fuel model for a TTX: enough for endurance, bingo decisions, oilers and refuelling queues to matter.

- **Code:**
  - `crates/sim-core/src/fuel.rs` (burn rate);
  - `crates/sim-core/src/movement.rs` (burning, running dry);
  - `crates/sim-core/src/planning.rs` (diversion, transfer, oilers, berths);
  - `crates/sim-scenario` (fuel from the platform database).
- **Tests:** unit tests in `fuel.rs` (hand values); engine tests in `crates/sim-core/tests/combat.rs` (fuel section).
- **Design log:** [DL-0030](../design-log/DL-0030-fuel.md)

## Who has fuel

A scenario turns the fuel model on with `"fuel": true` (format 6). Older scenarios, and those without it, have no fuel.

With fuel on, every ship and submarine whose platform gives an **endurance** gets fuel:
- **Capacity:** 10 % of full-load displacement (generic).
- **Endurance:** the database's range at cruise speed.
- **Cruise speed:** from the database.
- **Starting level:** full, or the unit's `fuel_level` (a fraction).

Units with no endurance in the database have no fuel model; nuclear submarines are the usual case. Aircraft and land units have no fuel model either; aircraft fuel and tanking arrive with air operations in Phase 4.

## Burn

Each tick, a unit moving at speed v burns, in tonnes per second:

r(v) = C · (v_c / E) · [h + (1 − h)·(v / v_c)³]

| Symbol | Meaning |
|---|---|
| C | capacity, t |
| E | endurance at cruise, m |
| v_c | cruise speed |
| h | hotel load, 0.1 |

At cruise r = C·v_c/E, so a full load covers exactly E. The cube is propulsion power against speed for a displacement hull. The hotel load (generators, auxiliaries) burns even when stopped.

**Worked example:** a frigate with 500 t, 4 500 nm at 16 kn (8.231 m/s).
- A full load lasts 8 334 000 m / 8.231 m/s = 1 012 500 s, or 11.7 days, at cruise.
- At 32 kn the rate is 0.1 + 0.9 × 8 = 7.3 × the cruise rate. A full load then covers 2/7.3 = 27 % of the endurance, 1 233 nm.
- Stopped, it burns 10 % of the cruise rate, so a full load lasts 117 days.

**Running dry:** a unit at 0 t has no propulsion. It decelerates to a stop at its normal deceleration and stays there until refuelled (`FuelExhausted` is logged once).

## Bingo and diversion

Every 60 s, an operational unit **on a mission** (AI-run) whose fuel is below its **bingo** (default 30 %) is sent to the nearest refuelling point of its side that has fuel left (`DivertedForFuel`). Its mission is set aside until it is full. Units without a mission are left to their player, who sees the fuel level in the unit panel.

A **refuelling point** is any facility other than a command post that offers `refuel`: a depot ashore, a sheltering port, an anchorage, or an **oiler**. An oiler is a refuelling point carried by a unit (`unit`), so it moves with that unit. Because the nearest point is chosen automatically, oilers are assigned to the units that need them without orders. A point carried by a unit that is lost stops serving.

## Transfer

Every 60 s, a unit **alongside** a refuelling point takes fuel.
- **Alongside means:**
  - at a fixed point: within `radius_m` and at ≤ 0.5 m/s;
  - at an oiler under way: within `radius_m` and within 1 m/s of the oiler's speed.
- **Diverted units hold position:** they stop at a fixed point. Under way, they keep company with the oiler by steering for a point 2 km ahead of it at its speed.
- **Rate:** `transfer_t_per_h` (default 300 t/h), up to the unit's capacity and the point's stock (`fuel_t`; none means unlimited).
- **Stock:** what each point has issued is tracked. An empty point is no longer chosen.
- **Full:** a unit filled up is logged (`UnitRefuelled`) and, if it was sent for fuel, resumes its mission.

**Worked example:** the frigate at 28 % (140 t) goes alongside a depot. It needs 360 t, which at 300 t/h is 72 minutes, so it resumes in the 72nd minute.

**Queues:** a point serves at most `berths` units at once (0 means no limit), lowest unit id first. Others in the radius wait (`WaitingForBerth`, logged once per wait) and are served when a berth frees.

## In the picture

- **Units:** each unit's `fuel_fraction` (none for units without fuel). The desktop shows **Fuel** in the unit panel.
- **Facilities:** the scenario editor sets a refuelling point's stock, rate and carrier.

## Files

```json
{ "format_version": 6, "fuel": true,
  "units": [{ "id": 3, "platform": "GEN-FFG", "fuel_level": 0.4, "...": "..." }],
  "facilities": [{ "id": 7, "kind": "refuelling_point", "unit": 9, "radius_m": 1500,
                   "berths": 1, "fuel_t": 3000, "transfer_t_per_h": 400,
                   "services": { "refuel": true } }] }
```

## Limitations

- **Generic capacity:** 10 % of displacement. The database has no fuel capacity yet.
- **One fuel type:** marine fuel. Aviation fuel and fuel types arrive with air operations.
- **No weather or sea-state effect** on the burn rate, and no damage effect on fuel.
- **Oilers do not run dry themselves:** an oiler's own fuel is its unit's fuel model, and its cargo is `fuel_t`.
