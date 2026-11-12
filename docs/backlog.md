# Backlog

Items deferred out of the current change, or requested out of phase (master prompt §14). Each item names the phase it belongs to.

Phase 2 work is planned in [plan/phase-2.md](plan/phase-2.md); Phase 3 in [plan/phase-3.md](plan/phase-3.md); Phase 4 (draft) in [plan/phase-4.md](plan/phase-4.md).

## Phase 3, deferred from slices

P1 gaps found at the Gate 3 review ([docs/gate3/README.md](gate3/README.md)), closed in slices 10–17 (2026-11-12):

- [x] Comms links and their disruption (jamming, relays), including jamming a command post (slice 10, DL-0029).
- [x] Support, mining, mine-clearing and cargo missions; barrier-patrol and denial missions on a choke point (slice 12, DL-0031).
- [x] Anchorages with polygon areas, depth and holding limits; units holding there between missions (slice 13, DL-0032).
- [x] Sense-zone layers by sensor type, and coverage and gaps drawn on the map (slice 14, DL-0033).
- [x] Coordinated air defence and one shared mission object for a surface action group (slice 16, DL-0035).
- [x] Day/night shading on the map (slice 17, DL-0036).
- [x] Repair restores sensors and speed lost to earlier criticals: full repair at a facility restores the loadout (DL-0028).
- [x] Fuel and endurance for ships and submarines, so refuelling points refuel (slice 11, DL-0030).
- [x] Desktop white-cell control of the planning layer during an exercise (slice 17).
- [x] Polygon zones and facility areas drawn in the scenario editor (slices 13, 17).
- [x] Forms for event triggers, conditions and actions in the scenario editor (slice 17).
- [x] Weapons in flight as detectable, engageable targets for every sensor (slice 15, DL-0034).

Gate 3 conditions still open:

- [ ] Code-signing certificates from the client (Windows PFX; Apple Developer ID and notarisation).
- [ ] Air-gapped run of the signed release build on all three OSes.
- [ ] The navy TTX pilot ([plan](gate3/ttx-pilot-plan.md)).

## Phase 4, deferred by the client (2026-11-12)

The client chose to close the in-phase P1 gaps and move these to Phase 4, because they need outside data or a Phase 4 subsystem. The [Phase 4 plan](plan/phase-4.md) schedules them. The region (the Philippine Area of Responsibility), imagery and data-pack decisions are in [ADR-0007](adr/0007-phase-4-region-and-data.md):

- [ ] Satellite imagery layers (needs a licensed imagery source).
- [ ] Terrain elevation and bathymetry packs (GEBCO/SRTM), then:
  - radar terrain masking;
  - ~~depths for ports and anchorages from bathymetry~~ (Phase 4 slice 3, DL-0039);
  - choke points that inherit depth and shipping lanes;
  - ~~shallow-water routing~~ (Phase 4 slice 3, DL-0039).
- [ ] Airbases and air operations: aircraft fuel and tanking, aircraft loadout choice, runways at facilities.
- [ ] Automatic sense-zone sensor placement and tasking (§7.1, analysis phase).

## Phase 2, deferred from slices

- [ ] Radar terrain masking (needs bundled elevation data, with GEBCO/SRTM in Phase 4).
- [ ] Low air targets over land should see land clutter (needs a land-mask lookup in detection).
- [ ] ECM equipment (jammers) in the platform database, with soft kill in slice 9; until then jammers are given per unit in scenarios.
- [ ] Track velocity estimation and association errors (tracks model limitations).
- [ ] Acoustic figures (radiated noise, sonar frequency, DI, DT, source level) as platform-database fields; generic defaults in code until then.
- [ ] Sonobuoy laying and towed-array streaming states (missions, slice 10); active-sonar reverberation.
- [x] Weapons in flight as detectable, engageable targets (Phase 3 slice 15); salvo deconfliction within an action group (slice 16).
- [ ] Seeker re-attack, and salvo deconfliction across ships that are not in a group.
- [ ] Aircraft loadout choice (air operations, Phase 4); damage location and compartments.

## Phase 1, still to do (Gate 1: "units move and detect")

