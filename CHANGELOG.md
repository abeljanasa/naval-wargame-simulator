# Changelog

## [Unreleased]

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
