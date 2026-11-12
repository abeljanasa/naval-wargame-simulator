# Phase 4 plan: Analysis (Gate 4: historical replays)

> **Started 2026-11-12, ahead of Gate 3, by the client's decision.** Master prompt §13 has Phase 4 wait for the gate. The client chose to start while the TTX pilot is arranged, because the first slices do not touch what the pilot tests.
> - Phase 4 work is on `phase-4/analysis`, branched from `phase-3/ttx-release`. PR #3 stays open for the Gate 3 panel.
> - Fixes from the pilot land on Phase 3 and are merged forward.
> - Until the client names its measures of effectiveness (decision 5), slice 1 reports a default set (§4).

- **Months:** 12–24 (master prompt §13).
- **Scope (§13):**
  - Monte Carlo batch;
  - acoustics (SVP, layers, CZ);
  - weather;
  - air operations and sorties;
  - automatic sense-zone tasking;
  - regional database;
  - model validation.
- **Added by the client at Gate 3 (2026-11-12):** the in-phase gaps that need outside data or a Phase 4 subsystem ([Gate 3 pack §7](../gate3/README.md)):
  - satellite imagery;
  - terrain elevation and bathymetry (GEBCO/SRTM), and what depends on them: terrain masking, depths and shipping lanes for choke points, shallow-water routing;
  - airbases and air operations: aircraft fuel, tanking, loadout choice, runways.
- **Branch:** `phase-4/analysis`, from `main` after the Gate 3 merge.
- **Region:** the Philippine Area of Responsibility ([ADR-0007](../adr/0007-phase-4-region-and-data.md)).
- **Gate 4 (§12, §13):** historical replays. SME-built scenarios of historical engagements are run and compared against the recorded outcomes, and the differences are documented.

## 1. Definition of done (Gate 4)

- Every slice below meets the feature definition of done (§16): a behaviour spec and design-log entry; unit tests, plus a duel test if it touches detection or combat; determinism and golden replays green; model and user docs updated; usable in the UI or CLI.
- The batch runner produces Monte Carlo statistics (mean, SD, confidence intervals, distributions) for scenario-defined measures of effectiveness, and scales near-linearly with cores (§11, §12).
- At least two historical engagements, chosen with the client's SMEs, run as scenarios. Each has:
  - its outcome distribution over N replications, compared with the record;
  - every difference explained (model limitation, data gap or historical uncertainty).
- The regional database covers the order of battle those replays and the client's planning scenarios need, with citations and confidence.
- The terrain, bathymetry and imagery data ships in the installers, or as separate offline packs, and still runs air-gapped.
- All CI checks green, including the determinism matrix with the data packs loaded.

## 2. Client decisions

Decisions 1–4 were made on 2026-11-12 and are recorded in [ADR-0007](../adr/0007-phase-4-region-and-data.md). Decision 5 is still open.

| # | Decision | Status |
|---|---|---|
| 1 | **Primary region of interest** | **Decided:** the whole Philippine Area of Responsibility (PAR). Full-resolution data covers 3°N–27°N, 111°E–137°E: the PAR with a margin, including the Kalayaan Island Group. The regional database covers the navies and coast guards operating there. Phase 4 demonstrators move to this region |
| 2 | **Imagery source and licence** | **Decided:** the client procures licensed imagery of the PAR for offline use and delivers it as a tile pack, which the app imports (slice 6). No imagery is committed to the repository |
| 3 | **Historical engagements for Gate 4** | **Decided:** one surface-missile and one submarine/ASW engagement. Proposed: Operation Praying Mantis (18 April 1988) and *Hangor* vs *Khukri* (9 December 1971), with the *Samuel B. Roberts* mine strike (14 April 1988) as the alternate. **The client's subject-matter experts confirm them** and build the scenarios |
| 4 | **Data pack size against resolution** | **Decided:** a coarse global layer in the installer, plus a full-resolution PAR pack installed separately and verified by SHA-256. **Measured in slice 2:** the global layer is 64.5 MB at 2′ and 17.5 MB at 4′, so the installer layer is **4′**; the 15″ PAR pack is 30.5 MB |
| 5 | **Measures of effectiveness** | **Open.** The client names the first set from its own TTX questions. Slice 1 needs them |

