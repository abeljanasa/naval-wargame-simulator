# Behaviour: Scenario events

> Phase 3 slice 2. Master prompt §7: "events: triggers → conditions → actions". Original design.

- **Code:**
  - `crates/sim-events` (pure: event definitions, triggers, conditions, validation)
  - `crates/sim-core/src/scenario_events.rs` (state, facts, actions, message log)
  - `crates/sim-scenario` (scenario format v3: `events`, `reserve` units)
- **Tests:**
  - unit tests in `sim-events`
  - `crates/sim-core/tests/events.rs`
  - scenario tests in `sim-scenario`
  - `gate3_demo_events_drive_the_exercise` in `crates/sim-scenario/tests/determinism.rs`
- **Design log:** [DL-0022](../design-log/DL-0022-events.md)

## When events are checked

- Once per tick, after every model has run (movement, weapons, damage, detection, AI), so triggers see that tick's outcomes.
- Events are checked in id order. For each event:
  1. Its **trigger** is checked against the tick's facts. Area triggers update their record of who is inside every tick, even while the event is disabled or spent.
  2. If the trigger occurred, the event is **enabled**, and it has not fired yet (or is **repeatable**), its **conditions** are checked in order. The first one that fails stops the check, and `ScenarioEventHeld { event, condition }` names it.
  3. Otherwise the event **fires**: `ScenarioEventFired`, then its **actions** run in order.
- An action that cannot be carried out logs `ScenarioActionFailed { event, action, reason }`, and the rest still run.
- Actions take effect at once, so a later event in the same tick sees them. For example, an event enabled by an earlier one is checked in the same tick.

## Triggers

| Trigger | Occurs |
|---|---|
| `time { at_s }` | Once, on the tick when game time reaches `at_s` (previous tick < `at_s` ≤ now). |
| `every { period_s, start_s }` | At `start_s`, then every `period_s`. |
| `area_entered { area, side?, unit? }` | A matching unit is inside the area now but was not at the last check. A unit inside when the events are loaded counts as entering at the first check. |
| `unit_destroyed { side?, unit? }` | A matching unit was destroyed this tick. Removal by the white cell does not count. |
| `contact_gained { side, target_side? }` | `side` gained a new track this tick on a unit of `target_side` (or any side). |

Areas:
- `circle { centre, radius_m }`: geodesic distance, with the edge counted as inside.
- `polygon { points }`: at least 3 points; edges are straight in latitude and longitude, and the polygon must not cross the antimeridian.

## Conditions

All conditions must hold.

| Condition | Holds when |
|---|---|
| `event_fired { event }`, `event_not_fired { event }` | The event has fired at least once, or not yet. |
| `unit_alive { unit }`, `unit_gone { unit }` | The unit is in play, or is not. |
| `units_alive { side, cmp, count }` | The side's units in play, compared (`lt`, `le`, `eq`, `ge`, `gt`). |
| `time_between { from_s?, to_s? }` | Game time is within the window, ends included. |
| `probability { p }` | One draw from the `Events` random stream is below `p`. It is drawn only when this condition is reached. |

## Actions

Actions run as `Issuer::Script` commands, with the scenario's authority, and are validated like any command.

| Action | Effect |
|---|---|
| `message { to, text }` | Adds a message to the log of `to`: `all`, `side` (with `side`), or `white_cell`. The white cell sees every message. The sender is the event's name, or its id. |
| `set_posture`, `set_doctrine`, `set_environment` | As the matching commands. |
| `set_speed`, `set_route`, `assign_mission` | Orders for a unit. |
| `spawn_reserve { unit }` | Brings in a scenario unit marked `"reserve": true`, held off the board until then. |
| `enable_event { event, enabled }` | Turns an event on or off. |
| `send_inject { inject }` | Sends an inject from the scenario library ([injects.md](injects.md)). |

## Messages

- The message log is saved and part of the state hash.
- Each picture carries the latest 100 messages its viewer receives.
- The desktop shows them in the **Messages** panel, newest first. In the white-cell seat, each message also shows its audience.

## Scenario files (format v3)

- `events` is a list of `{ id, name?, enabled? (true), repeatable? (false), trigger, conditions?, actions }`.
- Loading checks:
  - ids are unique;
  - values are valid;
  - every event, side and unit named is declared;
  - `spawn_reserve` names a reserve unit.
- v1 and v2 files still load.

Example from `data/scenarios/phase3-ttx-demo.json`. Hostilities begin when Red enters the Blue patrol box:

```json
{ "id": "e04-escalation", "name": "Exercise control",
  "trigger": { "type": "area_entered", "side": 2,
               "area": { "shape": "polygon", "points": [ ... ] } },
  "actions": [
    { "type": "set_posture", "side": 1, "toward": 2, "posture": "hostile" },
    { "type": "message", "to": "all", "text": "Both sides declared hostile; weapons free." } ] }
```

## Determinism

- Events are checked in id order, and their facts are gathered in unit-id order.
- The only random draws are `probability` conditions, on their own stream.
- Event state (enabled, times fired, who is inside each area), reserves and messages are saved (format 14) and hashed.
- A saved and resumed run fires exactly as an uninterrupted one (tested).

## Limitations

- Conditions are AND only. Lua conditions (slice 4) give anything more complex.
- Polygon edges are straight in latitude and longitude, not great circles; the difference is negligible for exercise areas of tens of kilometres.
- `unit_destroyed` knows a unit's side only if the unit was in play at an earlier check. A unit spawned and destroyed in the same tick matches only triggers with no side filter.
