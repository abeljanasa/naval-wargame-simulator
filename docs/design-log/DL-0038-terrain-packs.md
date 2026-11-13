# DL-0038: Terrain and bathymetry packs

- **Date:** 2026-11-13
- **Phase:** 4, slice 2
- **Sources:**
  - Master prompt §6 (terrain and bathymetry), §14 (determinism).
  - ADR-0003 (determinism), ADR-0006 (Windows and macOS), ADR-0007 (Philippine AOR, data packs).
  - GEBCO 2020 Grid (free to use and redistribute; credit given), served by NOAA CoastWatch ERDDAP.
  - The netCDF classic format specification (Unidata, public).
  - Textbook bilinear interpolation, marching squares and hill shading.
  - Original design otherwise.

## Decisions

- **One source, GEBCO 2020.** GEBCO's land heights are SRTM15+, so one grid gives sea floor and land with no seam. SRTM15+ is recorded as the land source, for cross-checks only.
- **Integer grid, plain arithmetic.**
  - Positions are whole tenths of an arc-second. GEBCO's 15″ cell centres sit at 7.5″, which is whole in these units.
  - Heights are whole metres.
  - Sampling uses `+ − × ÷`, `floor` and `%` only.
  - A pinned 20,000-sample golden runs on both CI operating systems, so a change in maths or platform is caught.
- **Pack identity is content.** The id is the name plus 16 hex digits of the file's SHA-256, and the stack's id is in the state hash only when packs are set.
  - Every existing golden replay keeps its hash.
  - Two runs that differ only in their packs cannot match.
- **Records refuse the wrong packs.** A record made with terrain replays only with exactly those packs, and the error names them. Today no model reads heights, but slices 3–5 and 8 will, and a replay with other depths would silently diverge.
- **Format:**
  - It is our own: a small JSON header, then zlib over row-delta `i16`.
  - Packs of the PAR size load in about 2 s, so tiling and lazy loading are left until a larger region needs them.
  - `flate2` (pure-Rust backend) and `sha2` were already in the lockfile, so there are no new crates.
- **netCDF reader written here.** About 150 lines read the netCDF-3 classic subset ERDDAP serves. The alternative, the `netcdf` crate, needs the C library and HDF5, which the air-gapped build cannot assume.
- **Sizes (decision 4 measured):**
  - 15″ PAR pack: 30.5 MB.
  - Global layer: 64.5 MB at 2′, 17.5 MB at 4′.
  - The 4′ layer is the installer layer. The plan's "about 2 arc-minutes" is revised to 4′ to keep the installer in the tens of MB.
- **Install checks the SHA-256 before writing.** The user picks the pack and its `.sha256` file together. The host compares the pack against it, then writes it via a `.part` file, so a half-written pack is never loaded. At start-up, a pack that no longer matches its `.sha256` is set aside, and the reason is shown.
- **Raw IPC for bulk data.** A 30 MB pack as a JSON number array would be about 100 MB of text, so the install and the map's height grid use raw-bytes Tauri commands outside the typed bindings. Everything else stays typed.
- **Map layers in the base style.** Depth shading, contours and relief are MapLibre layers below the coastline, and so below the deck.gl picture: terrain never covers a unit or a label.
  - Rows are sampled evenly in Web Mercator, so an image placed on four corners lines up with the coast at any latitude.
  - The colours are original.
- **A new exercise picks the packs up.** The exercise on screen takes newly read packs only while it is still at its starting picture. A running exercise keeps the terrain it started with, so its record stays replayable.

## The world layer in the installer (decided 2026-11-13)

- **Built from the online source on every release**, at the client's request: "host the 4′ world layer from its online source so we can have updated map".
  - The release workflow downloads the GEBCO grid from NOAA ERDDAP and builds the pack.
  - It bundles the pack with a Tauri config overlay, and also ships it beside the installers with its sum and `SOURCE.txt` (the query, the download's SHA-256 and the date).
- **Not a pinned SHA-256:** a pinned sum would stop the map from updating. Instead the dataset id and credit are inputs of the workflow, and every build records what it downloaded.
- **No date in the pack**, so unchanged data gives an unchanged pack id, and records stay replayable across releases.
- **Bundled only by release builds.** CI and local builds have no built-in layer unless built with the overlay. The repository stays free of generated data.