**Data licences:**
- GEBCO and SRTM are published for free use. Their exact terms (attribution, redistribution) are checked and recorded in the data dictionary before any file is bundled, as for every source (§2, §3).
- Imagery follows the client's licence (decision 2).

## 3. Order

The order front-loads what unblocks the rest:
- **Monte Carlo first:** model validation and Gate 4 need statistics.
- **Terrain and bathymetry next:** four deferred items and bottom-bounce acoustics depend on them.
- **Air operations as one block:** it is the largest subsystem.
- **Regional database alongside, from the start:** the database is the project's long pole (§9).

```
 1 Monte Carlo ─────────────────────────────────────────────┐
 2 Terrain/bathymetry packs ─┬─ 3 Shallow water & depths    │
                             ├─ 4 Terrain masking           │
                             ├─ 5 Choke-point lanes         │
                             └─ 8 Acoustics (bottom) ─┐     │
 6 Imagery (on client licence)                        │     │
 7 Weather zones ─────────────────────────────────────┤     │
 9 Airbases ── 10 Sorties & loadouts ── 11 Air fuel   │     │
12 Auto sense-zone tasking (after 4, 8)               │     │
13 Regional database (throughout) ────────────────────┴─ 14 Historical replays (Gate 4)
```

## 4. Slices

Each slice: behaviour spec, design-log entry, tests, docs, CHANGELOG, green CI (§15, §16).

