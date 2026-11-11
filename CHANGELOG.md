# Changelog

## [Unreleased]

### Added (Phase 3, slice 1: white-cell adjudication)

- **Phase 3 plan** (`docs/plan/phase-3.md`): white cell and injects first, at the client's choice.
- **`Adjudicate` command** (white cell only, reason required; `docs/models/adjudication.md`, DL-0021):
  - Force a weapon in flight to hit or miss, set or repair damage, destroy or remove a unit, move a unit, set stores.
  - Every override is logged with umpire and reason before it takes effect, saved, and replays exactly.
- **Desktop white-cell console:** reason box; Repair, Destroy and Remove for the selected unit; Hit and Miss for weapons in flight; the override log.

### Added (Phase 3, slice 2: event engine)

- **`sim-events` crate and the event engine** (`docs/models/events.md`, DL-0022):
  - Triggers: time, every, area entered, unit destroyed, contact gained.
  - Conditions: event fired or not, unit alive or gone, units alive, time window, probability.
  - Actions: message, posture, doctrine, environment, speed, route, mission, spawn a reserve, enable an event.
  - Checked once per tick in id order. Firings, held events and failed actions are logged.
- **Scenario format 3:** `events` and `reserve` units.
- **Message logs per side** in every picture.
- **Desktop Messages panel.**
- **New demonstrator:** `data/scenarios/phase3-ttx-demo.json`, an escalation exercise.

### Added (Phase 3, slice 3: injects)

- **Inject library in scenarios** (`docs/models/injects.md`, DL-0023): role-play sender, audience, text, and a white-cell note.
- **Scheduled injects:** the `send_inject` event action.
- **White-cell `Facilitate` command:** send an inject or a free-text message, fire an event, turn an event on or off. Logged with the umpire.
- **White-cell picture:** the inject library with send counts, and every event's state.
- **Desktop white-cell console:**
  - Injects (Send).
  - Events (enable, Fire).
  - Message to.
  - Inject titles in the Messages panel.

### Added (Phase 3, slice 4: Lua scripting)

- **Lua 5.4 conditions and actions on events** (`docs/models/lua-scripting.md`, DL-0024):
  - Built on mlua, vendored, so it works air-gapped.
  - A `sim` API: read time, events, units and counts; draw from the simulation stream; ask for messages, injects, event switches, postures, speeds and reserves.
- **Sandbox:**
  - Only `table`, `string`, `math` and `utf8`; no file, OS, module, debug, coroutine, `load` or `pcall` access.
  - Text only; 1 M instructions and 16 MiB per run; 64 KiB per script.
  - Escape tests cover all of this.
- **Determinism replacements:** sorted `pairs`, address-free `tostring` and `format`, `libm` maths, `sim.random()`, no finalizers or weak tables.
- **Errors:** script errors are logged, and the run continues. Syntax errors stop the scenario loading.
- **Demonstrator:** gains a scripted umpire brief, which the cross-OS golden replay runs.

### Added (Phase 3, slice 5: multi-seat play)

- **Seat table** (`docs/models/multi-seat.md`, DL-0025):
  - White-cell and side seats, declared in the scenario or defaulted (one white-cell seat plus one per side).
  - Set by the logged `SetSeats` command.
  - The core refuses player commands from the wrong seat or side, and umpire commands not from a white-cell seat.
- **Desktop seat windows:**
  - The main window is the control station and may sit at any seat.
  - The white cell opens windows locked to a seat, each with its own picture.
  - Only the main window and white-cell seats run the clock.
- **Seat management** in the white-cell console: add, remove, open a window.

### Added (Phase 3, slice 6: replay and AAR)

- **`sim-aar` crate** (`docs/models/replay-aar.md`, DL-0026):
  - **Exercise records** (`.nwsrec`): seed, coastline, command log, final hash, and the scenario.
  - **Replay:** rebuilt from the record and checked against the final hash, with keyframes for seeking.
  - **AAR tables:** side summary, losses with cause and credit, weapons expended against effective (fired, hits, missed, shot down, kills), detections, and a timeline. CSV export (RFC 4180, formula-safe).
