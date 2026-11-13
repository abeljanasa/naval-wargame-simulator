# DL-0044: Ocean acoustics, towed arrays and sonobuoys

- **Date:** 2026-11-13
- **Phase:** 4, slice 8
- **Sources:**
  - Master prompt §13 (acoustics: SVP, layers, CZ).
  - The [Phase 4 plan](../plan/phase-4.md), slice 8.
  - Public textbook acoustics: Urick, *Principles of Underwater Sound*; Jensen et al., *Computational Ocean Acoustics*. Used for the surface-duct skip distance, ray arcs in linear-gradient layers, the reverberation-level form and Thorp absorption.
  - Original design otherwise.

## Decisions

- **Closed-form paths, not a ray tracer or a normal-mode model.** In a piecewise-linear profile every ray is a chain of circular arcs, so the limiting ray's CZ range is a sum of square roots. That is exact for the profile, deterministic with plain arithmetic and `libm`, and cheap enough for every scan. A full propagation model (rays, modes, parabolic equation) would be slower and harder to keep bit-identical across operating systems, for little gain at the fidelity of the database.
- **The least loss over the open paths.** Real fields add paths incoherently. Taking the strongest is simpler, close in practice, and makes the logged path say plainly why a contact was heard.
- **No profile, no change.** Without a profile the Phase 2 law holds bit for bit. Existing scenarios keep their results and their golden replays; checked.
- **Profiles by area and month, the higher id winning.** This is the same rule as weather zones, so the white cell has one rule to learn. Seasons come from the scenario's clock, so a long scenario crossing a month boundary changes profile.
- **Bottom bounce only where the depth is known.** It uses the terrain packs (slice 2). Without a pack, no bounce path is claimed.
- **Reverberation only with a profile.** It adds a masking term to active sonar. It is kept under the profile switch so that older scenarios are unchanged.
- **Towed arrays start streamed.** That matches how scenarios have run so far. Streaming and recovery take a fixed 10 minutes; the array listens only at or below 15 kn.
- **Sonobuoys as bearing sources.** Each buoy feeds the same bearing pool as ESM and passive sonar, under a synthetic observer id per buoy, so crossing buoy bearings triangulate without new fusion code. Buoys report for the side that laid them, which sees them on the map.
- **Database figures optional.** The five sonar figures added in migration 0003 replace the generic figures by kind only where given. A sonar without them hashes as before.

## Open

- Range-dependent profiles (fronts, eddies) and per-path profile sampling.
- Sediment types and multiple bottom bounces.
- Sonobuoy drift with the wind and current; active and DIFAR buoy types; buoy radio range to the aircraft.
- Towed-array depth by speed and cable scope.
- SME validation of the generic figures (Gate 4, the submarine/ASW engagement).
