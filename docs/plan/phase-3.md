# Phase 3 plan: TTX release 1.0 (Gate 3: navy TTX pilot on the release build)

- **Started:** 2026-11-11, on branch `phase-3/ttx-release`, after Phase 2 merged (PR #2). The Gate 2 review pack recommends going ahead with three conditions; the formal panel review has not been held yet. Anything it changes feeds back into this plan.
- **Scope (master prompt §13):** scenario editor, events + Lua, white cell, injects, adjudication override, multi-seat, replay and AAR, operational planning layer (§7.1).
- **Order:** the client chose white cell and injects first (2026-11-11): what a facilitator needs to run an exercise.
- **Gate 3 definition of done (§16):**
  - All P1 features in §7 and the planning layer in §7.1 work.
  - A white cell can run a multi-seat exercise with injects, overrides and AAR.
  - Signed installers exist for Windows, macOS and Linux, and run air-gapped.
  - CI is green and the manuals are complete.
  - A navy staff has run a TTX pilot on the release build.

## Slices

Each slice: behaviour spec, design-log entry, tests, docs, CHANGELOG, green CI (§15, §16).

| # | Slice | Delivers | Depends on |
|---|---|---|---|
| 1 ✅ | **White-cell adjudication** | Force a weapon's hit or miss, set or repair damage, destroy or remove a unit, move a unit, set stores. Every override is logged with umpire ID and reason (§10). White-cell console in the desktop | — |
| 2 ✅ | **Event engine** (`sim-events`) | Scenario events: triggers (time, area entry, unit destroyed, contact gained) → conditions → actions (message, posture, doctrine, environment, spawn, mission). Evaluated deterministically each tick, every firing logged | — |
| 3 ✅ | **Injects** | Inject library in the scenario; scheduled (by event) and manual (white cell) injects; message log per side in the desktop | 2 |
| 4 | **Lua scripting** | `mlua` (Lua 5.4) conditions and actions, sandboxed (no io/os/package/debug/load), instruction and memory limits, sandbox-escape tests | 2 |
| 5 | **Multi-seat play** | Seats per side; white-cell control of time for all seats; seat management | — |
| 6 | **Replay and AAR** | Replay scrubber from the command log, event timeline, loss tables, weapons expended vs effective, detection timeline, CSV export | — |
| 7 | **Scenario editor** | Units, sides, postures, missions, minefields, reference points, events; validation on save | 2 |
| 8 | **Planning layer (§7.1)** | Command posts, refuelling points, sheltering ports, anchorages, choke points with sense and engage zones (per-zone ROE/WCS), surface action groups | 2, 7 |
| 9 | **Gate 3 pack** | User and white-cell manuals, scripting reference, signed installers (with the client's certificates), macOS re-enabled, TTX pilot plan | all |

Out of phase (backlog): Monte Carlo batch statistics, SVP acoustics, weather zones, air operations (Phase 4); networked multiplayer (Phase 5).
