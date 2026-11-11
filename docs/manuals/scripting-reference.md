# Scripting reference

For scenario authors: the scenario file, events, injects, Lua scripts and the planning layer. The scenario editor covers most of this with forms (see [white-cell manual](white-cell-manual.md) §7); this reference is for the JSON it writes and the parts edited as JSON. The behaviour docs in `docs/models/` are the full specification and are linked from each section.

## 1. The scenario file

A scenario is one JSON file (`"format": "nws-scenario"`, `"format_version": 5`). Older formats (1–4) still load.

| Key | Required | Meaning |
|---|---|---|
| `name`, `description` | name | Shown in the header and the AAR |
| `seed` | yes | Random seed. The same scenario, seed and orders always give the same result. |
| `duration_s` | yes | Planned length, seconds (> 0) |
| `environment` | | `sea_state` (0–9), `weather_loss_db`, `visibility_km`, `start_unix_s` (UTC, for daylight) |
| `sides` | yes | `{ id, name, postures: [{ toward, posture }], doctrine }`; posture `friendly`, `neutral` (default), `unfriendly` or `hostile` |
| `groups` | | `{ side, name, doctrine }`: task groups with shared doctrine |
| `units` | yes | See §2 |
| `minefields` | | `{ side, centre, radius_m, mines, weapon }`; `weapon` is a database mine |
| `reference_points` | | `{ name, position, side? }`: briefing marks (no effect on the simulation) |
| `facilities`, `zones`, `action_groups` | | The planning layer, §6 |
| `events`, `injects` | | §3 and §4 |
| `seats` | | `{ id, name, role: "white_cell" }` or `{ id, name, role: "side", side }`. Default: White cell, then one seat per side. |

**Doctrine** (sides, groups, units, zones) has three optional fields:
- `wcs`: `safe`, `tight` or `free`;
- `emcon`: `active` or `silent`;
- `engage_unfriendly`: `true` or `false`.

Unset fields are inherited: zone over unit, unit over group, group over side.

Positions are `{ "lat_deg": -39.6, "lon_deg": 146.4 }`. Distances are metres, speeds m/s and times seconds.

**Validation.** **Check** in the editor, or loading the file, runs the engine's checks. The editor lists every problem at once. From the command line, `nws-batch run scenario.json --ticks 1` loads, builds and runs one tick.

## 2. Units

```json
{ "id": 1, "name": "Blue destroyer — Anvil", "side": 1, "platform": "GEN-DDG",
  "position": { "lat_deg": -39.6, "lon_deg": 146.4 }, "heading_deg": 90,
  "speed_mps": 8.0, "route": [{ "lat_deg": -39.6, "lon_deg": 147.0 }],
  "group": "TG 1", "doctrine": { "wcs": "tight" },
  "mission": { "kind": "patrol", "centre": { "lat_deg": -39.6, "lon_deg": 146.7 }, "radius_m": 8000 },
  "reserve": false }
```

- `platform` names a database class (see the platform database browser). It supplies the domain, speeds, sensors, signatures, stores and damage profile.
- Without a platform, give `domain`, `max_speed_mps` and `accel_mps2`.
- `speed_mps` defaults to the platform's cruise speed.
- `jammer` adds a noise jammer.

