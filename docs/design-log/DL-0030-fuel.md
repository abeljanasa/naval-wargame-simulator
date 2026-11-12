# DL-0030: Fuel and refuelling

- **Date:** 2026-11-12
- **Phase:** 3, slice 11 ([plan](../plan/phase-3.md))
- **Sources:**
  - Master prompt §7 (platforms: fuel, P1), §7.1 (refuelling points and areas: units below a threshold route to the nearest valid point; tankers and oilers assigned automatically; queues when capacity is exhausted), §8 (fuel burn by speed regime).
  - Textbook propulsion power for displacement hulls (power roughly ∝ speed³).
  - Platform database fields `endurance_m`, `cruise_speed_mps` and `displacement_t`.
  - Original design: the hotel load, the capacity fraction, bingo, the transfer rate and the RAS station.
  - No vendor product, database or binary was consulted.
- **Behaviour doc:** [fuel.md](../models/fuel.md)

## Decisions

- **Fuel is opt-in per scenario** (`"fuel": true`, scenario format 6). Turning it on everywhere would change every shipped scenario's result and hash, and would make old exercises end differently. Turning it on per scenario lets a TTX use logistics on purpose.
- **Burn anchored on the database's endurance.** The only fuel figure the database has is range at cruise. The cube law with a 10 % hotel load reproduces that range exactly at cruise, and penalises sprinting the way staff expect: about a quarter of the range at twice cruise.
- **Capacity in tonnes, 10 % of displacement.** Planners and logisticians think in tonnes, and depots and oilers hold tonnes. With no capacity field in the database, a generic fraction keeps one number per class. It is a constant in one place, ready for a database field.
- **Out of fuel means no propulsion, not loss.** The ship stops and waits for an oiler: a TTX decision point, not a silent removal.
- **Only AI-run units divert at bingo.** A player's ship is the player's decision; the panel shows the fuel level. This is the same rule as diversion for repair (DL-0028).
- **Oilers are carried refuelling points.** An oiler reuses the facility model: services, berths, radius, and now stock and rate. Carried by a unit, the point moves with it. Nearest-point selection then assigns oilers automatically. RAS under way is "within the radius and matching the oiler's speed", and the receiving ship steers for a point ahead of the oiler.
- **Issued fuel is state, stock is definition.** The planning layer records what each point has issued. Editing the layer (white cell) keeps the account, and it is saved and hashed.
- **Queues are visible.** Units in a full point's radius wait in unit-id order. `WaitingForBerth` is logged once per wait, so the AAR shows logistics bottlenecks.
- **Hashed only when present.** A unit's fuel enters the state hash only if it has fuel, so scenarios without fuel keep their golden hashes.

## Compatibility

- **Scenario format 6:** `fuel` (scenario), `fuel_level` (unit). Format 5 files load unchanged, with no fuel.
- **Facility fields:** `transfer_t_per_h`, `fuel_t`; refuelling points may be carried by a unit.
- **Save format 19:** unit fuel; the planning layer's issued fuel, waiting units, and diversion reasons. Older saves load with none.
- **Spawn:** new `fuel` field (optional).
- **New events:** `FuelExhausted`, `DivertedForFuel`, `UnitRefuelled`, `WaitingForBerth`.
- **Snapshot:** `fuel_fraction` on each unit.
- **Golden hashes:** only the planning demonstrator's changes again, because facilities hash with the new fields. Behaviour is unchanged.