- **Desktop exercise review:**
  - Replay the live exercise, or open a saved record.
  - Scrubber; every seat window follows the replay; orders are refused until Back to live.
  - AAR overlay with click-to-jump and Save CSV.
- **`nws-batch aar`:** replays a record, or runs a scenario and records it, then writes the five CSV tables. It fails if the replay does not match.
- **Save format 16:** hit rolls record the firing unit and side (golden hashes unchanged).

### Added (Phase 3, slice 7: scenario editor)

- **Desktop scenario editor** (`docs/models/scenario-editor.md`, DL-0027):
  - Edit the loaded scenario, start a new one, or edit a file.
  - Forms for the overview and environment; sides and postures; units (platform, position, heading, speed, route, mission, reserve); minefields; and reference points.
  - Map picking for positions, routes, patrol centres, destinations and minefields.
  - Events, injects, seats and groups as JSON, with event templates.
  - The map shows the draft while editing.
- **Validation by the engine:** `Scenario::check` lists every error and warning at once. Errors block Save and Play.
- **Scenario format 4:** `reference_points`, shown on the map to the seats that may see them.

### Added (Phase 3, slice 8: operational planning layer)

- **Planning layer** (`docs/models/planning-layer.md`, DL-0028):
  - **Command posts** ashore or carried by a unit, with subordinates and range. When a post's unit is lost, an alternate takes over after a handover delay, and orders in the gap are lost or queued.
  - **Damaged units divert:** mission-killed units on a mission go to the nearest repair port, and resume the mission when repaired.
  - **Engage-zone shooters** engage contacts inside the zone on their own.
  - **Choke-point transits** are logged.
  - **Action-group doctrine.**
  - **White-cell per-zone weapons control override.**
  - **Sheltering ports, anchorages and refuelling points:** repair, rearm and berths for units alongside. A full repair restores the sensors and speed lost to critical hits.
  - **Sense zones:** doctrine for own units inside, and alerts on new contacts.
  - **Engage zones:** per-zone WCS and ROE for targets inside.
  - **Choke points:** a sense zone round an engage zone.
  - **Surface action groups:** members keep station on the leader.
- **Scenario format 5** (`facilities`, `zones`, `action_groups`); **save format 17** (planning layer, unit loadouts).
- **Desktop:**
  - The planning layer is drawn on the map for each seat.
  - Planning tab in the scenario editor.
  - Alerts, repairs and rearming in the AAR timeline.
- **New demonstrator:** `data/scenarios/phase3-planning-demo.json`, with its golden replay. Existing golden hashes are unchanged.

### Changed

- Save format 15 (events, injects, messages, seats).
- Golden hashes: phase 1 `dd942f76e85bd4f2`, phase 2 `cb360f9a36c8bc87`, phase 3 `db033be444af12f8`.
- Desktop: `set_viewer` is replaced by `set_seat`. Time-control commands now return errors to seats that may not use them.
- CI: the air-gapped job builds and tests `sim-events`.
- Save format 13 (adjudication log).

### Added (Phase 2, slice 12: Gate 2 pack)

- **Combat demo scenario** `data/scenarios/phase2-surface-action.json`: Blue destroyer and frigate against two Red missile corvettes in Bass Strait, weapons free, with a Red minefield and a neutral merchant. It is included in the golden replays and the cross-OS determinism matrix.
- **Gate 2 scenario test** `gate2_demo_detects_engages_defends_and_damages`: both sides fire, point defence and decoys engage, hits do damage, a unit is destroyed, the minefield is run, and the neutral is untouched.
- **Duel-test index** `docs/gate2/duel-tests.md`: every model's duel and hand-value tests.

### Added (Phase 2, slice 11: basic mines)

