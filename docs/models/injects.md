# Behaviour: Injects and white-cell facilitation

> Phase 3 slice 3. Master prompt §10 (white-cell console: "inject events"). Original design.

- **Code:**
  - `crates/sim-events/src/model.rs` (`Inject`, `send_inject`)
  - `crates/sim-core/src/scenario_events.rs` (`Facilitation`, delivery, status views)
  - desktop white-cell console in `apps/desktop/src/App.svelte`
- **Tests:**
  - `crates/sim-core/tests/events.rs` (injects section)
  - `gate3_demo_events_drive_the_exercise`
  - e2e: the white cell sends an inject
- **Design log:** [DL-0023](../design-log/DL-0023-injects.md). Events are in [events.md](events.md).

## The inject library

A scenario's `injects` are prepared messages:

```json
{ "id": "i01-distress", "title": "Distress call", "from": "MV Ember",
  "to": "side", "side": 1,
  "text": "MV Ember, 20 nm north-west of you: engine room fire, two injured, request assistance.",
  "note": "Send early to see whether Blue detaches a unit from the patrol." }
```

- `to` is `all`, `side` (with `side`), or `white_cell`.
- `from` is the role-played sender. When it is empty, the sending event's name is used, or `Exercise control` when the white cell sends the inject.
- `note` is guidance for the white cell. It is never sent to a side.
- Loading checks that ids are unique, every inject has text, every `side` is declared, and every `send_inject` action names an inject.

## Sending injects

- **Scheduled:** an event action, `{ "type": "send_inject", "inject": "i02-media" }`, sends it when the event fires.
- **Manual:** the white cell's `Facilitate` command (below).

A sent inject becomes a message with its title and inject id. Each inject keeps a count of times sent.

## White-cell facilitation

`Facilitate { action }` is accepted from `Issuer::WhiteCell` only. It runs the exercise; it does not override outcomes, so unlike an adjudication it needs no reason. Each one logs `Facilitated { umpire, action }` before it takes effect, and replays exactly.

| Action | Effect |
|---|---|
| `send_inject { inject }` | Sends the inject now. |
| `send_message { to, text }` | A free-text message from `Exercise control`. |
| `fire_event { event }` | Runs the event's actions now, whatever its trigger and conditions, and even if it is disabled or spent. Counts as fired. |
| `set_event_enabled { event, enabled }` | Turns an event on or off. |

An unknown inject or event is refused, and the refusal comes back to the seat.

## What the white cell sees

Each white-cell picture carries:
- the inject library, with notes and send counts;
- every event's state: enabled, repeatable, times fired.

Sides never receive either.

In the desktop's white-cell console:
- **Injects:** each with its audience, send count, note and a **Send** button.
- **Events:** an enable checkbox, the fired count and a **Fire** button.
- **Message to:** a free-text message for all sides, one side, or the white cell.

The **Messages** panel shows injects with their title in bold.

## Limitations

- An inject cannot carry attachments or map overlays yet.
- Players cannot reply in the simulation. Replies go through the exercise's own channels.
- The umpire ID is the white-cell seat's name ([multi-seat.md](multi-seat.md)).
