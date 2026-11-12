# DL-0023: Injects and white-cell facilitation

- **Date:** 2026-11-11
- **Phase:** 3, slice 3 ([plan](../plan/phase-3.md))
- **Sources:**
  - Master prompt §10 (white-cell console: "inject events").
  - Original design:
    - a scenario inject library with role-play senders and white-cell notes;
    - scheduled injects as an event action;
    - a white-cell `Facilitate` command (inject, message, fire event, enable event);
    - inject and event status in the white-cell picture.
  - No vendor product, database or binary was consulted.
- **Behaviour doc:** [injects.md](../models/injects.md)

## Decisions

- **Facilitation is separate from adjudication.** Sending an inject or firing an event runs the exercise as written, so it needs no reason. Overrides change outcomes, so they keep their mandatory reason. Both are logged with the umpire.
- **Injects are data in the scenario**, loaded with the events by `SetEvents`. A replay or save therefore carries the library, and send counts are saved and hashed.
- **A manually fired event bypasses its trigger, conditions and enabled flag**, because the umpire's decision replaces them. It still counts as fired, so `event_fired` conditions see it.

## Compatibility

- Save format 14, unchanged; it was not yet released when injects were added.
- Scenario format 3 gains `injects`.
- Golden hashes change because the inject count is hashed, and the demonstrator gains a scheduled media inject:
  - phase 1 `8bedd2be4a43ad72`;
  - phase 2 `21f3cc8300d560a7`;
  - phase 3 `7fb4c0bd32644c04`.