- **Minefields** (`docs/models/weapons-damage.md` §Mines, DL-0020):
  - Created by the scenario or white cell, or laid from a ship's stores.
  - Ships and submarines of other sides moving through a field run the simple-initial-threat risk P = 1 − exp(−ρWd) every 10 s. An encounter rolls the mine's Ph and damage.
  - The owning side is immune and sees its fields; the desktop draws them.
- Duel test: 200 transits match the hand-calculated 0.471 encounter rate.

### Changed

- Save format 12 (older saves still load). Golden hash is now `1f07e67a956cb372`.

### Added (Phase 2, slice 10: basic missions)

- **`sim-ai` crate and AI system** (`docs/models/missions.md`, DL-0019):
  - Patrol (random points over a circle, logged draws), ferry, escort station-keeping and strike missions, run every 10 s.
  - Tasked units engage automatically: nearest contact first, with the first weapon that passes every `Engage` check, release rule included.
- `AssignMission` command; optional `mission` on scenario units.
- **Desktop:** the unit panel shows the mission and offers "Patrol here" and "Clear".

### Changed

- Save format 11 (older saves still load). Golden hash is now `b296e935e3375df2`.

### Added (Phase 2, slice 9: point defence and soft kill)

- **Point defence** (`docs/models/weapons-damage.md`, DL-0018):
  - Ships with a radiating radar detect incoming missiles and engage them after a reaction time.
  - SAMs defend the area; guns and CIWS defend their own ship. Fire channels are limited, so saturation emerges.
  - Interceptors resolve at the intercept time with a logged Ph draw. Point defence works under any WCS.
- **Soft kill:**
  - Targets fire matched decoys (chaff or IR against missiles, countermeasures against torpedoes); the database decoy effect lowers Ph.
  - A radiating jammer on the target lowers an active radar seeker's Ph by × 0.8. Hit rolls log the countermeasure modifier.

### Changed

- Save format 10 (older saves still load).

### Added (Phase 2, slices 7 and 8: weapons, Ph and damage)

- **`sim-weapons` crate and combat engine** (`docs/models/weapons-damage.md`, DL-0016, DL-0017):
  - **`Engage` orders** are checked against the shooter's condition, magazine, target domain, track quality for the guidance type, range, and the WCS/ROE release rule (the white cell may override).
  - **Missiles and torpedoes** fly toward the side's current track estimate. A seeker gets one acquisition draw when the real target is in range, then homes. Semi-active and command weapons need the track held to impact.
  - **Guns, CIWS, bombs, depth charges and rockets** resolve at once.
  - **Ph** = base × size × manoeuvre × sea-state modifiers.
  - **Damage:** damage points from the warhead, and critical hits on sensors, propulsion, weapons, flight deck or comms. Fire and flooding burn every minute until damage control wins. Units become mission-killed or destroyed.
  - Every draw is logged with its inputs.
  - **Gate 2 duel test:** 300 engagements match the hand-calculated 0.72 hit rate.
- **Database queries** for a platform's stores (magazines, or the first loadout for aircraft), weapon figures and damage profile. The scenario loader sends the weapon catalogue.
- **Desktop:**
  - The selected unit shows its condition, damage, criticals and stores, plus an Engage form (target, weapon, rounds).
  - Weapons in flight appear on the map.
  - Refused orders show the engine's reason.

### Changed

- Save format 9 (older saves still load).
- Golden hash is now `1e874e39cf5d2dbe`.

### Added (Phase 2, slice 6: visual, EO/IR and MAD)

- **Visual, EO/IR and MAD** (`docs/models/optical-mad.md`, DL-0015):
  - Daylight from solar elevation at the scenario's start time.
  - Optical range set by the sensor, target size, light and visibility. IR works at night and sees further through haze.
  - Close looks identify to category or class, and class reveals affiliation.
  - Visual gives positions; EO and IR give precise bearings that join the fix pool.
  - MAD on aircraft finds submarines within about a kilometre of slant range.
