# Phase 2 plan: Core combat (Gate 2: duel tests pass)

- **Started:** 2026-11-10, on branch `phase-2/core-combat`, at the client's instruction. The Gate 1 review pack recommends GO with three conditions; the formal SME review has not been held yet. Anything it changes feeds back into this plan.
- **Scope (master prompt §13):** radar, sonar, ESM, IR/visual; contacts and tracks; weapons, Ph, damage, point defence, soft kill; doctrine, ROE, WCS; EMCON; basic missions. P1 basic mines (§8).
- **Gate 2:** every model in this phase has a duel test against hand calculation (§12), and the whole suite passes in CI on every OS that ships.

## Architecture (ADR-0005)

Subsystem crates hold the **models** as plain, deterministic functions and data types with no ECS dependency: `sim-sensors`, `sim-weapons`, `sim-c2`, `sim-ai`. `sim-core` keeps the world, the schedule and the systems, and calls those models. Duel tests run against the pure functions, so they need no world.

## Slices

Each slice is one or more small PRs that leave CI green, with its model doc, design-log entry, duel tests, UI and CHANGELOG in the same change (§15.6, §16).

| # | Slice | Depends on | Main model decisions (original design unless a source is named; all pending SME validation) |
|---|---|---|---|
| 1 ✅ | **`sim-sensors` crate + tracks.** Gate 1 contacts become tracks: measured position with error, an error ellipse that grows with time since update, a classification ladder, fusion of every sensor on a side into one track per target | — | Measurement error per sensor kind (range σ, bearing σ); growth by the fastest speed the track's classification allows; ladder steps reachable per sensor kind |
| 2 ✅ | **C2 basics:** side postures (friendly/neutral/unfriendly/hostile), doctrine inheritance side → group → unit, ROE, WCS (safe/tight/free), EMCON states | — | Which doctrine fields exist in Phase 2; what WCS "tight" means for an unknown track |
| 3 ✅ | **Radar, full Phase 2 model:** sea clutter for low-altitude targets, jamming hook (noise jamming burn-through), weather hook (constant until Phase 4 weather), EMCON-aware emission | 1, 2 | Clutter as an R₅₀ penalty below a height threshold vs sea state; burn-through range equation |
| 4 ✅ | **ESM:** detects active emitters beyond their own range, bearing lines, multi-platform triangulation fixes, emitter-library identification with confidence | 1, 2, 3 | Bearing σ per ESM class; fix quality from crossing angle |
| 5 ✅ | **Sonar:** passive SE = SL − TL − (NL − DI) − DT, speed-dependent source level, spherical/cylindrical spreading + absorption; active sonar reveals the emitter; hull, towed, dipping, sonobuoy | 1 | TL regime switch range; absorption coefficient; no SVP, layers or CZ until Phase 4 |
| 6 ✅ | **Visual, IR/EO, MAD:** short range, daylight and weather dependent | 1 | Daylight from solar elevation; visual range from target height and size |
| 7 ✅ | **Weapons as entities:** missiles, torpedoes, guns, bombs; launch needs a track of the quality the weapon requires (third-party targeting via shared tracks); seeker acquisition; flight on the movement model | 1, 2 | Required track quality per guidance type; seeker cone and acquisition Pd |
| 8 ✅ | **Ph and damage:** Ph = base × modifiers (size, manoeuvre, countermeasures, environment); damage points; critical hits on sensors, propulsion, weapons, flight deck, comms; fire and flooding with damage control; mission kill vs loss | 7 | Modifier tables; damage points from displacement; critical-hit table |
| 9 | **Point defence and soft kill:** engagement opportunities from reaction time, range and fire channels (saturation emerges); chaff, flares, decoys, ECM, torpedo countermeasures | 7, 8 | Reaction time per system; soft-kill seduction probabilities |
| 10 | **Basic missions:** patrol (area), strike, escort/screen, ferry; automated target assignment under ROE/WCS | 2, 7 | Mission set for Phase 2; AI cadence (every 10–30 s per §6.2) |
| 11 | **Basic mines (P1):** laying, fields, transit risk | 7, 8 | Actuation probability per transit; field density |
| 12 | **Gate 2 pack:** the full duel suite, a combat demo scenario, golden replays, a review pack | all | — |

Out of phase (stay in the backlog): sound-velocity profiles, layers and convergence zones; weather zones; air ops and sortie generation; Lua events; scenario editor; operational planning layer.

## Data

The open database already has 33 generic sensors and 27 generic weapons with `base_ph`. Each slice adds the fields its model needs through a forward-only migration, with generic UNCLASSIFIED values, citations and confidence D, and new validation rules. Until a field exists, the model uses a documented per-kind default (listed in its model doc).

## Determinism

Every new random draw uses a named RNG stream (Sensors, Weapons, Damage, AI) and is logged with its inputs (§8). Every slice that changes outcomes updates the golden hash, and states why in its commit.