| # | Slice | Delivers | Depends on |
|---|---|---|---|
| 1 ✅ | **Monte Carlo batch** (`nws-batch mc`, 2026-11-13: parallel runs, default MOEs, CSV, JSON and Parquet. 12 one-hour runs: 2.8 s on 1 thread, 0.9 s on 12) | `nws-batch mc`: N replications across seeds, in parallel (one simulation per thread, so each run stays deterministic). Scenario-defined MOEs, such as units lost by side, time to first detection, weapons to kill, tonnage delivered or a choke-point transit outcome. Results in Arrow/Parquet and CSV, plus a summary: mean, SD, 95 % confidence intervals, histograms. Scaling benchmark with a CI budget | — |
| 2 ✅ | **Terrain and bathymetry packs** (2026-11-13: GEBCO 2020 packs, PAR 15″ and global 4′, with SHA-256-checked install, depth shading, contours and relief. [DL-0038](../design-log/DL-0038-terrain-packs.md). Open: bundling the 4′ layer in the installer needs a host for the file) | Offline elevation and depth grids from GEBCO (sea) and SRTM (land), tiled and compressed: a coarse global layer in the installer, plus the PAR pack at full resolution, built first. Sampled with integer grids and `libm` bilinear interpolation, so results match on every OS. Pack identity in the state hash. Map layers for depth shading, contours and hillshade. Air-gapped install and SHA-256 check | — |
| 3 ✅ | **Shallow water and depths** (2026-11-13: routes planned round water shallower than draft + 1 m, shoal guard off the charted route, chart depths for ports and anchorages with the planner's override, depth along the route in the unit panel. [DL-0039](../design-log/DL-0039-shallow-water.md). Choke-point profiles move to slice 5) | A navigable-water routing graph that honours each unit's draft, plus depth along the route. Ports, anchorages and choke points take their depth from the bathymetry, and the planner can still override it. Grounding risk off the graph | 2 |
| 4 | **Terrain masking** | Radar, ESM, EO/IR and comms line of sight blocked by terrain along the path, with a sampled profile and 4/3 earth. Land clutter for low air targets over land (Phase 2 backlog). Coverage on the map (slice 14 of Phase 3) uses it | 2 |
| 5 | **Choke points inherit depth and lanes** | A choke point drawn or selected on the map takes its depth profile and shipping lanes. Planner-drawn lanes always work; lanes derived from open traffic data only where a licensed source exists. Traffic density raises transit risk in the AAR | 2, 3 |
| 6 | **Imagery layer** | Import of a licensed offline tile pack (MBTiles/PMTiles) as a base layer, with no network access. Its attribution is shown. Nothing is fetched at run time | The client's licensed tile pack (decision 2) |
| 7 | **Weather zones** | Polygon weather zones with sea state, wind, visibility, rain and cloud, changed by events and the white cell. Per-zone effects on radar (rain loss, clutter), EO/IR and visual, weapon Ph, sheltering and speed penalties. Replaces the single scenario-wide sea state, and older scenarios load as one global zone | — |
| 8 | **Acoustics** | Sound-velocity profiles by region and season, entered or imported. Mixed-layer depth with cross-layer loss, convergence zones, bottom bounce (using the bathymetry) and depth dependence. Reverberation for active sonar. Acoustic figures as database fields (a migration). Sonobuoy laying and towed-array streaming states. Duel tests against hand-calculated TL | 2 for bottom bounce |
| 9 | **Airbases and runways** | Airbase and carrier facilities with runways or decks, aprons and parking capacity, and aircraft based at a facility. Readiness states: ready, alert 15, alert 60, maintenance | — |
| 10 | **Sorties and loadouts** | Loadout choice per mission from the database. Turnaround time by loadout. Sortie generation limited by deck and runway capacity. Strike, patrol (CAP, ASW) and AEW missions launched as sorties | 9 |
| 11 | **Aircraft fuel and tanking** | Aircraft fuel by phase of flight, bingo and return to base, tanker missions, and air-to-air refuelling queues. Aviation fuel at depots and on ships (the Phase 3 fuel model gains a fuel type) | 9, 10 |
| 12 | **Automatic sense-zone tasking** | The engine places and tasks the side's sensors to keep each sense zone's coverage above a planner-set level, as §7.1 asks for in the analysis phase. It uses the Phase 3 coverage model, and every tasking decision is logged | 4, 8 |
| 13 | **Regional database** | Open-source platforms, sensors and weapons of the navies and coast guards operating in the PAR (decision 1), each with a citation, confidence and an `UNCLASSIFIED` marking. Reference data (nations, services, propulsion) editing in the app, if the data team needs it; platforms, sensors and weapons are editable already (DL-0037). ECM equipment as database records | — |
| 14 | **Model validation and Gate 4** | Historical replay scenarios built with SMEs (decision 3), run through slice 1. Comparison against the record, with a validation report per engagement. Duel tests extended to the new models. Gate 4 review pack | 1, 13, and the models the engagements use |

## 5. Carried over

Open items from the backlog that fit this phase and need no separate slice:
- function icons inside the APP-6 frames (needs a symbology behaviour spec);
- seeker re-attack, and salvo deconfliction across ships not in a group;
- track velocity estimation and association errors;
- a full end-to-end test through the packaged app (WebDriver with tauri-driver);
- rerunning the benchmarks on the reference laptop.

Still out of phase: networked multiplayer, comms and cyber effects (including data-link picture sharing through comms links), advanced AI and a 3D viewer (Phase 5).

## 6. Risks

| Risk | Mitigation |
|---|---|
| Data packs break determinism across OSes | Integer grids, `libm` interpolation, and the pack's hash in the state hash. The determinism matrix runs with the packs loaded |
| Data pack size makes installers unwieldy | Coarse global layer in the installer; regional packs installed separately (decision 4) |
| Historical records are incomplete or disputed | SMEs record the uncertainty in each scenario; the comparison is against a range, not a single number |
| Acoustics needs data the open build cannot hold | Generic SVPs from open climatology; classified profiles only in the accredited database, never in this repository (§3) |
| Air operations grow without limit | Slices 9–11 cover sortie generation, turnaround, capacity and tanking only. Air combat tactics are Phase 5 (advanced AI) |
| Regional database effort underestimated | Start in month 12, alongside everything else; validation tooling first (§9, §14) |
| Imagery licence never arrives | Slice 6 is independent; the map stays usable with the vector base and the depth shading |