- Environment gains a start time (UTC) and visibility. Units carry their category and length from the database.

### Changed

- Save format 8 (older saves still load).
- The demo starts at 2026-11-10 00:00 UTC, in daylight. Golden hash is now `a6d33e91f79e97ea`.

### Added (Phase 2, slice 5: sonar)

- **Sonar** (`docs/models/sonar.md`, DL-0014):
  - Passive SE = SL − TL − (NL − DI) − DT and active (noise-limited) sonar equations. Pd = Φ(SE / 5 dB).
  - Spherical-then-cylindrical spreading with Thorp absorption.
  - Speed-dependent radiated noise and self-noise; ambient noise set by sea state.
  - Active sonar transmits under EMCON active, and a pinging ship is heard at its ping level.
  - Active detections feed tracks. Passive bearings join the ESM pool and triangulate into fixes.
  - Dipping sonar needs a low hover; sonobuoys wait for missions.
- Every sonar attempt logs each term of the sonar equation.

### Changed

- Save format 7 (older saves still load). `UnitSpawn.radiated_noise_db` and `SensorSpec.active` are added.
- Golden hash is now `66e6a556f1ac1068`.

### Added (Phase 2, slice 4: ESM)

- **ESM** (`docs/models/esm.md`, DL-0013):
  - Passive intercept of radiating radars and jammers, out to 2.5× the radar's reference range within the horizon. Draws and bearing errors are logged.
  - Bearings from two or more platforms in one scan triangulate into a fix that feeds or creates a track.
  - An emitter library from the platform database identifies the carrier to class, category or domain. Class-level identification reveals affiliation.
- **Engine:** `SetEmitterLibrary` command, issued by the scenario loader. Contacts carry category and class. Snapshots carry the side's bearing lines.
- **Desktop:** ESM bearing lines on the map; contacts named by class, category or domain.
- **Performance:** an exact chord-distance pre-check and per-unit geodesic reuse in the radar and ESM passes. 1,000 units run at 451× real time and 10,000 at 5.0× (budgets 60× and 1×). Results are unchanged.

### Changed

- Save format 6 (older saves still load).
- Golden hash is now `e32c602bbebfedbb`.

### Added (Phase 2, slice 3: radar)

- **Radar model** (`docs/models/radar.md`, DL-0012):
  - Sea and land clutter shorten detection range against low targets, scaled by sea state.
  - A scenario-wide weather loss applies to every radar.
  - Noise jamming, self-screening and stand-off, with main-beam and sidelobe J/N. Burn-through emerges as range closes.
  - Jammers radiate only while EMCON is active.
- **Environment:** `SetEnvironment` (scenario and white cell) sets sea state and weather. Every side sees it, and the desktop header shows the sea state.
- **Database migration 0002:** radar `clutter_db_per_sea_state` and `beamwidth_deg`, filled for all 15 generic radars, with two new validation rules.
- **Scenario format:** optional `environment` and per-unit `jammer`.
- Detection events log clutter loss, weather loss and J/N.

### Changed

- Save format 5 (older saves still load).
- The demo scenario runs in sea state 3. Golden hash is now `271dd129bbb9548d`.

### Added (Phase 2, slice 2: command and control)

- **`sim-c2` crate:** postures, doctrine inheritance (unit, then group, then side), the WCS weapons-release rule (safe, tight, free) with the ROE `engage_unfriendly` setting, and EMCON (model doc `docs/models/c2-doctrine.md`, DL-0011).
- **Engine:**
  - `SetPosture` (scenario and white cell only) and `SetDoctrine` (side, group or unit) commands.
  - EMCON silent switches a unit's radar off.
  - Snapshots carry each unit's doctrine, the viewer's postures, and contact affiliation once a track is identified.
