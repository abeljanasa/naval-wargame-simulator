# ADR-0001: Technology stack

- **Status:** Accepted
- **Date:** 2026-11-10

## Context

The navy needs an engine it owns outright. It must run on Windows and macOS (Linux was dropped on 2026-11-12, ADR-0006), work on air-gapped networks, give bit-identical results for analysis, and drive a map with 10,000+ moving symbols. One validated model must serve three clients: the desktop app, a headless batch runner, and (later) a multiplayer host.

## Decision

| Layer | Choice |
|---|---|
| Simulation core | Rust (stable, pinned in `rust-toolchain.toml`) + `bevy_ecs` (ECS only, not the Bevy engine) |
| Geodesy & maths | Own WGS-84 routines in `sim-geo` using the pure-Rust `libm` crate (see ADR-0003) |
| Desktop shell | Tauri 2 |
| UI | TypeScript + Svelte 5 (ADR-0002) |
| Map | MapLibre GL JS + deck.gl |
| Offline map data | PMTiles/MBTiles, GEBCO bathymetry, SRTM terrain. Phase 1 bundles Natural Earth 110 m land (public domain) in the app |
| Platform DB | SQLite via `rusqlite` |
| Scripting | Lua 5.4 via `mlua`, sandboxed |
| Networking (Phase 5) | QUIC (`quinn`) + `serde`/MessagePack |
| Analysis output | Apache Arrow/Parquet + CSV |
| Serialisation | `serde`: JSON for scenarios, MessagePack for streams and saves; every format carries a version |

The simulation core is a headless library with no UI code. The desktop app, the batch runner and the multiplayer host are all clients of it.

## Rejected alternatives

| Option | Why rejected |
|---|---|
| C++ / Qt 6 + osgEarth | Commercial Qt licence cost or LGPL obligations; osgEarth is heavy; memory-safety burden; slower onboarding |
| C# / .NET 8 + Avalonia | GC pauses hurt batch throughput; weaker GPU map performance; cross-OS float determinism is harder to control |
| Electron | Installer size and memory footprint far above Tauri; no benefit over Tauri for our needs |
| Godot / Unity | Strong 3D, weak for data-dense UIs (tables, editors, logs). May later serve as an **optional** 3D viewer that reads the engine's state stream |

Do not reopen these without new evidence.

## Consequences

- Rust hiring is harder; we offset this with readable code, docs and an onboarding guide.
- The UI and core talk only through serialisable commands and snapshots, which also gives us the multiplayer protocol for free.
- Tauri uses the OS webview (WebView2 / WKWebView / WebKitGTK), so the UI must be tested on all three.
