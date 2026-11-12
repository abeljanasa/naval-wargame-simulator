# ADR-0003: Determinism strategy

- **Status:** Accepted
- **Date:** 2026-11-10

## Context

Same scenario + same seed + same command log must give an identical result, bit for bit, on Windows and macOS (ADR-0006). Analysts compare Monte Carlo runs, golden replays guard against regressions, and multiplayer reconnect relies on replaying the command log.

## Decision

### 1. Time

- Fixed step. One tick is 1 s of game time (`sim_core::TICK_SECONDS`).
- Subsystems that run less often (sensors, AI) run on integer multiples of the tick, chosen by `tick % period == 0`.
- Time compression runs more ticks per wall-clock second. It never makes a tick longer.
- No wall clock in simulation logic. `Instant::now` and `SystemTime::now` are banned by `clippy.toml`. The desktop app's real-time pacing lives outside the core and only decides *how many* ticks to run.

### 2. Random numbers

- One `rand_chacha::ChaCha8Rng` per subsystem, held in the `RngStreams` resource.
- Each stream is seeded with the scenario seed and set to a fixed stream number with `ChaCha8Rng::set_stream(StreamId as u64)`. ChaCha's stream parameter gives independent sequences from one key. Adding a draw in one subsystem therefore never shifts another subsystem's sequence.
- RNG state is saved and restored with the save game.
- `rand::thread_rng`, `rand::random` and OS entropy are banned in sim crates.
- Systems that run in parallel must not draw from a shared stream. If a parallel system needs randomness, it derives a per-entity value from (stream, tick, entity ID) and documents it.

### 3. Ordering

- The ECS schedule uses the **single-threaded executor**, and systems are `.chain()`-ed in a fixed, documented order.
- No `HashMap`/`HashSet` in logic (banned in `clippy.toml`). Use `BTreeMap`, `Vec` sorted by a stable key, or `IndexMap`.
- ECS query iteration order is **not** relied on wherever order matters. Systems that need an order collect into a `Vec` and sort by `UnitId`.
- Commands are applied at tick boundaries, ordered by `(tick, sequence number)`. The sequence number is assigned when the command enters the log.

### 4. Floating point

- Use `f64` throughout simulation logic.
- `+ − × ÷` and `sqrt` are IEEE-754 correctly rounded on every target, so they are deterministic. Transcendental functions (`sin`, `cos`, `atan2`, `asin`, `acos`, `tan`, `exp`, `ln`, `powf`, `hypot`, …) in `std` call the platform's C maths library, whose results differ in the last bit between MSVC, Apple and glibc.
- **All transcendental functions in logic go through the `libm` crate** (a pure-Rust port of musl libm), which gives the same bits everywhere. `clippy.toml` bans the `std` `f64` methods.
- No fused multiply-add (`mul_add`), no `-ffast-math`-style flags, no SIMD intrinsics in logic.
- When summing many floats, iterate in a stable order (sorted by entity ID). Any parallel reduction must be documented with its order.

### 5. Verification

- The state hash is FNV-1a 64 over a canonical byte encoding of the world, using `f64::to_bits`, never formatted text.
- CI runs every shipped scenario through the batch runner on all three OSes and fails if the hashes differ.
- Golden replays (stored command log → stored final hash) fail CI on any change. Updating a golden hash must be deliberate and explained in the PR.

## Consequences

- `libm` is slower than platform maths. Profile it first; only consider caching or approximations if a benchmark shows it matters.
- The single-threaded schedule limits parallelism inside one run. Batch throughput comes from running replications in parallel, one per core. Data-parallel systems can be added later behind an ADR, provided they are order-independent and draw no RNG.
