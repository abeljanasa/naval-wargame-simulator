# Model: Weather zones (Phase 4 slice 7)

- **Code:**
  - `crates/sim-events/src/weather.rs`: the zone, local weather, rain, visibility, cloud, speed.
  - `crates/sim-core`: `detection.rs` (`Weather`, `local_env`, radar), `optical.rs`, `sonar.rs`, `combat.rs`, `weapon_tracks.rs`, `planning.rs`, `movement.rs`, `coverage.rs`.
  - `apps/desktop`: the map layer, the unit panel, the Planning tab.
- **Tests:**
  - `sim-events` weather unit tests;
  - `crates/sim-scenario/tests/weather.rs`;
  - `src/lib/weather.test.ts`.
- **Design log:** [DL-0043](../design-log/DL-0043-weather-zones.md)

## Zones over the scenario-wide weather

A **weather zone** is an area (circle or polygon) with its own:
- sea state, 0–9;
- wind, in knots and the direction it blows from;
- visibility;
- rain rate, in mm/h;
- cloud base (optional).

**Precedence:** where zones overlap, the zone with the **higher id** wins, so a squall drawn over a front shows.

**Outside every zone,** the scenario-wide environment holds as before: its sea state, visibility and radar weather loss. There is no rain, wind or low cloud there; those exist only in zones. A scenario with no zones, every scenario before this slice included, is one global zone and runs exactly as before. Its state hash and golden replays are unchanged.

**Setting them:**
- in the scenario (`weather_zones`);
- by scenario events (the `set_weather` action);
- by the white cell (the Planning tab, `SetWeather`).

They are saved (format 27), in the state hash when present, and shown to every seat.

## Effects

Each effect uses the weather **where it acts**.

| Effect | Weather used | How |
|---|---|---|
| Radar sea clutter | The target's sea state | As before, from its zone |
| Radar rain loss | Rain along the path | Two-way loss summed over 16 points on the path, with a generic X-band specific attenuation γ = 0.0122·R^1.257 dB/km (the ITU-R P.838 power-law form at about 10 GHz). Added to the scenario-wide weather loss |
| Visual, EO, IR | The least visibility along the path | Both ends and 16 points between; the optical R₅₀ falls with it as before |
| Low cloud | The cloud base at the path's middle | If one end is below it and the other above, the eye and EO/IR cannot see through: no attempt. Radar and ESM are unaffected |
| Passive and active sonar | The listener's sea state | Ambient noise from it |
| Weapon Ph | The target's sea state | The existing sea-state modifier |
| Weapon tracks (missile sighting) | The observer's visibility and sea state | |
| Sheltering | The unit's and the facility's sea state | A unit is threatened by the sea where it is; a facility over its holding limit by the sea where it is |
| Speed | The hull's sea state | See below |
| Sense coverage | The sensor's sea state and visibility | The nominal ranges use them |

**Speed in heavy seas.** A surface hull's top speed is cut near and above its weather limit, which the planning layer already uses for sheltering: sea state 5 under 60 m, 7 under 120 m, 8 above.

| Sea state | Top speed |
|---|---|
| Two or more below the limit | Full |
| One below the limit | 85 % |
| At or above the limit | 60 %, while it makes for shelter |

The penalty applies under the scenario-wide sea state too. In the shipped scenarios it changes nothing, because no small hull runs above 60 % of its top speed in sea state 5, so their golden replays are unchanged.

## On the map and in the panels

- **Map:** each zone is a pale outline (with a light wash where it rains), named, with its sea state.
- **Unit panel:** the selected unit's weather.
- **Planning tab (white cell):** zones are added at a point on the map with a radius, then edited (sea state, wind, visibility, rain, cloud base) or removed. Each change is one logged white-cell order.
