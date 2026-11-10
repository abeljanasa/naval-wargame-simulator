# Performance budgets

Master prompt §12 sets these budgets, measured on the reference laptop with a release build:

| Budget | How to check it |
|---|---|
| 1,000 active entities at ≥ 60× real time | `cargo run --release -p nws-batch -- bench --units 1000 --ticks 600 --min-ratio 60` |
| 10,000 entities at ≥ 1× real time | `cargo run --release -p nws-batch -- bench --units 10000 --ticks 60 --min-ratio 1` |
| Map at 60 fps with 10,000 symbols | `cd apps/desktop && npm run bench:map -- --headed` (see "Map rendering" below) |
| Batch runner scales near-linearly with cores | Phase 4 (Monte Carlo); not measured yet |

## Workload

`nws-batch bench` builds N copies of a database class (default `GEN-FFG`, five radars each).

- **Sides:** the units alternate between two sides.
- **Placement:** seeded random positions in a 20° × 20° sea box, random courses, cruise speed.
- **Timing:** it times `--ticks` ticks after the spawn tick.
- **Load:** every tick runs movement, and every 5th tick runs radar detection across all opposing pairs.
- **Output:** one JSON line with the ratio (game seconds per wall second), the detection draw count and the final state hash.

## Results on the development machine (2026-11-10, Windows 11, release build)

| Units | Before the broad phase | After |
|---|---|---|
| 1,000 | 5.6× | **203×** |
| 10,000 | 0.05× | **2.2×** |

The improvement comes from an exact latitude broad phase in `detection.rs`.

- **Why it's safe:** the geodesic between two points is at least |Δlat| × the minimum meridional radius (110,574 m per degree). So each radar only needs to look at targets inside the latitude band it can possibly reach.
- **Proof the output is unchanged:** the skipped pairs are exactly pairs the model would have skipped anyway, so draw order and results are bit-identical. The golden replay hash is unchanged, and the 1,000-unit, 120-tick benchmark gives hash `14c631676919c2af` both before and after.

## Map rendering (10,000 symbols at 60 fps)

`apps/desktop/bench/map-fps.html` drives the **production map code** (`createMap` and `buildLayers`) with N unit symbols.

- **Updates:** every 33 ms it replaces the unit array with fresh objects at new positions and rebuilds the layers, just as 30 Hz snapshot frames do in the app.
- **Measurement:** it measures `requestAnimationFrame` frame times for 10 s after a 5 s warm-up.
- **Runner:** `scripts/map-fps.mjs` serves the page with Vite and runs it in a Chromium browser through Playwright, reporting fps, frame-time percentiles and the WebGL renderer.
  - If an installed Edge or Chrome is present it uses that; otherwise run `npx playwright install chromium` once.
  - Use `--headed`. Headless Chromium falls back to software WebGL (SwiftShader); the script detects that and does not judge the budget on it.

Results on the development machine (Intel UHD Graphics, Direct3D 11 through ANGLE, 2026-11-10):

| Configuration | fps | p50 / p99 frame | Frames > 50 ms |
|---|---|---|---|
| 10,000 symbols, static | 60.0 | 16.7 / 17.0 ms | 0 |
| 10,000 symbols, 30 Hz updates | **59.2** | 16.7 / 33.3 ms | 0 |
| 10,000 symbols + 10,000 labels + leaders, 30 Hz | 27.8 | 33.3 / 83.1 ms | 24 |

The last row is why the map **declutters**. Above `LABEL_LIMIT` = 1,500 units it drops unit name labels and heading leaders, except for the selected unit. Labels at that density overlap into noise anyway. Symbols are always drawn.

Two smaller fixes also went in:
- The SVG symbol URLs are cached.
- The label glyph set is fixed, instead of being rescanned from every label on every update.

The budget is a GPU budget, so CI (software rendering) does not gate it. Run it on the reference laptop before a gate review.

## CI

- **Gate:** the `performance` job fails if 1,000 units fall below 60×.
- **Recorded only:** the 10,000-unit figure is saved as an artefact without failing the build. GitHub runners are not the reference laptop, and the 10,000-unit budget has only about 2× headroom here.
- **Benchmark regression:** the `bench-regression` job runs on every pull request. It runs the criterion benchmarks (`cargo bench -p sim-scenario --bench engine`: Vincenty direct and inverse, and one 5-tick sensor cycle for 1,000 radar-equipped units) on the PR's base commit and then on its head, **on the same runner**. `scripts/check_bench_regression.py` fails the job if any benchmark is more than 15 % slower. Both the point estimate and the lower 95 % confidence bound must exceed 15 %, so noise alone doesn't fail a PR.
