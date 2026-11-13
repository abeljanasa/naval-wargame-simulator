# Model: single-observer target motion analysis (TMA)

- **Phase:** 4, Gate 4 finding 1 ([DL-0050](../design-log/DL-0050-single-observer-tma.md)).
- **Code:**
  - `crates/sim-sensors/src/tma.rs`: the estimator (`tma_solve`).
  - `crates/sim-core/src/tma.rs`: the bearing histories (`Tma`).
  - `crates/sim-core/src/esm.rs`: `resolve_bearings`, where TMA runs.
- **Tests:**
  - `sim-sensors` unit tests (hand cases).
  - `sim-core` `tma` unit tests (history rules).
  - `crates/sim-scenario/tests/tma.rs` (engine).

A passive sonar gives a bearing, not a range. Two observers' bearings taken in the same scan cross at the target ([esm.md](esm.md)). One observer can range a target too, from its own bearings over several minutes, provided it manoeuvres. That is how a submarine localises a ship before attacking it on passive sonar alone.

## Bearing histories

- **What is kept:** each side keeps, for each target and each of its observers, the passive acoustic bearings (hull and towed sonar, sonobuoys) on that target:
  - one every 15 s (`TMA_SAMPLE_TICKS`);
  - over the last 15 minutes (`TMA_WINDOW_TICKS`).
- **When a history is dropped:** after 2 minutes with no new bearing (`TMA_GAP_TICKS`), for instance when the target fades.
- **Save and hash:** the histories are saved (format 27) and hashed only when present. Runs with no passive acoustic bearings therefore keep their hashes.

## The estimate

When the scan's bearings on a target come from one observer only, or do not cross, the engine solves each observer's history. The target is assumed to move at constant velocity, p(t) = p + v t, with p its position now. The four unknowns x = (pₑ, pₙ, vₑ, vₙ) minimise the weighted squared bearing residuals:

Σᵢ ((βᵢ − β̂ᵢ(x)) / σᵢ)², with β̂ᵢ(x) = atan2(eᵢ, nᵢ) and (eᵢ, nᵢ) = p + tᵢ v − oᵢ,

for bearings βᵢ taken at tᵢ ≤ 0 from oᵢ, in the local east/north plane centred on the newest observer position.

- **Method:** this is the maximum-likelihood estimate for Gaussian bearing errors. It is solved by Levenberg–Marquardt, started at rest at 1, 2, 4, 8, 16, 32 and 64 km along the newest bearing; the lowest-cost solution is kept.
- **Covariance:** the top-left 2 × 2 block of (JᵀWJ)⁻¹, with J the gradient of β̂ and W = diag(1/σᵢ²).
- **Track:** the solution enters the side's track as a position measurement with that covariance (sensor `tma`). Fusion, track quality and the engagement checks then apply as for any other measurement. A `TmaSolution` event (range, error, bearings used) is logged once per history and shown on the AAR timeline.

A solution is refused when:

- there are fewer than 8 bearings, or they span less than 3 minutes;
- the normal matrix is singular. This is the observer on one straight leg at constant speed, against a target at constant velocity: the range is then unobservable;
- the bearings fit badly: mean squared normalised residual above 9 (a 3σ RMS misfit), which means the target manoeuvred;
- the solution is behind the observer, nearer than 100 m, or faster than 20 m/s;
- its one-sigma error exceeds 30 % of its range (it gives a direction, not a distance).

The simpler pseudo-linear estimator was tried first and rejected. It minimises cross-range distances, which favour short ranges. In the engine test it put a frigate 7.3 km away at 140 m.

## Hand values

| Test | Case | Result |
|---|---|---|
| `a_manoeuvring_observer_ranges_a_moving_target` | Exact bearings every 30 s; observer 6 min north then 6 min east at 3 m/s; target 10 km ENE making 6 m/s west | Position within 30 m (flat-plane approximation), velocity within 0.1 m/s |
| `a_straight_leg_does_not_range_a_moving_target` | Same, observer on one straight leg | No solution |
| `solve4_inverts_by_hand` | diag(2, 4, 5, 10) | x = b / diag, inverse = 1 / diag |
| Engine: `a_manoeuvring_submarine_ranges_a_ship_from_its_own_bearings` | Silent submarine, dog-leg; silent frigate | One solution after the turn, true range within 3σ, contact near the truth |
| Engine: `a_stopped_submarine_cannot_range_a_ship` | Submarine stopped | No solution, no contact |

## Limitations

- Constant-velocity target over the window; a manoeuvring target is refused until the history refills.
- Only acoustic bearings: ESM bearings from one platform are not ranged.
- No automatic TMA manoeuvre: a submarine ranges only if its mission or orders already make it turn (a patrol does).
- Each observer's history is solved alone; histories from several observers are not combined, except through track fusion.
