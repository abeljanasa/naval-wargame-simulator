# Changelog

## [Unreleased]

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