| Mission | Fields | Behaviour ([missions.md](../models/missions.md)) |
|---|---|---|
| `patrol` | `centre`, `radius_m` | Random points in the circle |
| `ferry` | `destination` | Go there; then complete |
| `escort` | `escorted` (unit id), `bearing_deg` (relative), `range_m` | Keep station |
| `strike` | `contact` (side's contact number) | Close and attack |

Units on a mission also engage automatically under their doctrine.

**Reserve units** (`"reserve": true`) stay off the board until an event's `spawn_reserve` brings them in.

## 3. Events

An event: when its **trigger** occurs and every **condition** holds, its **actions** run ([events.md](../models/events.md)).

```json
{ "id": "e04-escalation", "name": "Exercise control",
  "trigger": { "type": "area_entered", "side": 2,
               "area": { "shape": "circle", "centre": { "lat_deg": -39.6, "lon_deg": 146.5 }, "radius_m": 20000 } },
  "conditions": [ { "type": "event_not_fired", "event": "e05-ceasefire" } ],
  "actions": [
    { "type": "message", "to": "all", "text": "Red units have entered the exclusion zone." },
    { "type": "set_posture", "side": 1, "toward": 2, "posture": "hostile" }
  ],
  "repeatable": false, "enabled": true }
```

**Triggers:**

| `type` | Fields | Fires |
|---|---|---|
| `time` | `at_s` | Once, at that game time |
| `every` | `period_s`, `start_s` | At `start_s`, then every `period_s` (set `repeatable`) |
| `area_entered` | `area`, `side?`, `unit?` | When a matching unit enters the area |
| `unit_destroyed` | `side?`, `unit?` | When a matching unit is destroyed (not removed) |
| `contact_gained` | `side`, `target_side?` | When `side` gains a new contact |

An area is `{ "shape": "circle", "centre", "radius_m" }` or `{ "shape": "polygon", "points": [...] }` (at least three points, not crossing the antimeridian).

**Conditions** (all must hold):
- `event_fired` / `event_not_fired` `{ event }`;
- `unit_alive` / `unit_gone` `{ unit }`;
- `units_alive { side, cmp, count }`, with `cmp` one of `lt`, `le`, `eq`, `ge`, `gt`;
- `time_between { from_s?, to_s? }`;
- `probability { p }`: a seeded draw;
- `lua { script }`: §5.

**Actions** (in order):
- `message { to: "all" | "side" | "white_cell", side?, text }`;
- `send_inject { inject }`;
- `set_posture { side, toward, posture }`;
- `set_doctrine { scope, doctrine }`;
- `set_environment { environment }`;
- `set_speed { unit, speed_mps }`;
- `set_route { unit, waypoints }`;
- `assign_mission { unit, mission }`;
- `spawn_reserve { unit }`;
- `enable_event { event, enabled }`;
- `lua { script }`.

**Rules:**
- Events are checked once a tick, in id order.
- An event fires at most once, unless `repeatable`.
- A disabled event (`enabled: false`) waits for `enable_event` or the white cell.
- Every firing, hold and failure is logged.

The editor's **Add an event** offers templates, valid as written.

## 4. Injects

Prepared messages for the white cell to send, or for events to send ([injects.md](../models/injects.md)):

```json
{ "id": "i01-distress", "title": "Distress call", "from": "MV Ember",
  "to": "side", "side": 1, "text": "Engine room fire, request assistance.",
  "note": "Send early to see whether Blue detaches a unit." }
```

`note` is for the white cell only. `to` is `all`, `side` (with `side`) or `white_cell`.

## 5. Lua

A `lua` condition returns `true` or `false`. A `lua` action asks for actions through `sim.*` ([lua-scripting.md](../models/lua-scripting.md)).

| Function | Returns or does |
|---|---|
| `sim.time()` | Game time, s |
| `sim.fired(id)` | Times event `id` has fired |
| `sim.unit_alive(id)` | Whether unit `id` is in play |
| `sim.units_alive(side)` | Count of the side's units in play |
| `sim.unit(id)` | `{ id, side, lat, lon }` or `nil` |
| `sim.random()` | Seeded uniform [0, 1) |
| `sim.message(to, text)` | `to`: `"all"`, `"white_cell"` or a side number |
| `sim.inject(id)` | Sends an inject |
| `sim.enable_event(id, on)` | Turns an event on or off |
| `sim.set_posture(side, toward, p)` | Changes a posture |
| `sim.set_speed(unit, mps)` | Orders a speed |
| `sim.spawn_reserve(unit)` | Brings in a reserve unit |

```lua
-- condition: Red has lost half its force
return sim.units_alive(2) <= 2
```

```lua
-- action: a scripted umpire brief
local u = sim.unit(1)
if u then
  sim.message('white_cell', string.format('Anvil at %.3f, %.3f', u.lat, u.lon))
end
```

**The sandbox:**
- Only `table`, `string`, `math` and `utf8` are available. There is no file, OS, module, debug, coroutine, `load` or `pcall` access.
- 1 million instructions and 16 MiB per run; 64 KiB per script.
- Use `sim.random()`, not `math.random`. Iteration with `pairs` is sorted.
- A script error is logged and the event is held; the exercise carries on.
- A syntax error stops the scenario loading. The editor's **Check** reports it.

## 6. The planning layer

([planning-layer.md](../models/planning-layer.md))

**Facilities:**

```json
{ "id": 6, "name": "Blue flagship command", "side": 1, "kind": "command_post",
  "position": { "lat_deg": -39.6, "lon_deg": 146.4 }, "radius_m": 500, "range_m": 150000,
  "unit": 1, "subordinates": [], "alternate": 1, "handover_s": 300, "orders_in_gap": "queue" }
```

| `kind` | Fields that matter |
|---|---|
| `command_post` | `range_m`, `unit` (carried by), `subordinates` (empty = all), `alternate`, `handover_s`, `orders_in_gap` (`lose` or `queue`) |
| `sheltering_port`, `anchorage`, `refuelling_point` | `radius_m`, `berths` (0 = no limit), `services: { repair, rearm, refuel }` |

**Zones:**

```json
{ "id": 2, "name": "Passage engage zone", "side": 1, "kind": "engage",
  "area": { "shape": "circle", "centre": { "lat_deg": -39.6, "lon_deg": 147.0 }, "radius_m": 15000 },
  "doctrine": { "wcs": "free", "engage_unfriendly": true },
  "choke_point": "Kent passage", "shooters": [3] }
```

- A `sense` zone governs the side's own units inside it, and alerts the side when it gains a contact there.
- An `engage` zone governs engagements of targets inside it. Its `shooters` engage those targets on their own.
- Zones sharing a `choke_point` name form a choke point; other sides' units entering it are logged.

**Surface action groups:**

```json
{ "name": "SAG 1", "side": 1, "leader": 1, "doctrine": { "emcon": "active" },
  "members": [ { "unit": 2, "bearing_deg": 180, "range_m": 3000 } ] }
```

- Members keep station on the leader (bearing relative to its heading).
- All are put in a group named after the action group, under its doctrine.
- Members must not have a mission or a group of their own.

## 7. Determinism, for authors

- The same scenario, seed and orders give the same run on every machine. Records replay exactly, and the batch runner's hashes match across operating systems.
- Randomness in scenarios comes only from `probability` conditions and `sim.random()`, both seeded.
- Changing anything in a scenario changes its runs. If it is a shipped scenario, its golden hash in `tests/golden/` must be re-blessed (`NWS_BLESS=1`), with the reason in the pull request.
