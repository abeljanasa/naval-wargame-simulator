# Behaviour: Replay and after-action review

> Phase 3 slice 6. Master prompt §13 (replay and AAR) and §16 (Gate 3: a white cell runs a multi-seat exercise with injects, overrides and AAR). Original design.

- **Code:**
  - `crates/sim-aar` (records, replay, report, CSV)
  - `apps/desktop/src-tauri/src/host.rs` (review mode)
  - `apps/desktop/src/lib/AarPanel.svelte` (scrubber and tables)
  - `apps/batch/src/main.rs` (`nws-batch aar`)
- **Tests:**
  - `crates/sim-aar/tests/aar.rs`
  - host tests in `host.rs` (review, records, AAR per seat)
  - e2e: replay, scrubber, AAR tables, CSV, back to live
- **Design log:** [DL-0026](../design-log/DL-0026-replay-aar.md)

## Exercise records

A record (`.nwsrec`, MessagePack, `NWS-RECORD` format 1) holds:

| Field | Meaning |
|---|---|
| `scenario_name`, `scenario_json` | The scenario the exercise started from, so a record opens on its own (sides, seats, names) |
| `seed` | The run's seed |
| `land_mask` | The coastline the run navigated round (`None`: open ocean) |
| `end_tick` | How far the run had got |
| `final_hash` | `state_hash()` at `end_tick` |
| `commands` | Every accepted command: scenario set-up, orders, overrides, facilitation, seat changes |

The run is rebuilt from the seed and the commands alone (ADR-0003). AI and script decisions are not in the log; they are re-made identically.

## Replay

- **Opening** a record rebuilds the run to `end_tick` once and compares the state hash with `final_hash`.
  - A match is shown as *"Replay: matches the recorded run"*.
  - A mismatch (altered record, or a determinism fault) is shown in red; the replay still opens, and `nws-batch aar` exits with an error.
- **Keyframes:** while rebuilding, the state is kept every *k* ticks (*k* ≥ 60, at most 240 keyframes). Keyframes leave out the event log, so they cost about one save each, whatever the length of the run. (The model reads only the current tick's events.)
- **Seeking** to tick *t*: forward runs on from the cursor; backward, or a jump beyond the next keyframe, restarts from the keyframe at or before *t*. A seek is at most *k* ticks of simulation.
- **Playing** runs the replay at the chosen compression and pauses at the end.

## Desktop

The **Exercise review** section is shown to windows that control time: the control station and white-cell seats.

- **Replay:** reviews the live exercise so far. The live run pauses and is kept. Every window, including locked seat windows, shows the replay at the scrubber for its own seat. Orders, overrides and seat changes are refused until **Back to live**.
- **Open a record:** the record's scenario replaces the session (as loading a scenario does), then the record is replayed.
- **Save record:** the replay under review, or the live run so far. The webview saves it as a download; the app needs no filesystem permission.
- **Scrubber:** drag to any time; the time controls play the replay; the header shows **REPLAY**.
- **AAR tables:** an overlay with the tabs below. In a replay, clicking a timeline, loss or detection row jumps there. **Save CSV** exports the open tab.

## AAR tables

Built from the command log and the event log only, so they are the same for a live run, its replay and `nws-batch aar`.

| Table | Rows | Columns |
|---|---|---|
| **Summary** (`sides`) | One per side | Units at start, reinforcements (spawned later), destroyed, removed (white cell), remaining, strike weapons fired, hits, kills |
| **Losses** | One per unit destroyed or removed | Time, unit, side, platform, cause, side credited |
| **Weapons** | One per side × weapon × use | Fired, hits, missed, shot down, kills |
| **Detections** | One per contact gained | Side, contact number, target, gained, lost, seconds held |
| **Timeline** | Key moments | Time, kind, side concerned, text |

**Counting rules:**
- **Fired:** one per missile or torpedo launched, and one per gun, CIWS, bomb or depth-charge round and per mine strike (resolved at once). Interceptors are counted as **defence** rows: one per point-defence shot.
- **Hits:** hit rolls won, plus white-cell forced hits. For defence rows: threats killed.
- **Missed:** hit rolls lost, and weapons that missed for another reason (lost track, seeker never acquired, fuel, adjudicated miss). A weapon shot down is counted as **shot down**, not missed.
- **Kills:** credited to the last weapon to damage the unit, and its side, when the unit is destroyed, even by later fire or flooding. A unit destroyed by the white cell (destroy, or damage set to destruction) is credited to the white cell, not to a side. A removed unit is never a kill.
- Fired − hits − missed − shot down = weapons still in flight at the end.
- Hit rolls record the firing unit and side from save format 16. In older saves, gun hits are credited only when the side can be found.

**Timeline kinds:**
- `order`: a seat's order, applied; `refused`: refused, with the reason.
- `contact`: gained or lost.
- `launch`, `hit`, `miss`, `intercept`, `damage`, `loss`, `mine`.
- `override` and `facilitation`: white-cell actions, with the umpire.
- `event`: a scenario event fired, or a unit arrived.
- `script_error`: a scenario script or action failed.

Detection draws are not in the timeline; they are in the event log.

**Who sees what:** the white cell sees the whole timeline. A side seat at the control station sees only lines about its own side. The summary, loss, weapon and detection tables are ground truth and are shown only to windows that control time.

**CSV** follows RFC 4180:
- comma-separated, CRLF line ends, a header row;
- fields quoted when they hold a comma, quote or line break;
- text starting with `=`, `+`, `-` or `@` is prefixed with `'`, so spreadsheets do not run it as a formula.

## Batch

```
nws-batch aar <record.nwsrec | scenario.json> --out <dir> [--seed N] [--ticks N] [--db file]
```

- Writes `sides.csv`, `losses.csv`, `weapons.csv`, `detections.csv` and `timeline.csv`.
- Given a scenario, it runs it first and also writes `record.nwsrec`.
- Prints one JSON line (`verified`, losses, timeline length).
- Fails if the replay does not match.

## Limitations

- A record replays only on a build with the same model behaviour. A model change after the exercise makes `verified` false: the replay still opens, and the AAR still reflects the record's commands under the new model.
- Opening a record holds the whole run in memory once (the event log), as the live run did.
- No charts yet: the tables are exported for spreadsheets. Monte Carlo statistics over many runs are Phase 4.
