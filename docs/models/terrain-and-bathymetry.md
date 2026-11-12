# Model: Terrain and bathymetry packs (Phase 4 slice 2)

- **Code:**
  - `crates/sim-geo/src/terrain.rs`: pack format, sampling, the terrain stack.
  - `crates/sim-core/src/sim.rs`: `set_terrain`, `terrain_id`, and the state hash.
  - `crates/sim-aar`: records and replays.
  - `apps/batch/src/terrain.rs`: builder and netCDF reader.
  - `apps/desktop/src-tauri/src/terrain.rs`: folders, SHA-256 checks, install.
  - `apps/desktop/src/lib/terrain.ts`: map layers.
- **Tests:**
  - `sim-geo` terrain unit tests, including the cross-OS sampling golden;
  - `nws-batch` terrain tests (netCDF reading, averaging, the `.sha256` file);
  - `sim-aar` `terrain_is_hashed_recorded_and_needed_to_replay`;
  - desktop `installs_only_a_pack_that_matches_its_sha256`;
  - `src/lib/terrain.test.ts`.
- **Design log:** [DL-0038](../design-log/DL-0038-terrain-packs.md)
- **Sources and licences:** [data dictionary](../db/data-dictionary.md#terrain-and-bathymetry-sources-phase-4-slice-2-adr-0007)

## What a pack holds

A pack is a regular latitude/longitude grid of heights in whole metres, positive up: the sea floor is negative, land positive.

- **Positions:** grid positions are integers in tenths of an arc-second.
  - The header gives the centre of the south-west cell (`lat0`, `lon0`), the spacing (`cell`), and the size (`rows`, `cols`).
  - A pack whose columns span exactly 360° wraps round the antimeridian.
- **Heights:** they are stored row by row, south to north. Each row is delta-coded west to east and the whole is zlib-compressed. The layout is in the module documentation.
- **Header:** UTF-8 JSON, carrying the pack's name, its source, the credit its licence asks for, and the lowest and highest height.
- **Identity:** a pack's id is `name@` plus the first 16 hex digits of the file's SHA-256, so two different files never share an id.

| Pack | Area | Cell | Grid | File | Uncompressed |
|---|---|---|---|---|---|
| `gebco2020-par-15s` | 3–27°N, 111–137°E (the Philippine AOR, ADR-0007) | 15″ (≈ 460 m) | 6,241 × 5,761 | 30.5 MB | 71.9 MB |
| `gebco2020-global-4m` | The world | 4′ (≈ 7.4 km) | 5,400 × 2,700 | 17.5 MB | 29.2 MB |
| `gebco2020-global-2m` (measured, not used) | The world | 2′ | 10,800 × 5,400 | 64.5 MB | 116.6 MB |

The 4′ global layer is the one meant for the installer (decision 4 of the [Phase 4 plan](../plan/phase-4.md)). The 2′ layer is about four times larger for detail the regional pack already gives where it matters.

## Building a pack

`nws-batch terrain build <file.nc> --name <name> [--average N] --out <pack.nwsterrain>`

- **Input:** a GEBCO subset in netCDF-3 classic, as NOAA ERDDAP serves it: `elevation[latitude][longitude]` (short), with both axes ascending and evenly spaced.
  - The reader is written here, about 150 lines. A netCDF-3 file is a short big-endian header followed by the arrays, so no C library is needed. netCDF-4/HDF5 files are refused with a message.
- **Checks:** both axes must fall on whole tenths of an arc-second and be evenly spaced, and the two spacings must be equal.
- **`--average N`** averages `N × N` blocks in integers (mean, rounded half up) and moves the cell centres to the middle of each block. The 4′ layer is the 2′ download with `--average 2`.
- **Output:** the builder writes the pack and a `sha256sum`-style `.sha256` file next to it.
- **Speed:** the 15″ pack builds in about 10 s, release build on the reference laptop.

`nws-batch terrain info <pack>` prints the id, SHA-256, grid and coverage. `nws-batch terrain sample <packs…> --lat --lon` prints the height and depth at a point.

## Sampling

`TerrainPack::height_m` interpolates bilinearly between the four nearest cell centres:
- `y = (lat × 36,000 − lat0) / cell`, and likewise for `x`;
- then `floor`, and two linear blends along longitude followed by one along latitude.

Only `+ − × ÷`, `floor` and `%` are used, so a height is bit-identical on every OS (ADR-0003). The `sampling_is_bit_identical_everywhere` test folds 20,000 samples over an uneven grid into one number, pinned in the test. CI runs it on Windows and macOS.

- **Edges:** within half a cell of the outer rows and columns, the edge value holds. Beyond that the pack does not cover the point (`None`).
- **Wrapping:** a global pack wraps in longitude. A regional pack east of the antimeridian is also found from longitudes near −180°.
- **The terrain stack:** `Terrain` holds every pack in use, finest first, then by id. A point takes its height from the finest pack that covers it, so the 15″ pack overrides the 4′ layer inside its box.
  - The order never depends on how the packs were found.
  - The stack's id joins the pack ids with `+`.
- **Depth:** `depth_m` is `max(0, −height)`.

Sanity checks against the built packs (the 15″ pack, falling back to the 4′ layer):

| Point | Pack value | Reference |
|---|---|---|
| Mt Apo summit | 2,792 m | 2,954 m: the summit is narrower than a 15″ cell |
| Philippine Trench, 10.33°N 126.67°E | −9,246 m | The trench floor nearby is about −10,000 m |
| Manila Bay shore | +11 m | A low coast |
| Mindoro Strait, 12.6°N 120.6°E | −346 m | |
| Challenger Deep area (4′ layer) | −10,319 m | −10,900 m at the deepest point: averaging smooths it |

## The state hash, saves and records

- **State hash:** the terrain id is hashed only when terrain is set (`"terrain"` and the id). Runs without packs keep their hashes, so the golden replays did not change. A run with packs cannot silently match one without them.
- **Saves:** like the coastline, terrain is not saved; it is set again after loading.
- **Records:** an exercise record stores the terrain id (`terrain`, absent in older records). `Replay::open_with_terrain` opens a record made with terrain only when given packs with exactly that id. Otherwise it refuses with `UnknownTerrain`, naming the packs it needs. Keyframe restores set the terrain again.
- **Use:** no model reads heights yet. Shallow water and routing by draft (slice 3), terrain masking (slice 4), choke-point depths (slice 5) and bottom bounce (slice 8) build on this.

## On this machine (desktop)

- **Folders:** packs are read from the app's bundled `terrain/` resource folder, then from the installed-packs folder (`<app data>/terrain`, shown in the app).
  - A pack with a `.sha256` file next to it must match it, or it is not used, and the reason is shown.
  - Reading happens on a thread of its own at start-up: about 2 s for the 15″ pack.
- **Which exercise uses them:** the exercise on screen takes the packs if it is still at its starting picture. Otherwise the next scenario opened does. The *Terrain packs* dialog says which.
- **Install:** *File › Install terrain pack…* takes the pack and its `.sha256` file together.
  - The bytes go to the host as raw IPC, with the `.sha256` line in a header.
  - The host compares the SHA-256 before anything is written. It then writes the pack (via a `.part` file) and its `.sha256`.
  - A copy that does not match is refused as damaged or altered.
- **Map layers:** the map asks for heights for the visible area (`terrain_grid`) when it stops moving or is resized.
  - The grid is one cell per two screen pixels, at most 1,024 each way, with rows evenly spaced in Web Mercator so it lines up with the coastline at any latitude.
  - It is drawn as three layers below the coastline and below every unit and label:

    | Layer | Drawn as |
    |---|---|
    | Depth shading | Colours interpolated on log depth, pale inshore and dark offshore |
    | Depth contours | Marching squares; fewer and deeper levels when zoomed out, such as 200 and 2,000 m at world zoom and 10–500 m at harbour zoom |
    | Relief | Hill shading over land, lit from the north-west at 45° with 2× exaggeration |

  - The status bar shows the depth or height under the cursor.
  - The GEBCO credit is added to the map attribution.
