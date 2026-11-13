# DL-0042: Offline imagery layer

- **Date:** 2026-11-13
- **Phase:** 4, slice 6
- **Sources:**
  - ADR-0007 decision 2 (client-licensed imagery, delivered as a tile pack).
  - The MBTiles 1.3 specification (Mapbox, CC-BY).
  - The PMTiles v3 specification (Protomaps, BSD-3).
  - Original design otherwise.

## Decisions

- **Both open pack formats, read here.**
  - MBTiles through the bundled SQLite that the platform database already uses.
  - PMTiles v3 with about 150 lines: header, varint directories, and Hilbert tile ids, which are checked against the specification's examples.
  - `rusqlite` and `flate2` were already in the lockfile, so there are no new crates.
- **Served through the app's own scheme.** A custom `nwstiles` scheme answers tile requests from the pack on disk. The content security policy is widened to that scheme only, so the map still cannot reach a network.
- **Install through the window up to 1 GB; larger packs by copying.**
  - The window sends the bytes to the app, so very large files would sit in memory.
  - Imagery of the PAR at useful zooms can be many gigabytes, so the folder route is documented and the app rescans on request.
  - No file-dialog plugin was added, to keep the dependency set and the capability surface unchanged.
- **Attribution is always shown.** It is the pack's own, or a visible note that the pack gave none, so a missing credit is noticed rather than hidden.
- **Imagery under the picture.** The land fill hides and the depth shading dims, so the imagery reads; the coastline, relief, contours and the whole tactical picture stay on top.
- **Display only.** Imagery changes nothing in the engine, the state hash or records.

## Open

- The client's licensed pack (ADR-0007). On arrival, check its attribution text and zoom range in *Imagery packs…*.
