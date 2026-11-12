# DL-0024: Sandboxed Lua scripting

- **Date:** 2026-11-11
- **Phase:** 3, slice 4 ([plan](../plan/phase-3.md))
- **Sources:**
  - Master prompt §7 (Lua scripting), §12 (mlua, Lua 5.4) and §13 (Phase 3).
  - mlua 0.12 with the `lua54` and `vendored` features: MIT. The Lua 5.4 source is MIT, built from the bundled source with no system Lua, so it works air-gapped. `cargo deny` passes.
  - Original design:
    - Lua conditions and actions on events;
    - the `sim` API;
    - the sandbox and its limits;
    - the determinism replacements.
  - No vendor product, database or binary was consulted.
- **Behaviour doc and reference:** [lua-scripting.md](../models/lua-scripting.md)

## Decisions

- **A fresh interpreter per run.**
  - Benefits: nothing leaks between scripts or seats; no Lua state needs saving or hashing; the engine stays `Send`. Scripts run only when their event's trigger occurs, so the setup cost (well under a millisecond) is negligible.
  - Cost: scripts cannot keep their own state. Event state covers that need.
- **Actions are requests, not side effects.**
  - The script returns a list of event actions, and the engine carries them out after the script ends.
  - So a script obeys every check an action does: validation, authority and land avoidance.
  - A script also cannot run another script.
- **No `pcall`/`xpcall`.** With them, a script could catch the instruction-limit error and loop forever. TTX scripts do not need error recovery.
- **Determinism by replacement:**
  - sorted `pairs`;
  - address-free `tostring` and `string.format`;
  - `libm` maths;
  - `sim.random()` on the `Events` stream;
  - no finalizers or weak tables.
  
  The remaining gap (the `^` operator and float formatting) is documented. The demonstrator's script runs on every OS in CI and its output is part of the golden hash, so any divergence fails the build.
- **Compile at load.** A syntax error is refused when the events load, not mid-exercise.

## Compatibility

- Save and scenario formats are unchanged: scripts are strings inside events.
- Golden hash: phase 3 `1928186cbeaf1578`, because the demonstrator gains the `e08-umpire-brief` script. Phases 1 and 2 are unchanged.
