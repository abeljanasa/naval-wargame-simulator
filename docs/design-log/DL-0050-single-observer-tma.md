# DL-0050: Single-observer target motion analysis

- **Date:** 2026-11-13
- **Phase:** 4, Gate 4 finding 1 ([Gate 4 pack](../gate4/README.md))
- **Sources:**
  - The Gate 4 *Hangor* draft, in which a lone submarine never attacked.
  - Public textbook bearings-only tracking: the maximum-likelihood estimate solved by Gauss–Newton/Levenberg–Marquardt, and the known short-range bias of the pseudo-linear estimator.
  - Original design for the history rules.

## Decisions

- **Range from one observer's bearings over time.** A cross-fix needs two observers in the same scan, so a lone submarine could never form a track on passive sonar. The engine now solves each observer's recent acoustic bearings for a constant-velocity target.
- **Maximum likelihood, not pseudo-linear.**
  - The pseudo-linear estimator is linear and simple, but it minimises cross-range distances. With real bearing noise it collapses toward the observer (140 m against a true 7.3 km in the engine test).
  - The maximum-likelihood fit over angular residuals has no such bias. It is solved from seven starting ranges, so it does not hang on one local minimum.
- **Observability decides.** No range is invented for an observer that has not manoeuvred: the solution is refused when the normal matrix is singular or the error is over 30 % of the range. The solution's covariance enters the track, so a weak solution cannot meet a weapon's track-quality limit.
- **Acoustic only.** ESM ranging from one platform is a separate change, not needed for the engagements in hand.
- **Hashed only when used**, like the other Phase 4 state.

## Effect on Gate 4

*Hangor* now ranges and attacks the frigates on passive sonar. See the regenerated [report](../gate4/validation/hangor-khukri-1971/report.md).

## Open

- A deliberate TMA manoeuvre for submarines with a passive contact.
- ESM single-platform ranging.