- **Scenario format:** optional `postures`, side `doctrine`, `groups`, and unit `group` and `doctrine`.
- **Desktop:** weapons (WCS) and emissions (EMCON) selectors on the selected unit, showing what is inherited. Contacts are coloured by affiliation once identified.

### Changed

- Save format 4 (older saves still load).
- The demo scenario sets both sides neutral and Blue weapons safe. Golden hash is now `d46bae3f7ea350a8`.

### Added (Phase 2, slice 1: tracks)

- **Phase 2 plan** (`docs/plan/phase-2.md`) and **ADR-0005**: model crates hold pure functions, and `sim-core` runs them.
- **`sim-sensors` crate:** the radar Pd model moved here, plus measurement error, track fusion and the classification ladder.
- **Tracks:**
  - A detection reports a position with drawn range and bearing error, logged on the detection event.
  - Each side fuses every detection of a target into one track (Kalman update). The error grows between updates by the domain's speed bound.
  - Radar classifies tracks to their domain.
  - Model doc `docs/models/tracks.md` (DL-0010).
- **Desktop:** contacts show the estimated position, a 95 % error ellipse on the map, their domain and their error in the Contacts panel.

### Changed

- Save format 3 (v2 saves still load).
- Golden hash for the Phase 1 scenario is now `8a1f96e397c0a0b7`: detections draw their measurement errors.

### Added (Phase 1, close-out)

- **Turn rates:** units turn at ω = min(cap, v/r_min) and reach a waypoint within their turning radius. Generic class values come from scenario resolution (ships r_min = 2.5 × length).
- **Land avoidance:** Natural Earth 50m land rasterised at 1/30°, A* routes round the coast with a coastal penalty and string-pulling, and a grounding guard. Blocked routes are rejected and logged (DL-0008).
- **Desktop:**
  - APP-6-style frames per domain (surface, air, subsurface, land) and affiliation.
  - Light theme alongside the default dark theme.
  - Load a scenario from a file.
  - Snapshot diffs on the UI stream: a full picture first, then only changed and removed units.
  - **Platform database browser:** search, record detail with citation, confidence and marking, and validation feedback (DL-0009).
- **Front-end tests:** Vitest unit tests and a Playwright end-to-end smoke test against a mocked backend, both in CI.

### Changed

- CI and release builds skip macOS for now (the jobs are commented, not deleted).
- Golden hash for the Phase 1 scenario is now `7ad884a980341eb1` (turn rates and land mask).

### Added (Phase 1, remaining delivery items)

- **Criterion benchmarks** (`cargo bench -p sim-scenario --bench engine`), with a PR CI job that fails on a > 15 % regression against the base commit, measured on the same runner.
- **Map rendering benchmark** (`npm run bench:map`): the production map code with 10,000 symbols at 30 Hz updates reaches 59.2 fps on integrated graphics.
- **Map decluttering:** above 1,500 units, name labels and heading leaders are drawn only for the selected unit; with every label drawn it fell to 27.8 fps. Symbol URLs are cached, and the label glyph set is fixed.
- **npm licence audit** of production dependencies (`npm run licenses`, in CI).
- The synthetic benchmark scenario moved to `sim_scenario::synthetic`. The output is unchanged (bench hash `14c631676919c2af`).

### Fixed

- The map's coastline URL is now root-relative, so pages outside the site root can load it.

### Added (Phase 1, delivery)

- **Performance:** an exact latitude broad phase in detection with bit-identical results. 1,000 units run at 203× real time and 10,000 at 2.2× (budgets: 60× and 1×). Measure with `nws-batch bench`.
- **Release scan:** `sim-db scan <dirs>` checks database files against `data/db/release-allowlist.txt`, recognising them by header, and fails on any record or JSON classification other than UNCLASSIFIED.
- **Dependency policy** (`deny.toml`): no GPL/AGPL/LGPL; `cargo deny check` passes.
- **CI:** dependency audit, a performance gate, and an air-gapped (vendored, `--offline`) build.
- **Release workflow:** MSI/NSIS, DMG, AppImage/deb/rpm installers, signed when secrets are configured, with the release scan and CycloneDX SBOMs (Rust and npm).
- **Docs:** `docs/build/performance.md`, `releasing.md`, `air-gapped.md`.

