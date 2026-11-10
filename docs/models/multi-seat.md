# Behaviour: Multi-seat play

> Phase 3 slice 5. Master prompt §10 (white cell controls time for all seats) and §13 (multi-seat). Original design. Networked play across machines is Phase 5; this slice puts several seats on one host.

- **Code:**
  - `crates/sim-core/src/seats.rs` (seat table, command checks)
  - `apps/desktop/src-tauri/src/host.rs` (windows, seats, time authority)
  - `apps/desktop/src-tauri/src/lib.rs` (seat windows)
- **Tests:**
  - `crates/sim-core/tests/events.rs` (seats section)
  - scenario seat tests
  - host tests in `host.rs`
  - e2e: adding a seat and opening its window
- **Design log:** [DL-0025](../design-log/DL-0025-multi-seat.md)

## Seats

A seat is a player position: `{ id, name, role }`. The role is `white_cell`, or `side` with a `side`.

- A scenario may declare `seats`. If it does not, the default is seat 0 `White cell`, then one seat per side, named after the side.
- The table reaches the simulation through a `SetSeats` command, so it is logged, saved (format 15), hashed and replayed. Only the scenario and the white cell may set it.
- **Checks in the core**, once a table is set:
  - A player's command (`Issuer::Player { seat }`) must come from a seat in the table, for that seat's side. A white-cell seat cannot issue player commands.
  - An umpire's command (`Issuer::WhiteCell { umpire }`) must name a white-cell seat. Overrides, injects and facilitation are therefore always attributable to a seat.
  - Scenario, AI and script commands are not seat-checked.
  - A refused command logs `CommandRejected` with the reason.
- **Without a table** (tests, batch runs), commands are checked by side only, as before.

Each picture carries the seats its viewer may know about: all of them for the white cell, the side's own for a side.

## Windows

| Window | Seat | Time and seats |
|---|---|---|
| **Main window** (the control station) | Any seat, chosen in the **Seat** picker. It starts at the first side's seat, so solo play is unchanged. | Always controls the clock, loads scenarios and manages seats. |
| **Seat window** | Locked to the seat it was opened for. | Controls the clock and manages seats only if it is a white-cell seat. |

- Each window gets its own picture through its own frame events: diffs, refusals and messages.
- A side's seat window never receives another side's data.
- **Orders:** a white-cell seat's orders go out as `WhiteCell { umpire: seat name }`, and a side seat's as `Player { seat }`.
- **Time controls:** in a side's seat window they are disabled ("The white cell controls time"), and the host refuses them anyway.

## White-cell console: Seats

- Lists every seat with **Open window**, which opens a window locked to that seat, for example on a commander's screen, and **Remove**.
- **New seat** adds a seat for a side, or another white-cell seat; names are unique.
- Removing a seat closes its windows.
- The main window's seat and the last white-cell seat cannot be removed.
- Every change is sent to the simulation as `SetSeats` from the acting umpire.

## Limitations

- All seats run on one machine. Remote seats need the Phase 5 network layer, which will reuse this seat model.
- There are no logins: whoever sits at a window plays its seat. Access control comes with networked play.
- A side's seat window shows the side's whole picture; there is no per-seat restriction within a side yet.
