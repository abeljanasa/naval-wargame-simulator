# Backlog

Items deferred out of the current change, or requested out of phase (master prompt §14). Each item names the phase it belongs to.

## Phase 1, still to do (Gate 1: "units move and detect")

- [x] `sim-db`: SQLite schema, forward-only migrations, `cargo run -p sim-db -- validate`, and 50 generic platform types with citations, confidence and classification fields (DL-0005).
- [ ] Scenarios reference database platforms (`"platform": "GEN-FFG"`) instead of inline speeds and acceleration. Needs the desktop app and batch runner to build or load the database.
- [ ] In-app database editor (search, record diff, citation entry, live validation). It edits the JSON source (ADR-0004).
- [ ] Planning-layer facility attributes for airbases and ports (runways, berths, draft limit, repair/rearm), needed by §7.1 in Phase 3.
- [ ] Release scan in CI (open-data allowlist; fail on any record with `classification != "UNCLASSIFIED"`).
- [ ] Signed installers (MSI/NSIS, DMG, AppImage/deb), SBOM generation, a `cargo deny` licence audit, and an air-gapped vendored build.
- [ ] Criterion benchmarks with CI regression gates: 1 000 entities at ≥ 60×, 10 000 at ≥ 1×.
- [ ] A basic detection model for Gate 1 ("units move **and detect**"). Full sensors are Phase 2. **Needs a decision** on what minimal detection Gate 1 requires.
- [ ] Turn-rate / turning-circle limits on movement.
- [ ] Land and shallow-water blocking with pathfinding on a navigable-water graph honouring draft limits (needs coastline and bathymetry sampling in `sim-geo`).
- [ ] Snapshot diffs instead of full snapshots on the UI stream (the protocol is versioned for this).
- [ ] Scenario loading from file in the desktop app (Phase 1 embeds the demo scenario).
- [ ] Front-end unit tests (Vitest) and a Tauri end-to-end smoke test (WebDriver).
- [ ] APP-6-style symbol icons by domain and type, beyond affiliation frames.
- [ ] Light-theme toggle in the UI (the CSS tokens already exist).

## Later phases

- [ ] Karney geodesics to replace Vincenty's spherical fallback near antipodes (only if a scenario needs it).
- [ ] Higher-resolution offline coastline and tile packs (PMTiles), GEBCO and SRTM sampling.
