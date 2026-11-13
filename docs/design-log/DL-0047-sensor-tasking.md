# DL-0047: Automatic sense-zone tasking

- **Date:** 2026-11-13
- **Phase:** 4, slice 12
- **Sources:**
  - Master prompt §7.1 (the planning layer: the engine places and tasks sensors to keep sense zones covered, in the analysis phase).
  - The [Phase 4 plan](../plan/phase-4.md), slice 12.
  - The Phase 3 coverage model ([DL-0033](DL-0033-coverage.md)).
  - Original design otherwise.

## Decisions

- **A goal per zone, set by the planner.** The plan asks for "coverage above a planner-set level". A share of the zone's area is what the coverage model already reports, so the goal and the measure are the same number. A zone may name the units the engine may use; without them, any free unit is used.
- **Greedy, one unit at a time.** Each step takes the unit and station that would cover the most uncovered cells. This is fast enough to run every five minutes for many zones, deterministic, and easy to explain in the log: each line says what was sent, where, and the coverage before and after. Optimal set cover was rejected as slow and opaque, for little gain at the coverage model's resolution.
- **Only free units.** A unit with a mission is a player's or the AI's decision, and the engine does not override it. A player taking a tasked unit for something else releases it, and the next pass sends another.
- **Aircraft through air operations.** A parked aircraft is tasked by launching it on a patrol sortie. Readiness, runways, bingo and return all apply, and its going home releases it, so reliefs come out of the same rule.
- **Reviews every five minutes.** That is slow enough not to churn units as coverage flickers, and fast enough to replace a unit within a TTX turn. Shortfalls are logged once, not every pass.
- **No change without goals.** The tasking state is hashed only when present, so every shipped scenario keeps its golden replay; checked.

## Open

- Moving units between zones, or off a zone over its goal.
- Ordering sonobuoy fields and towed-array streaming for ASW zones.
- Reach at the station rather than at the unit, and terrain masking in the search.
