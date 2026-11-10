# Performance budgets

Master prompt §12 sets these budgets, measured on the reference laptop with a release build:

| Budget | How to check it |
|---|---|
| 1,000 active entities at ≥ 60× real time | `cargo run --release -p nws-batch -- bench --units 1000 --ticks 600 --min-ratio 60` |
| 10,000 entities at ≥ 1× real time | `cargo run --release -p nws-batch -- bench --units 10000 --ticks 60 --min-ratio 1` |
| Map at 60 fps with 10,000 symbols | Not automated yet (backlog) |
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

## CI

- **Gate:** the `performance` job fails if 1,000 units fall below 60×.
- **Recorded only:** the 10,000-unit figure is saved as an artefact without failing the build. GitHub runners are not the reference laptop, and the 10,000-unit budget has only about 2× headroom here.
- **Not yet built:** criterion micro-benchmarks and an automatic "> 15 % regression against the stored baseline" comparison are in the backlog.
