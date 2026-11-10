# ADR-0005: Model crates hold pure functions; sim-core runs them

- **Status:** Accepted
- **Date:** 2026-11-10

## Context

The master prompt (§6.1) lays out subsystem crates: `sim-sensors`, `sim-weapons`, `sim-c2`, `sim-ai`. Phase 1 kept everything in `sim-core`, including the interim detection model. Phase 2 adds most of the models, and each needs a duel test against hand calculation (§12).

If each subsystem crate owned ECS systems, it would need `sim-core`'s components, and `sim-core` would need its systems to build the schedule. That is a dependency cycle, broken only by a plugin layer that adds indirection without a current need.

## Decision

1. **Subsystem crates hold models only:** plain, deterministic functions and data types, with no `bevy_ecs` dependency. They may depend on `sim-geo` and on each other in the order sensors → c2 → weapons → ai.
2. **`sim-core` holds the world:** components, resources, the schedule and the systems. Systems gather inputs from the world, call the models, and write results and events back.
3. **Types that cross the protocol** (for example `SensorKind`, `Classification`, `ErrorEllipse`) live in the model crate that defines them and are re-exported by `sim-core`, so existing import paths keep working.
4. **Duel tests** for a model call its functions directly. Engine-level tests in `sim-core` check that the systems wire the model in correctly.
5. The determinism rules of ADR-0003 apply to model crates unchanged: `libm` for transcendental functions, no hash-ordered containers, no clock, and randomness only as arguments (uniform draws passed in by the caller, which owns the RNG streams).

## Consequences

- Models are testable without a world and readable without knowing the ECS.
- Randomness stays in `sim-core`, where the stream order is fixed, so a model cannot consume draws in an unexpected order.
- `sim-core` grows a system per subsystem. If it becomes too large, a later ADR can move systems out behind a plugin trait.
