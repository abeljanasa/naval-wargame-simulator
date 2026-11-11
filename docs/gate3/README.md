# Gate 3 review pack: TTX release 1.0

Gate 3 (master prompt §13, §16): **a navy TTX pilot on the release build.** This page maps each part of the definition of done to its evidence, and lists what is still open, so the review panel can decide.

- **Branch:** `phase-3/ttx-release` (PR #3). Plan: [docs/plan/phase-3.md](../plan/phase-3.md).
- **Status (2026-11-11):** all nine Phase 3 slices are built. Three conditions remain:
  - the client's signing certificates;
  - the air-gapped run of the signed release build;
  - the TTX pilot itself.

  Some P1 features of §7 are partial (below).

## Definition of done (§16)

| Criterion | Evidence | Status |
|---|---|---|
| All P1 features in §7 work | Table below | **Partial**: the gaps are listed and in the backlog |
| The planning layer in §7.1 works | [planning-layer.md](../models/planning-layer.md), DL-0028, `phase3-planning-demo.json` | **Mostly**: see the §7.1 table |
| A white cell can run a multi-seat exercise with injects, overrides and AAR | [white-cell manual](../manuals/white-cell-manual.md); models: [multi-seat](../models/multi-seat.md), [injects](../models/injects.md), [adjudication](../models/adjudication.md), [replay-aar](../models/replay-aar.md); host tests and e2e | **Done** |
| Signed installers for Windows, macOS and Linux | Release workflow (`.github/workflows/release.yml`); macOS re-enabled; [releasing.md](../build/releasing.md) | **Waiting for the client's certificates.** Installers build unsigned until the Actions secrets are set. Linux packages ship with SHA-256 sums. |
| Installers run air-gapped | [air-gapped.md](../build/air-gapped.md): Phase 1 Windows bundle built and run with networking blocked; the app makes no network calls | **To repeat** on the release build, on a disconnected machine, for all three OSes |
| All CI checks are green | PR #3 checks (Windows, Linux and, from slice 9, macOS) | Green through slice 8; macOS first run with slice 9 |
| Manuals are complete | [User manual](../manuals/user-manual.md), [white-cell manual](../manuals/white-cell-manual.md), [scripting reference](../manuals/scripting-reference.md) (examples checked by `sim-scenario/tests/docs.rs`) | **Done** (review at the pilot) |
| A navy staff has run a TTX pilot on the release build | [TTX pilot plan](ttx-pilot-plan.md) | **Planned**: needs the client's staff and the signed build |

## P1 features of §7

| Subsystem | Built | Not yet (backlog) |
|---|---|---|
| World and map | Global offline map, bundled coastline, land avoidance, daylight model for sensors, zones and planning objects drawn | Satellite imagery, terrain elevation and bathymetry (GEBCO/SRTM packs), day/night shading on the map |
| Platforms and units | Ships, submarines, aircraft and land units as domains; facilities (planning layer); damage, fires, flooding; magazines; loadouts restored by repair and rearm | Airbases and air operations (Phase 4); **fuel**; aircraft loadout choice |
| Sensors | Radar (horizon, clutter, jamming), active/passive sonar, ESM, visual, EO/IR, MAD; classification and position uncertainty | Terrain masking, SVP acoustics (P2) |
| Weapons and engagements | Missiles, guns, torpedoes, mines, decoys; Ph and damage; point defence and soft kill | Weapons in flight as engageable targets for every sensor |
| Command and control | Sides and postures, doctrine (side, group, unit, zone), ROE, WCS, EMCON; command posts with range, subordinates, alternates and handover | **Comms links and their disruption** (jamming, relays) |
| Missions and AI | Patrol, strike, escort, ferry; automated engagement; route planning round land; mine laying (order); diversion for repair; engage-zone shooters | Support, mining and mine-clearing missions, cargo; tactics beyond the release rule |
| Scenario editor | Units, sides, postures, missions, minefields, reference points, planning layer, environment; events, injects and seats (JSON with templates); Lua syntax checking; validation on save | Forms for event triggers and actions; polygon drawing |
| TTX facilitation | Ground truth, view switching, overrides with umpire and reason, injects (scheduled and manual), time control for all seats, multi-seat windows, per-zone WCS override | — |

## §7.1 operational planning layer

| Feature | Built | Not yet |
|---|---|---|
| Command posts | Ashore or carried by a unit; subordinates; range; alternate after a configurable handover; orders in the gap lost or queued | Comms links and jamming of the post; airborne posts as a specific case |
| Refuelling points | Facility with services and berths, drawn and saved | Fuel, so nothing to refuel yet; tankers; queues for fuel |
| Sheltering ports | Berths, repair and rearm; damaged AI units divert there and resume their mission | Draft limit (bathymetry); weather-threatened units |
| Anchorages | Facility with radius, services and berths | Polygon areas with depth and holding limits; units holding there between missions |
| Choke points | A named pair of sense and engage zones; transits logged for the AAR | Bathymetry and shipping lanes; barrier-patrol, mining and denial missions on the choke point |
| Sense zones | Own-unit doctrine (EMCON, WCS) inside; alerts on contacts gained | Layers by sensor type; coverage and gaps drawn on the map |
| Engage zones | Per-zone ROE and WCS; assigned shooters; white-cell override per zone | — |
| Surface action groups | Leader and stations; group doctrine; detach by clearing a mission | Coordinated air defence; shared single mission object |

## Evidence by slice

| Slice | Design log | Behaviour doc | Commit(s) |
|---|---|---|---|
| 1 White-cell adjudication | [DL-0021](../design-log/DL-0021-adjudication.md) | [adjudication.md](../models/adjudication.md) | `47dcf48`, `c73103c` |
| 2 Event engine | [DL-0022](../design-log/DL-0022-events.md) | [events.md](../models/events.md) | `d3438c3`, `f8b11ee` |
| 3 Injects | [DL-0023](../design-log/DL-0023-injects.md) | [injects.md](../models/injects.md) | `1e91890`, `c538dfa` |
| 4 Lua scripting | [DL-0024](../design-log/DL-0024-lua.md) | [lua-scripting.md](../models/lua-scripting.md) | `cf2299e` |
| 5 Multi-seat play | [DL-0025](../design-log/DL-0025-multi-seat.md) | [multi-seat.md](../models/multi-seat.md) | `aab6a0d`, `6f041e7` |
| 6 Replay and AAR | [DL-0026](../design-log/DL-0026-replay-aar.md) | [replay-aar.md](../models/replay-aar.md) | `2f4a538`, `89bb24b` |
| 7 Scenario editor | [DL-0027](../design-log/DL-0027-scenario-editor.md) | [scenario-editor.md](../models/scenario-editor.md) | `d4ba5cb`, `abe0d6b` |
| 8 Planning layer | [DL-0028](../design-log/DL-0028-planning-layer.md) | [planning-layer.md](../models/planning-layer.md) | `1bfe2e9`, `000ed1e`, `cea2707`, `77ccc5c` |
| 9 Gate 3 pack | — | this page, the manuals, the pilot plan | this commit |

## Whole-system checks

| Check | What it shows |
|---|---|
| `every_scenario_is_deterministic_per_seed`, golden replays, CI determinism matrix | Every shipped scenario gives identical hashes on every OS |
| `ttx_demo_*` (sim-scenario) and `ttx_demo_timeline_shows_events` (sim-aar) | The TTX demonstrator's events fire once each; the AAR shows them |
| `record_round_trips_and_replays_exactly`, `seeking_matches_a_straight_run`, `a_tampered_record_is_reported` | Records replay exactly; the scrubber matches a straight run; tampering is caught |
| `report_accounts_for_every_shot_and_loss` | The AAR counts every launch, hit, intercept and loss |
| `planning_layer_and_action_groups_load_build_and_hold_station`, planning tests in `sim-core/tests/combat.rs` | Every planning behaviour, by hand-checkable scenarios |
| e2e smoke (`npm run e2e`) | The real UI: seats, injects, overrides, replay and AAR, editor, zone override |

## Decisions for the panel

1. Accept the P1 gaps above for the pilot (fuel, comms links, bathymetry, extra missions), or require some before it.
2. Provide the code-signing certificates (Windows PFX; Apple Developer ID and notarisation account) as Actions secrets.
3. Name the navy staff and dates for the pilot ([ttx-pilot-plan.md](ttx-pilot-plan.md)).
