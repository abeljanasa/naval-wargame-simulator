# DL-0025: Multi-seat play

- **Date:** 2026-11-11
- **Phase:** 3, slice 5 ([plan](../plan/phase-3.md))
- **Sources:**
  - Master prompt §10 (white cell controls time for all seats), §13 (multi-seat) and §16 (Gate 3: a white cell runs a multi-seat exercise).
  - Original design:
    - the seat table and its core checks;
    - the control-station main window;
    - locked seat windows;
    - time authority.
  - No vendor product, database or binary was consulted.
- **Behaviour doc:** [multi-seat.md](../models/multi-seat.md)

## Decisions

- **Seats are enforced in the core, not only in the UI.** A player command must come from a seat of its side, and an umpire must be a white-cell seat. The command log therefore attributes every order and override to a seat, and the same rules will hold when seats are remote (Phase 5).
- **The seat table is a command (`SetSeats`)**, so seat changes made during an exercise are replayed and audited like any other.
- **One seat per window on one host.** Several screens or a projector in one room is the common TTX layout. The main window stays a free control station, so solo use and the facilitator's own console work as before.
- **Time belongs to the white cell.** The host refuses clock changes from side seat windows, and the UI disables them there.

## Compatibility

- Save format 15 (older saves load with no seat table, so no seat checks).
- Scenario format 3 gains optional `seats`.
- Desktop:
  - `set_viewer` is replaced by `set_seat`;
  - new commands: `add_seat`, `remove_seat`, `open_seat_window`;
  - `SessionInfo` gains the window's seat, all seats, `locked` and `controls_time`;
  - the window capability covers `seat-*` windows.
- Golden hashes change because the seat table is hashed:
  - phase 1 `dd942f76e85bd4f2`;
  - phase 2 `cb360f9a36c8bc87`;
  - phase 3 `db033be444af12f8`.
