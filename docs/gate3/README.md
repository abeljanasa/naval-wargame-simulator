# Gate 3 review pack: TTX release 1.0

Gate 3 (master prompt §13, §16): **a navy TTX pilot on the release build.** This page maps each part of the definition of done to its evidence, and lists what is still open, so the review panel can decide.

- **Branch:** `phase-3/ttx-release` (PR #3). Plan: [docs/plan/phase-3.md](../plan/phase-3.md).
- **Status (2026-11-12):** all nineteen Phase 3 slices are built.
  - **Slices 1–9:** the TTX release.
  - **Slices 10–17:** close the in-phase P1 gaps the first version of this pack listed.
  - **Slices 18–19:** this pack brought up to date, and a pilot scenario that uses the whole planning layer.
  - **Moved to Phase 4 at the client's choice (2026-11-12),** because they need outside data or a Phase 4 subsystem (§7 below):
    - satellite imagery;
    - terrain and bathymetry data, and what depends on them;
    - air operations.
- **Three conditions remain:**
  - the client's signing certificates;
  - the air-gapped run of the signed release build;
  - the TTX pilot itself.

## Definition of done (§16)

| Criterion | Evidence | Status |
|---|---|---|
| All P1 features in §7 work | Table below | **Done in phase.** Imagery, GEBCO/SRTM-dependent features and air operations are deferred to Phase 4 by the client's decision |
| The planning layer in §7.1 works | [planning-layer.md](../models/planning-layer.md), [comms.md](../models/comms.md), [fuel.md](../models/fuel.md), DL-0028 to DL-0035, `phase3-planning-demo.json` | **Done**, except depths from bathymetry and shipping lanes (Phase 4) |
| A white cell can run a multi-seat exercise with injects, overrides and AAR | [white-cell manual](../manuals/white-cell-manual.md); models: [multi-seat](../models/multi-seat.md), [injects](../models/injects.md), [adjudication](../models/adjudication.md), [replay-aar](../models/replay-aar.md); host tests and e2e | **Done**, now with live planning-layer edits (slice 17) |
| Signed installers for Windows, macOS and Linux | Release workflow (`.github/workflows/release.yml`); macOS re-enabled; [releasing.md](../build/releasing.md) | **Waiting for the client's certificates.** Installers build unsigned until the Actions secrets are set. Linux packages ship with SHA-256 sums. |
| Installers run air-gapped | [air-gapped.md](../build/air-gapped.md): Phase 1 Windows bundle built and run with networking blocked; the app makes no network calls | **To repeat** on the release build, on a disconnected machine, for all three OSes |
| All CI checks are green | PR #3 checks (Windows, Linux, macOS) | **Green** on the push of slices 10–18 and the desktop layout (2026-11-12) |
| Manuals are complete | [User manual](../manuals/user-manual.md), [white-cell manual](../manuals/white-cell-manual.md), [scripting reference](../manuals/scripting-reference.md) (examples checked by `sim-scenario/tests/docs.rs`) | **Done**, updated for slices 10–17 (review at the pilot) |
| A navy staff has run a TTX pilot on the release build | [TTX pilot plan](ttx-pilot-plan.md) | **Planned**: needs the client's staff and the signed build |

## P1 features of §7

| Subsystem | Built | Phase 4 (client's decision, 2026-11-12) |
|---|---|---|
| World and map | Global offline map, bundled coastline, land avoidance, daylight model for sensors, **day/night shading on the map** (slice 17), zones and planning objects drawn, **polygon areas** | Satellite imagery (needs a licensed source); terrain elevation and bathymetry (GEBCO/SRTM packs) |
| Platforms and units | Ships, submarines, aircraft and land units as domains; facilities; damage, fires, flooding; magazines; loadouts restored by repair and rearm; **fuel for ships and submarines** (slice 11) | Airbases and air operations; aircraft fuel and tanking; aircraft loadout choice |
| Sensors | Radar (horizon, clutter, jamming), active/passive sonar, ESM, visual, EO/IR, MAD; classification and position uncertainty; **weapons in flight as targets for every sensor** (slice 15) | Terrain masking (GEBCO/SRTM); SVP acoustics (P2) |
| Weapons and engagements | Missiles, guns, torpedoes, mines, decoys; Ph and damage; point defence and soft kill; point defence on any own or **group** track (slices 15–16) | — |
| Command and control | Sides and postures, doctrine (side, group, unit, zone), ROE, WCS, EMCON; command posts with range, subordinates, alternates and handover; **comms links with radio horizon, jamming, relays and disruption** (slice 10) | — (data-link picture sharing and cyber are Phase 5) |
| Missions and AI | Patrol, strike, escort, ferry, **support, mining, mine clearing, cargo, barrier patrol, denial** (slice 12); automated engagement; route planning round land; diversion for repair, fuel, weather and staging; engage-zone shooters | Tactics beyond the release rule (advanced AI, Phase 5) |
| Scenario editor | Units, sides, postures, missions, minefields, reference points, planning layer, environment; **forms for events** (slice 17); **polygon drawing**; Lua syntax checking; validation on save | — (injects, seats and groups stay JSON) |
| TTX facilitation | Ground truth, view switching, overrides with umpire and reason, injects (scheduled and manual), time control for all seats, multi-seat windows, per-zone WCS and EMCON override, **live planning-layer edits** (slice 17) | — |

## §7.1 operational planning layer

| Feature | Built | Phase 4 |
|---|---|---|
| Command posts | Ashore, afloat or airborne; subordinates; range; comms links (radio horizon, jamming, relays); a jammed or lost post hands over to its alternate after a configurable delay; orders in the gap lost or queued | — |
| Refuelling points | Depots and oilers with stock and transfer rate; units below bingo route to the nearest; oilers assigned automatically by nearest point; queues when berths are full | Aviation fuel and tanking (air operations) |
| Sheltering ports | Berths, repair, rearm, depth (draft) limit; damaged and weather-threatened units divert there and resume | Depths from bathymetry |
| Anchorages | Polygon areas with depth and holding (sea-state) limits; units hold there between missions (staging) | Depths from bathymetry |
| Choke points | Named sense and engage zones; transits logged for the AAR; barrier-patrol, mining and denial missions on the choke point | Inherited bathymetry and shipping lanes |
| Sense zones | Layers (outer, middle, inner) by sensor type; own-unit doctrine inside; alerts on contacts; **coverage and gaps drawn on the map**; players task sensors | Automatic sensor placement and tasking (analysis phase) |
| Engage zones | Per-zone ROE and WCS; assigned shooters; white-cell override per zone | — |
| Surface action groups | Engine objects: leader and stations, group doctrine, one shared mission, succession, detach and join in play, shared air picture and SAM fire deconfliction | — |

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
| 9 Gate 3 pack | — | this page, the manuals, the pilot plan | `1481b34` |
| 10 Comms links | [DL-0029](../design-log/DL-0029-comms.md) | [comms.md](../models/comms.md) | `34e8439` |
| 11 Fuel and refuelling | [DL-0030](../design-log/DL-0030-fuel.md) | [fuel.md](../models/fuel.md) | `e2fcafb` |
| 12 More missions | [DL-0031](../design-log/DL-0031-missions.md) | [missions.md](../models/missions.md) | `249ff16` |
| 13 Anchorages and weather | [DL-0032](../design-log/DL-0032-anchorages-weather.md) | [planning-layer.md](../models/planning-layer.md) | `ba07e0f` |
| 14 Sense-zone coverage | [DL-0033](../design-log/DL-0033-coverage.md) | [planning-layer.md](../models/planning-layer.md) §Coverage | `cde611b` |
| 15 Weapons as sensor targets | [DL-0034](../design-log/DL-0034-weapon-tracks.md) | [weapons-damage.md](../models/weapons-damage.md) | `0808fa2` |
| 16 Action groups in the core | [DL-0035](../design-log/DL-0035-action-groups.md) | [planning-layer.md](../models/planning-layer.md) | `5aaea45` |
| 17 Editing and facilitation | [DL-0036](../design-log/DL-0036-desktop-facilitation.md) | [scenario-editor.md](../models/scenario-editor.md) | `cb7bdbb` |
| 18 Pack update | — | this page, the manuals, the backlog | `3846541` |
| 19 Pilot scenario | — | [ttx-pilot-plan.md](ttx-pilot-plan.md) §4 | this commit |

## Whole-system checks

| Check | What it shows |
|---|---|
| `every_scenario_is_deterministic_per_seed`, golden replays, CI determinism matrix | Every shipped scenario gives identical hashes on every OS. The planning demonstrator's golden was re-blessed in slices 10, 11, 13, 14 and 16, each time with its timeline checked (the reasons are in each design log) |
| `ttx_demo_*` (sim-scenario) and `ttx_demo_timeline_shows_events` (sim-aar) | The TTX demonstrator's events fire once each; the AAR shows them |
| `pilot_scenario_reaches_every_decision_point` (sim-aar), `a_clean_scenario_has_only_warnings_and_builds` | The pilot scenario reaches every decision point of the pilot plan in order, replays exactly, and has no editor warnings |
| `record_round_trips_and_replays_exactly`, `seeking_matches_a_straight_run`, `a_tampered_record_is_reported` | Records replay exactly; the scrubber matches a straight run; tampering is caught |
| `report_accounts_for_every_shot_and_loss` | The AAR counts every launch, hit, intercept and loss |
| `planning_layer_and_action_groups_load_build_and_hold_station`, planning tests in `sim-core/tests/combat.rs` | Every planning behaviour, by hand-checkable scenarios: comms, fuel, missions, anchorages and weather, coverage, weapon tracks, action groups (52 engine tests in that file) |
| e2e smoke (`npm run e2e`) | The real UI: seats, injects, overrides, replay and AAR, editor (event forms), zone override |

## Decisions for the panel

1. Confirm the Phase 4 deferrals above:
   - imagery (a licensed source is needed);
   - GEBCO/SRTM terrain and bathymetry, and the terrain masking, depths and shipping lanes that depend on them;
   - air operations.
2. Provide the code-signing certificates (Windows PFX; Apple Developer ID and notarisation account) as Actions secrets.
3. Name the navy staff and dates for the pilot ([ttx-pilot-plan.md](ttx-pilot-plan.md)).
4. Review the draft [Phase 4 plan](../plan/phase-4.md), and make its decisions: the primary region of interest, the imagery source, and the historical engagements for Gate 4.