- [x] `sim-db`: SQLite schema, forward-only migrations, `cargo run -p sim-db -- validate`, and 50 generic platform types with citations, confidence and classification fields (DL-0005).
- [x] Scenarios reference database platforms (`"platform": "GEN-FFG"`) (DL-0007).
- [x] In-app database browser: search, record detail with source citation, confidence and marking, and validation feedback (DL-0009).
- [x] In-app database *editing* for platform records: record diff, citation entry, live validation while typing. It writes the JSON source (ADR-0004, DL-0037).
- [x] Editing sensors and weapons in the app, in the same record editor (`sensors.json`, `weapons.json`).
- [ ] Planning-layer facility attributes still to do: runways (air operations, Phase 4). Berths, repair, rearm, fuel and draft limits (planner-entered depth, DL-0032) are done.
- [x] Release scan (`sim-db scan`), run in the release workflow.
- [x] Installers, signed when secrets are configured, plus SBOMs and a `cargo deny` licence audit (docs/build/releasing.md).
- [x] Air-gapped vendored build, with the engine verified offline (docs/build/air-gapped.md).
- [x] Throughput budgets met (`nws-batch bench`; docs/build/performance.md), with a CI gate on 1,000 units at ≥ 60×.
- [ ] Code-signing certificates: the client provides them as Actions secrets. Until then installers are unsigned.
- [x] Criterion benchmarks, with a PR check that fails on a > 15 % regression against the base commit (docs/build/performance.md).
- [x] Map at 60 fps with 10,000 symbols: `npm run bench:map`. 59.2 fps with 30 Hz updates on Intel UHD, with labels decluttered above 1,500 units.
- [ ] Rerun the map and engine benchmarks on the actual reference laptop at the Gate 1 review.
- [x] Full Windows desktop bundle built from vendored sources with every network path blocked (docs/build/air-gapped.md).
- [x] Offline install on a physically disconnected Windows machine, with the signed release (2026-11-12).
- [ ] The same on macOS (Apple Silicon). Skipped for Gate 3 by the client's decision (2026-11-12); run it if the panel asks. Linux is no longer a target (ADR-0006).
- [x] npm licence audit of production dependencies (`npm run licenses`, in CI). 52 packages, 0 rejected.
- [x] A basic detection model for Gate 1: the interim radar model (DL-0006).
- [x] Desktop app shows contacts: covered by the end-to-end smoke test (Contacts panel) and engine tests.
- [x] Turn-rate / turning-circle limits on movement (geodesy-and-movement model doc §Turning).
- [ ] Turn performance as platform-database fields (generic class values are derived in scenario resolution today).
- [x] Land blocking with route planning round the coastline (DL-0008, land-avoidance model doc).
- [x] Shallow water and draft limits: routing by draft, shoal guard, chart depths (Phase 4 slice 3, [DL-0039](design-log/DL-0039-shallow-water.md)).
- [x] Snapshot diffs on the UI stream: full picture first and after seat/scenario changes, then changed units + removed ids.
- [x] Scenario loading from file in the desktop app (webview reads the file; engine validates; 5 MB limit; no filesystem permission).
- [x] Front-end unit tests (Vitest, 8 tests) and an end-to-end smoke test of the real UI against a mocked backend (Playwright, 11 checks); both in CI.
- [ ] Full end-to-end test through the packaged Tauri app (WebDriver + tauri-driver).
- [x] APP-6-style frames by domain (sea surface full, air upper half, subsurface lower half, land) and affiliation.
- [x] Function codes inside the frames (DDG, FFG, SSK, F, H…), by class or category as far as the side has classified it ([symbology spec](behaviour-specs/symbology.md)). Echelon and status amplifiers, and graphic air and land icons, are still to do if asked for.
- [x] Light-theme toggle (header button; map base layers recoloured; remembered per viewer).

## Phase 2 (replacing Gate 1 interim detection)

- [ ] Move detection into a `sim-sensors` crate (§6.1) and replace the interim radar model (DL-0006).
- [ ] Add a fluctuation model (Swerling), clutter, jamming, weather, terrain masking, aspect- and speed-dependent RCS, and each radar's own scan period.
- [ ] Add ESM, sonar (passive and active), visual, EO/IR and MAD.
- [ ] Contacts: position error ellipses, a classification ladder, track correlation and fusion, data-link sharing.

## Later phases

- [ ] Karney geodesics to replace Vincenty's spherical fallback near antipodes (only if a scenario needs it).
- [ ] Higher-resolution offline coastline and tile packs (PMTiles), GEBCO and SRTM sampling.
