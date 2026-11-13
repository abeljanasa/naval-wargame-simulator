# DL-0046: Air operations: airbases, sorties, loadouts, aircraft fuel and tanking

- **Date:** 2026-11-13
- **Phase:** 4, slices 9–11
- **Sources:**
  - Master prompt §13 (air operations and sorties).
  - The [Phase 4 plan](../plan/phase-4.md), slices 9–11.
  - The [Gate 3 pack](../gate3/README.md) deferrals: airbases and runways, aircraft fuel and tanking, loadout choice.
  - Public descriptions of readiness states (alert 15, alert 60), turnaround and bingo fuel.
  - Original design otherwise.

## Decisions

- **Parked aircraft leave the map.** A parked aircraft's whole record moves into the base's hangar and comes back at launch. The save format already captures a unit completely, so nothing is lost. Sensors, weapons, collisions and the AI never have to learn about "aircraft on the ground": no system needed a special case.
- **Airbases are planning-layer facilities.** They sit with ports and refuelling points, edited the same way, and can be carried by a ship as command posts can. A carrier is a ship carrying an airbase, and its aircraft are lost with it.
- **Readiness as a launch delay.** Ready, alert 15 and alert 60 are the delays crews and aircraft need from tasking to takeoff. Maintenance blocks launches. This is the planning-level view a TTX needs, without modelling crews.
- **Runway slots, recoveries first.** One aircraft per interval per runway (or deck spot) bounds sortie generation, which is the capacity limit the plan asks for. Recoveries go first because an aircraft in the air may be short of fuel.
- **Loadouts by the mission's role.** The database already has loadouts with roles. Picking the first matching role keeps orders short, and naming a loadout still overrides it. A change of loadout costs rearming time (2 minutes a weapon). The turnaround after landing uses the same rate, so a heavier loadout means a longer turnaround.
- **Aircraft fuel by the ship law, with a high idle share.** The existing cube law with h = 0.6 gives a flat loiter and an expensive dash, which is the shape of an aircraft's fuel flow against speed, with one parameter. Takeoff and climb are a fixed allowance, since aircraft appear at cruise altitude. Capacity is a share of the take-off mass by category, because the database has no fuel figures for aircraft. **Open:** a fuel-capacity field.
- **Bingo is fuel home plus a reserve, worked out continuously.** It moves with the aircraft and its base (a carrier steams), so it is correct wherever the aircraft is, unlike the ships' fixed fraction.
- **Tanking only at bingo, to the nearer of tanker and base.** This is the simplest rule that makes tankers extend sorties. Hoses and the tanker's own bingo bound it.
- **Aircraft fuel follows the scenario's fuel model.** Scenarios without the fuel model keep aircraft flying as before. No shipped scenario has aircraft, so every golden replay is unchanged; checked.

## Open

- A fuel-capacity field in the platform database, and per-airframe loiter figures.
- Munitions and fuel stocks moving between depots, bases and carriers (an aviation fuel oiler).
- Crew limits, spares, runway damage and repair.
- An air tasking order: sorties generated automatically to cover zones (slice 12).
