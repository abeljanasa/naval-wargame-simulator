# DL-0026: Replay and after-action review

- **Date:** 2026-11-11
- **Phase:** 3, slice 6 ([plan](../plan/phase-3.md))
- **Sources:**
  - Master prompt §13 (replay and AAR: scrubber, event timeline, loss tables, weapons expended against effective, detection timeline, CSV export) and §16.
  - RFC 4180 (CSV).
  - Original design:
    - the record format;
    - keyframed seeking;
    - the counting rules.
  - No vendor product, database or binary was consulted.
- **Behaviour doc:** [replay-aar.md](../models/replay-aar.md)

## Decisions

- **A record is the command log, not a recording of states.** The core is deterministic (ADR-0003), so the seed and the commands rebuild every tick exactly. Records stay small (kilobytes to a few MB), and the final hash proves the replay is the exercise that was run.
- **The scenario JSON travels in the record**, so a record opens on another machine with nothing else. The rebuild itself uses only the commands; the scenario supplies the names and seats.
- **Keyframes without the event log.** Scrubbing needs random access. A keyframe every *k* ticks bounds a seek to *k* ticks of simulation. Dropping the event log from keyframes keeps memory flat: the model reads only the current tick's events. The replay's own hash is checked once, on the full rebuild.
- **The AAR is computed from logs**, never from snapshots. One function serves the live run, the replay and the batch runner, and every number can be traced to events.
- **Hit rolls now carry the firing unit and side** (save format 16). Without them, gun and mine kills could not be credited. The fields are `serde(default)`, and the state hash counts events, not their contents, so golden hashes are unchanged.
- **Review is a host mode, not a second window.** Every seat window follows the replay, so a hot-wash can be run on the same screens as the exercise. Orders are refused while reviewing, so the live run cannot drift.
- **AAR tables are for windows that control time.** Loss and weapon tables are ground truth. A side's view of its own timeline is offered when the control station sits at a side seat.
- **CSV formula defusing:** unit names and reasons come from scenario authors and players, so text that a spreadsheet would run as a formula is prefixed.

## Compatibility

- **Save format 16:** older saves load, with hit rolls missing the shooter and side.
- **New crate:** `sim-aar`.
- **New record format:** `NWS-RECORD` format 1.
- **Desktop:**
  - `ClockStatus` gains `replay`.
  - New commands: `review_exercise`, `close_review`, `seek_replay`, `save_record`, `open_record` (64 MB limit), `get_aar`, `aar_csv`.
- **Batch:** new `aar` subcommand.
- Golden hashes are unchanged.
