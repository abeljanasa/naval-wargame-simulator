# Backlog

Items deferred out of the current change, or requested out of phase (master prompt §14). Each item names the phase it belongs to.

## Phase 1, still to do (Gate 1: "units move and detect")

- [x] `sim-db`: SQLite schema, forward-only migrations, `cargo run -p sim-db -- validate`, and 50 generic platform types with citations, confidence and classification fields (DL-0005).
- [x] Scenarios reference database platforms (`"platform": "GEN-FFG"`) (DL-0007).
- [x] In-app database browser: search, record detail with source citation, confidence and marking, and validation feedback (DL-0009).
- [ ] In-app database *editing* (record diff, citation entry, live validation while typing). It writes the JSON source (ADR-0004). Phase 2; until then records are edited in `data/db/open/` and checked with `sim-db validate`.
- [ ] Planning-layer facility attributes for airbases and ports (runways, berths, draft limit, repair/rearm), needed by §7.1 in Phase 3.
- [x] Release scan (`sim-db scan`), run in the release workflow.
- [x] Installers, signed when secrets are configured, plus SBOMs and a `cargo deny` licence audit (docs/build/releasing.md).
- [x] Air-gapped vendored build, with the engine verified offline (docs/build/air-gapped.md).
- [x] Throughput budgets met (`nws-batch bench`; docs/build/performance.md), with a CI gate on 1,000 units at ≥ 60×.
- [ ] Code-signing certificates: the client provides them as Actions secrets. Until then installers are unsigned.
- [x] Criterion benchmarks, with a PR check that fails on a > 15 % regression against the base commit (docs/build/performance.md).
- [x] Map at 60 fps with 10,000 symbols: `npm run bench:map`. 59.2 fps with 30 Hz updates on Intel UHD, with labels decluttered above 1,500 units.
- [ ] Rerun the map and engine benchmarks on the actual reference laptop at the Gate 1 review.
- [x] Full Windows desktop bundle built from vendored sources with every network path blocked (docs/build/air-gapped.md).
- [ ] Repeat the offline bundle on a physically disconnected machine, and for macOS and Linux.
- [x] npm licence audit of production dependencies (`npm run licenses`, in CI). 52 packages, 0 rejected.
- [x] A basic detection model for Gate 1: the interim radar model (DL-0006).
- [x] Desktop app shows contacts: covered by the end-to-end smoke test (Contacts panel) and engine tests.
- [x] Turn-rate / turning-circle limits on movement (geodesy-and-movement model doc §Turning).
- [ ] Turn performance as platform-database fields (generic class values are derived in scenario resolution today).
- [x] Land blocking with route planning round the coastline (DL-0008, land-avoidance model doc).
- [ ] Shallow water and draft limits (needs bundled GEBCO bathymetry).
- [x] Snapshot diffs on the UI stream: full picture first and after seat/scenario changes, then changed units + removed ids.
- [x] Scenario loading from file in the desktop app (webview reads the file; engine validates; 5 MB limit; no filesystem permission).
- [x] Front-end unit tests (Vitest, 8 tests) and an end-to-end smoke test of the real UI against a mocked backend (Playwright, 11 checks); both in CI.
- [ ] Full end-to-end test through the packaged Tauri app (WebDriver + tauri-driver).
- [x] APP-6-style frames by domain (sea surface full, air upper half, subsurface lower half, land) and affiliation.
- [ ] Function icons inside the frames (frigate, submarine, fighter…); needs a symbology behaviour spec.
- [x] Light-theme toggle (header button; map base layers recoloured; remembered per viewer).

## Phase 2 (replacing Gate 1 interim detection)

- [ ] Move detection into a `sim-sensors` crate (§6.1) and replace the interim radar model (DL-0006).
- [ ] Add a fluctuation model (Swerling), clutter, jamming, weather, terrain masking, aspect- and speed-dependent RCS, and each radar's own scan period.
- [ ] Add ESM, sonar (passive and active), visual, EO/IR and MAD.
- [ ] Contacts: position error ellipses, a classification ladder, track correlation and fusion, data-link sharing.

## Later phases

- [ ] Karney geodesics to replace Vincenty's spherical fallback near antipodes (only if a scenario needs it).
- [ ] Higher-resolution offline coastline and tile packs (PMTiles), GEBCO and SRTM sampling.
