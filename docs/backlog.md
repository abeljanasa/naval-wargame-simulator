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
- [x] Criterion benchmarks, with a PR check that fails on a > 15 % regression against the base commit (docs/build/performance.md).
- [x] Map at 60 fps with 10,000 symbols: `npm run bench:map`. 59.2 fps with 30 Hz updates on Intel UHD, with labels decluttered above 1,500 units.
- [ ] Rerun the map and engine benchmarks on the actual reference laptop at the Gate 1 review.
- [x] Full Windows desktop bundle built from vendored sources with every network path blocked (docs/build/air-gapped.md).
- [ ] Repeat the offline bundle on a physically disconnected machine, and for macOS and Linux.
- [x] npm licence audit of production dependencies (`npm run licenses`, in CI). 52 packages, 0 rejected.
- [x] A basic detection model for Gate 1: the interim radar model (DL-0006).
- [ ] Desktop app: visual check of contacts during a live run. Covered by engine tests so far; there is no automated UI test yet.
- [x] Turn-rate / turning-circle limits on movement (geodesy-and-movement model doc §Turning).
- [ ] Turn performance as platform-database fields (generic class values are derived in scenario resolution today).
- [x] Land blocking with route planning round the coastline (DL-0008, land-avoidance model doc).
- [ ] Shallow water and draft limits (needs bundled GEBCO bathymetry).
- [ ] Snapshot diffs instead of full snapshots on the UI stream (the protocol is versioned for this).
- [ ] Scenario loading from file in the desktop app (Phase 1 embeds the demo scenario).
- [ ] Front-end unit tests (Vitest) and a Tauri end-to-end smoke test (WebDriver).
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
