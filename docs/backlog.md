# Backlog

Items deferred out of the current change, or requested out of phase (master prompt §14). Each item names the phase it belongs to.

## Phase 1, still to do (Gate 1: "units move and detect")

- [x] `sim-db`: SQLite schema, forward-only migrations, `cargo run -p sim-db -- validate`, and 50 generic platform types with citations, confidence and classification fields (DL-0005).
- [x] Scenarios reference database platforms (`"platform": "GEN-FFG"`) (DL-0007).
- [ ] In-app database editor (search, record diff, citation entry, live validation). It edits the JSON source (ADR-0004).
- [ ] Planning-layer facility attributes for airbases and ports (runways, berths, draft limit, repair/rearm), needed by §7.1 in Phase 3.
- [x] Release scan (`sim-db scan`), run in the release workflow.
- [x] Installers, signed when secrets are configured, plus SBOMs and a `cargo deny` licence audit (docs/build/releasing.md).
- [x] Air-gapped vendored build, with the engine verified offline (docs/build/air-gapped.md).
- [x] Throughput budgets met (`nws-batch bench`; docs/build/performance.md), with a CI gate on 1,000 units at ≥ 60×.
- [ ] Code-signing certificates: the client provides them as Actions secrets. Until then installers are unsigned.
- [ ] Criterion micro-benchmarks and an automatic "> 15 % regression against the stored baseline" CI check.
- [ ] Map at 60 fps with 10,000 symbols: needs an automated UI performance test.
- [ ] Build the full desktop bundle on a disconnected machine, with pre-staged WiX/NSIS tools.
- [ ] npm dependency licence audit (the Rust side is covered by cargo deny).
- [x] A basic detection model for Gate 1: the interim radar model (DL-0006).
- [ ] Desktop app: visual check of contacts during a live run. Covered by engine tests so far; there is no automated UI test yet.
- [ ] Turn-rate / turning-circle limits on movement.
- [ ] Land and shallow-water blocking with pathfinding on a navigable-water graph honouring draft limits (needs coastline and bathymetry sampling in `sim-geo`).
- [ ] Snapshot diffs instead of full snapshots on the UI stream (the protocol is versioned for this).
- [ ] Scenario loading from file in the desktop app (Phase 1 embeds the demo scenario).
- [ ] Front-end unit tests (Vitest) and a Tauri end-to-end smoke test (WebDriver).
- [ ] APP-6-style symbol icons by domain and type, beyond affiliation frames.
- [ ] Light-theme toggle in the UI (the CSS tokens already exist).

## Phase 2 (replacing Gate 1 interim detection)

- [ ] Move detection into a `sim-sensors` crate (§6.1) and replace the interim radar model (DL-0006).
- [ ] Add a fluctuation model (Swerling), clutter, jamming, weather, terrain masking, aspect- and speed-dependent RCS, and each radar's own scan period.
- [ ] Add ESM, sonar (passive and active), visual, EO/IR and MAD.
- [ ] Contacts: position error ellipses, a classification ladder, track correlation and fusion, data-link sharing.

## Later phases

- [ ] Karney geodesics to replace Vincenty's spherical fallback near antipodes (only if a scenario needs it).
- [ ] Higher-resolution offline coastline and tile packs (PMTiles), GEBCO and SRTM sampling.
