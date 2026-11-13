# Model: Geodesy and surface movement (Phase 1)

- **Code:** `crates/sim-geo/src/vincenty.rs`, `crates/sim-core/src/movement.rs`
- **Tests:** `sim-geo` unit and property tests, and `sim-core` `movement::tests`, `tests/core.rs`
- **Design log:** [DL-0002](../design-log/DL-0002-geodesy-and-movement.md)

## 1. Earth model

WGS-84 ellipsoid: a = 6 378 137 m, f = 1/298.257 223 563, b = a(1 − f) (NIMA TR8350.2).

## 2. Geodesics

Distances and azimuths use Vincenty's (1975) inverse and direct solutions, which are accurate to about 0.5 mm.

- **Inverse** (point → point): iterate on λ until |Δλ| < 10⁻¹², with a maximum of 200 iterations. For nearly antipodal pairs the iteration may not converge. In that case we fall back to a great circle on a sphere of mean radius R₁ = (2a + b)/3, which has error ≤ about 0.5 %. The result is flagged `InverseMethod::SphericalFallback`.
- **Direct** (point + azimuth + distance → point): iterate on σ, which always converges.

All trigonometry goes through the pure-Rust `libm` crate, so results are bit-identical on every OS (ADR-0003).

### Verification against published values

| Test | Reference | Tolerance | Result |
|---|---|---|---|
| Flinders Peak → Buninyong, inverse | Geoscience Australia worked example: s = 54 972.271 m, α₁ = 306°52′05.37″, reverse α = 127°10′25.07″ | 1 mm, 0.006″ | pass |
| Same line, direct | Lands on Buninyong | 0.0005″ | pass |
| Equator to pole (quarter meridian) | 10 001 965.729 m | 1 mm | pass |
| 1° of equator | a·π/180 = 111 319.491 m | 1 µm | pass |
| direct(inverse(p₁, p₂)) returns to p₂ | Property test, 256 random pairs | 1 mm | pass |

## 3. Movement (per 1 s tick, units in `UnitId` order)

1. **Speed:** v ← v + clamp(v_ordered′ − v, −a·Δt, +a·Δt), where v_ordered′ = clamp(v_ordered, 0, v_max).
2. **Distance budget:** d = v·Δt. This is semi-implicit Euler: the speed after the update is used.
3. **With an active route:** follow the geodesic to the current waypoint.
   - If d ≥ the leg length (within 1 mm), snap to the waypoint, log `WaypointReached`, and carry the leftover distance onto the next leg.
   - After the final waypoint, log `RouteCompleted` and come to **all-stop** (v = v_ordered = 0).
4. **Without a route:** dead-reckon along the current heading. Heading is updated to the geodesic's forward azimuth at the new point, so the unit follows a great circle, not a rhumb line.
5. **Ships and submarines keep clear of each other**, and are damaged when they do not: see [collisions.md](collisions.md). A hull giving way steers its avoidance heading instead of its route for that tick, then comes back to the route or to its ordered heading.

### Worked example (hand calculation, matched by `hand_calculated_transit_time`)

A unit already at 10 m/s with one waypoint 10 000 m due east along the equator.

- Each tick moves 10 m, so the waypoint is reached on tick 10 000 / 10 = **1 000**, exactly.
- The test asserts 1 000 ticks.

From rest with a = 0.1 m/s² and v_ordered = 10 m/s:

- After 1 tick, v = 0.1 m/s.
- v reaches 10 m/s after 100 ticks (test `acceleration_is_limited`).

## 4. Assumptions and limitations (Phase 1)

- **Turning** (added 2026-11-10). A unit with turn data steers toward its waypoint at most at rate ω = min(ω_max, v / r_min) (deg/s), then moves along its new heading.
  - It cannot turn while stopped.
  - A waypoint counts as reached within max(r_min, 50 m, the tick's distance). The unit is not snapped onto it.
  - Units without turn data keep the instant-turn behaviour.
  - The database has no turn data yet, so scenario resolution uses generic class values: ships and submarines r_min = 2.5 × hull length (tactical diameter ≈ 5 lengths), ω_max = 3°/s; fighters 15°/s; helicopters and UAVs 8°/s; patrol, AEW and tanker aircraft 3°/s; land units unlimited.
  - **Worked example:** a frigate (135 m) at 10 m/s turns at min(3, 10 / 337.5 rad/s) = 1.70°/s, so a 180° reversal takes about 106 s. The unit test reverses a 10 m/s, 400 m-radius unit onto a waypoint 5 km behind it: the turn takes about 126 s and it arrives within 500–700 s.
- **No land or shallow-water blocking, and no draft limits.** Units can cross land. (Backlog: needs coastline and bathymetry sampling plus navigable-water pathfinding.)
- **No sea-state speed penalty, no fuel burn, no altitude or depth bands.** Altitude is stored but not changed.
- **All-stop is instantaneous on completing a route.** The unit does not decelerate into the final waypoint.
- **Spherical fallback near antipodes:** error up to about 0.5 %. It only happens on legs over ~19 000 km, which no realistic waypoint leg needs.