### Added (Phase 1, Gate 1: units move and detect)

- **Scenario format v2:**
  - A unit can name a platform class (`"platform": "GEN-FFG"`) that supplies its performance, sensors and RCS. Inline values override it, and v1 files still load.
  - The resolved values travel in the spawn command, so replays don't depend on the database.
- **Embedded open database** (`sim_db::embedded`): the desktop app and batch runner need no data files. `nws-batch --db <file>` loads a built database instead.
- **Interim radar detection model:**
  - Runs every 5 s: radar horizon, RCS^¼ range scaling, Pd = 1/(1+(r/R₅₀)⁸), one seeded draw per scan.
  - Every draw is logged with its inputs.
  - Per-side contacts are dropped after 300 s without a detection.
  - Side snapshots show contacts without target ids.
  - Duel tests check it against hand calculation (model doc `docs/models/radar-detection-gate1.md`, DL-0006).
- **Desktop:** contacts on the map and in a Contacts panel, and the unit's class in the details panel.
- **Demo scenario:** the bulk carrier now transits eastbound, so the two ships meet and detect each other. A Gate 1 test asserts both move and both detect.

### Changed

- **Save format v2:** adds sensors, signatures and contacts. v1 saves still load.
- **Golden hash re-blessed** (`92931f90968e0415`): engine state now includes platforms, sensors, RCS, contacts and detection draws, and the demo scenario uses database classes.

### Added (Phase 1, platform database)

- **`sim-db`:**
  - SQLite schema (migration 0001) with provenance columns on every value table.
  - Forward-only migrations tracked by `user_version`; newer and outdated schemas are refused.
  - A JSON source loader (ADR-0004).
  - A validation suite of 34 rules, each with a seeded-fault test.
  - `PlatformDb` loading through `DbSource::Open`. `DbSource::Accredited` sits behind the `accredited` feature.
  - CLI: `validate`, `build`, `migrate`, `list`.
- **Open generic database** (`data/db/open`): 50 platforms, 33 sensors, 27 weapons, 7 comms systems, 12 propulsion types and 18 damage profiles. All are generic and UNCLASSIFIED, with confidence D.
- **Docs:** ADR-0004, the data dictionary (`docs/db/data-dictionary.md`), DL-0005.

### Added (Phase 1, first tranche)

- **ADRs:** ADR-0001 (tech stack), ADR-0002 (Svelte 5 UI), ADR-0003 (determinism strategy).
- **Workspace and lints:** a Cargo workspace with `forbid(unsafe_code)`, and clippy rules that ban non-deterministic APIs (`HashMap`, the wall clock, `std` transcendental maths).
- **`sim-geo`:** WGS-84 Vincenty direct and inverse on `libm`, and the radar horizon, tested against published reference values.
- **`sim-core`:**
  - ECS world with a fixed 1 s tick and per-subsystem ChaCha8 RNG streams.
  - Versioned commands with authorisation by seat, plus a command log and replay.
  - Viewer-filtered snapshots, the state hash, MessagePack saves with a version check, and waypoint movement.
- **`sim-scenario`:** the JSON scenario format (v1), validation, determinism tests and golden-replay tests.
- **`nws-batch`:** a headless runner that prints final state hashes.
- **Desktop app (Tauri 2 + Svelte 5 + MapLibre + deck.gl):**
  - Offline Natural Earth coastline, unit symbols, routes and heading leaders.
  - Pause, step, time compression (1×–60×, max), seat or white-cell view, route planning and ordered speed.
  - Typed bindings generated with tauri-specta.
- **CI:** fmt, clippy and tests on Windows, macOS and Linux; a cross-OS determinism hash comparison; front-end typecheck, lint and build.
