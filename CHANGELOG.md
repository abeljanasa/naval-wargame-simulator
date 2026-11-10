# Changelog

## [Unreleased]

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
