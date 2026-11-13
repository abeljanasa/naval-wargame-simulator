# Model: Offline imagery layer (Phase 4 slice 6)

- **Code:**
  - `apps/desktop/src-tauri/src/imagery.rs`: pack readers.
  - `apps/desktop/src-tauri/src/lib.rs`: the `nwstiles` scheme and commands.
  - `apps/desktop/src/lib/imagery.ts`: the map layer.
- **Tests:**
  - desktop `imagery` unit tests: MBTiles and PMTiles reading, Hilbert tile ids against the PMTiles specification, install checks;
  - `src/lib/imagery.test.ts`.
- **Design log:** [DL-0042](../design-log/DL-0042-imagery.md)
- **Decision:** ADR-0007 decision 2. The client procures licensed imagery of the PAR and delivers it as a tile pack. No imagery is committed to the repository.

## Packs

The app reads raster tile packs in either of two open formats:

| Format | What is read |
|---|---|
| **MBTiles** (SQLite) | `metadata` (`name`, `format`, `minzoom`, `maxzoom`, `bounds`, `attribution`, `tilesize`), and `tiles`, whose rows count from the south |
| **PMTiles** v3 | The 127-byte header, the root and leaf directories (none or gzip), and JSON metadata (`name`, `attribution`, `tilesize`). Tiles are found by their Hilbert tile id |

- **Tile types:** PNG, JPEG and WebP. Vector tiles are refused with a message, because this is an imagery layer.
- **Where packs live:** the app's `imagery` folder (shown in *View › Imagery packs…*).
- **Installing:**
  - *File › Install imagery pack…* copies a pack of up to 1 GB, checking that it opens before it replaces anything.
  - Larger packs are copied into the folder by hand, then *Read the folder again*.
- **What the app shows:** each pack's name, format, zoom range and attribution, and any file in the folder it could not read, with the reason.

## Serving tiles, offline

- **The scheme:** the map asks for `nwstiles://localhost/<file>/<z>/<x>/<y>` (`http://nwstiles.localhost/…` on Windows, as WebView2 spells custom schemes). The app answers from the pack on disk.
- **Lockdown:** the content security policy allows images and fetches from that scheme alongside the app itself, and nothing else. There is no network fetch, and the app has no updater or tile server.
- **No tile:** a missing tile is a 404, and the map shows the base layers beneath.

## On the map

The chosen pack draws as the base layer, above the sea colour and beneath everything else.
- **Land fill** is hidden while imagery shows. The coastline line stays.
- **Depth shading** is dimmed to 45 %, so the imagery shows through. Contours and relief draw over it.
- **Units, labels and the planning layer** draw on top as always.
- **Attribution:** the pack's own credit joins the map attribution. A pack that gives none shows its name and "the pack gives no attribution: ask its supplier", so the gap is visible.
- **Remembered per viewer:** the choice (*View › Imagery*, or the dialog) is kept like the theme.

## Not yet

The licensed pack itself. Until it arrives, the layer has been checked with a generated test pattern over the Mindoro Strait (tile seams, zoom range and attribution), and with unit tests of both formats.
