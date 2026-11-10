# Behaviour and reference: Lua scripting

> Phase 3 slice 4. Master prompt §7 ("Lua scripting, sandboxed") and §12 (mlua). Original design.

- **Code:** `crates/sim-events/src/lua.rs` (sandbox, `sim` API); `crates/sim-core/src/scenario_events.rs` (runs scripts)
- **Tests:**
  - sandbox, API and escape tests in `crates/sim-events/src/lua.rs`;
  - engine tests in `crates/sim-core/tests/events.rs` (Lua section);
  - the demonstrator's `e08-umpire-brief`, which every OS's golden replay runs.
- **Design log:** [DL-0024](../design-log/DL-0024-lua.md). Events are in [events.md](events.md).

## Where scripts go

A script is Lua 5.4 source in a scenario event:

- **Condition:** `{ "type": "lua", "script": "..." }` must `return true` or `false`. It may read the simulation but not ask for actions.
- **Action:** `{ "type": "lua", "script": "..." }` may read the simulation and ask for actions. The engine carries them out in order after the script returns, with the same checks as any event action.

Scripts are compiled when the events are loaded, so a syntax error stops the scenario loading.

## The `sim` table

| Function | Returns or does |
|---|---|
| `sim.time()` | Game time, seconds. |
| `sim.fired(id)` | Times event `id` has fired. |
| `sim.unit_alive(id)` | Whether unit `id` is in play. |
| `sim.units_alive(side)` | Number of `side`'s units in play. |
| `sim.unit(id)` | `{ id, side, lat, lon }`, or `nil` when the unit is not in play. |
| `sim.random()` | A uniform number in [0, 1) from the simulation's `Events` stream: seeded, saved and replayed. |
| `sim.message(to, text)` * | `to` is `"all"`, `"white_cell"`, or a side number. |
| `sim.inject(id)` * | Sends an inject from the library. |
| `sim.enable_event(id, on)` * | Turns an event on or off. |
| `sim.set_posture(side, toward, p)` * | `p` is `"friendly"`, `"neutral"`, `"unfriendly"` or `"hostile"`. |
| `sim.set_speed(unit, mps)` * | Orders a speed. |
| `sim.spawn_reserve(unit)` * | Brings in a reserve unit. |

\* Action scripts only. In a condition script they raise an error.

Example: an action that reports to the umpires.

```lua
local u = sim.unit(1)
sim.message('white_cell', string.format('Anvil at %.4f S %.4f E', -u.lat, u.lon))
```

## The sandbox

Every run, condition or action, gets a fresh interpreter, so nothing carries over between runs.

- **Libraries:** `table`, `string`, `math` and `utf8`, plus the safe parts of the base library.
- **Not available:**
  - the libraries `io`, `os`, `package`, `debug` and `coroutine`;
  - the base functions `load`, `loadfile`, `dofile`, `require`, `collectgarbage`, `print`, `warn`, `pcall`, `xpcall` and `next`;
  - `string.dump`, `math.random` and `math.randomseed`.
- **Text only:** precompiled bytecode is refused.
- **Limits:**

  | Limit | Value | On breach |
  |---|---|---|
  | Instructions | 1 000 000 per run | "instruction limit exceeded" |
  | Memory | 16 MiB per run | "not enough memory" |
  | Source | 64 KiB per script | refused at load |

- Without `pcall`, a script cannot catch the limit errors and carry on.
- **On error:**
  - A failing condition logs `ScenarioScriptFailed { event, condition, reason }`, and the event is held.
  - A failing action logs `ScenarioActionFailed { event, action, reason }`.
  - The run always continues.

## Determinism

Lua is not deterministic out of the box. The sandbox removes or replaces every source of variation it can reach:

| Source | Treatment |
|---|---|
| `pairs` order (hash-seed dependent) | `pairs` walks keys sorted: numbers, then strings, then booleans. Other key types are an error. `next` is removed. |
| Addresses in `tostring` and `%s` | Tables, functions and threads print as their type name (unless they define `__tostring`). `%p` is refused. |
| Platform maths | `math.sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `exp`, `log` and the new `math.pow` are the Rust `libm` functions used by the models. |
| Randomness | `math.random` is removed; use `sim.random()`. |
| Garbage-collector timing | Finalizers (`__gc`) and weak tables (`__mode`) are refused. |

- **Known gap:** the `^` operator uses the platform's C `pow`. Use `math.pow(x, y)` in any result that matters.
- **Known gap:** float formatting uses the C library. `%d`, `%.Nf` and `%g` agree on Windows and Linux in the CI matrix, which replays the demonstrator's script on both.

## Limitations

- The API reads unit positions and counts, but not tracks, damage or weapons. It grows as exercises need it.
- Scripts cannot keep state between runs. Use event state instead (`sim.fired`, `enable_event`).
- `sim.message` cannot set a title; use an inject for that.
