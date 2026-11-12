# ADR-0007: Phase 4 region of interest, imagery, historical engagements and data packs

- **Status:** Accepted
- **Date:** 2026-11-12
- **Decided by:** the client, on the recommendations in the [Phase 4 plan](../plan/phase-4.md) §2
- **Sets:** master prompt §0 (*Primary region of interest*, until now "to be confirmed"); Phase 4 plan decisions 1–4. Decision 5 (measures of effectiveness) is still open.

## Context

The master prompt leaves the primary region of interest open. It drives the Phase 4 regional database (§9) and where terrain and bathymetry are bundled at full resolution. Phase 4 also needs:
- an imagery source that can ship in an offline installer;
- historical engagements for Gate 4;
- a size policy for the data packs.

## Decisions

### 1. Region: the whole Philippine Area of Responsibility

- **Area:** the Philippine Area of Responsibility (PAR), as published by the Philippine weather service (PAGASA). Its corners are 25°N 120°E, 25°N 135°E, 5°N 135°E, 5°N 115°E, 15°N 115°E and 21°N 120°E.
- **Full-resolution box:** 3°N–27°N, 111°E–137°E. That is the PAR with a 2° margin, extended west so the whole Kalayaan Island Group and the approaches to the West Philippine Sea are covered, since part of them lies west of the PAR's 115°E edge.
- **Regional database (Phase 4 slice 13):** the navies and coast guards that operate in this area, from open sources only, each record with a citation, a confidence grade and the `UNCLASSIFIED` marking. The client agrees the order of priority at the start of slice 13. The clean-room and data-security rules (master prompt §2, §3) apply unchanged: nothing classified enters this repository.
- **Shipped scenarios:** Phase 4 demonstrators and the Monte Carlo examples move to this region. The Bass Strait scenarios stay as regression tests, with their golden replays.

### 2. Imagery: client-licensed, offline, imported

- The client procures satellite imagery of the PAR under a licence that allows offline use on exercise and planning machines.
- It is delivered as an offline tile pack (MBTiles or PMTiles). The app imports it (slice 6), shows the licence's attribution, and never fetches imagery at run time.
- **No imagery is committed to this repository** or bundled in the open installers. A tile pack is installed on each machine like a regional data pack.
- Resolution is whatever the licence allows. Planning at TTX scale needs about 10 m or coarser.
- Until the pack arrives, the map uses the vector coastline and, from slice 2, depth shading.

### 3. Historical engagements for Gate 4: one surface-missile, one submarine/ASW

These are **proposals**, chosen because the open record of each is rich enough to compare against. The client's subject-matter experts confirm them, build the scenarios, and document each record's uncertainty (Phase 4 slice 14).

| Role | Proposed engagement | What it tests |
|---|---|---|
| Surface-missile | **Operation Praying Mantis, 18 April 1988:** surface action groups against surface units, with anti-ship and surface-to-air missiles, and decoys | Surface-group detection and engagement, anti-ship missile Ph, point defence and soft kill, damage |
| Submarine / ASW | **The sinking of INS *Khukri* by PNS *Hangor*, 9 December 1971:** a diesel submarine's torpedo attack on an ASW frigate group | Passive and active sonar detection, torpedo engagement. It exercises the Phase 4 acoustics (sound-velocity profile and layers) |
| Alternate (mine) | **USS *Samuel B. Roberts*, 14 April 1988:** a mine strike and the damage-control fight | The mine model, damage, fire and flooding |

- **Why not an engagement in the Philippine area?** No missile or submarine engagement there has a detailed open record.
- **What validation tests:** the models, which do not depend on the region. Regional relevance comes from the regional database and the planning scenarios.
- **The bar:** each replay is compared against a **range**, taken from the documented uncertainty, over N Monte Carlo replications (slice 1). It is not compared against a single historical figure.

### 4. Data packs: a coarse global layer in the installer, plus regional packs

- **In the installer:** a coarse global depth and elevation layer, so the whole world has bathymetry and terrain at planning scale.
  - Sources: GEBCO for depth and SRTM for land, resampled to about 2 arc-minutes.
  - Slice 2 measures the exact grid and its compressed size. Tens of megabytes is the target.
- **Separate regional pack for the PAR box** (3°N–27°N, 111°E–137°E):
  - GEBCO at full resolution (15 arc-seconds) and SRTM at 3 arc-seconds;
  - installed on its own, verified by SHA-256, as the scenarios are today;
  - expected to be a few hundred megabytes; slice 2 measures it.
- **Determinism:** the grids are integers, sampled with `libm` interpolation. A pack's identity is part of the state hash, so a run says which data it used, and the determinism matrix runs with the packs loaded.
- **Licences:** GEBCO's and SRTM's terms (attribution, redistribution) are checked and recorded in the data dictionary before the first pack is built.

## Consequences

- Phase 4 slices 1, 2, 3 and 13 have a region to work on. Slice 2 builds the PAR pack first.
- The client has two actions:
  - procure the imagery licence;
  - name the subject-matter experts who confirm and build the Gate 4 engagements.
- The master prompt's §0 parameter is answered here. The master prompt itself is kept as the original brief.
- Decision 5, the measures of effectiveness, is still needed before slice 1 can report anything other than the defaults.
