# DL-0001: Simulation loop, RNG streams, command/snapshot protocol, saves

- **Date:** 2026-11-10
- **Source:** Original design, from the project master prompt §4 and §6.2 and ADR-0003. No external product was consulted.
- **Code:** `crates/sim-core`

## Decisions

- **Clock:** a 1 s fixed tick, with the ECS schedule running apply-commands → movement → advance-clock, single-threaded.
- **Commands:** `Command { tick, issuer, side, payload }`, versioned by `PROTOCOL_VERSION`.
  - A command is accepted only for the current or a future tick, and gets a sequence number when it enters the log.
  - It is applied at the start of its tick.
  - Only the scenario loader and the white cell may spawn units or command another side's units. A player seat can command only its own side; anything else is rejected and logged as `CommandRejected`.
- **Snapshots:** filtered by viewer. White cell sees ground truth. A side sees its own units, plus contacts from Phase 2.
- **Saves:** MessagePack containing the world, RNG state, command log and event log.
  - The header is checked first. A save from a newer format version is refused with a clear error.
- **Replay:** scenario seed + command log reproduces the identical state hash (tested).
- **State hash:** FNV-1a over a canonical encoding with floats as their bit patterns. It is a regression check, not a security control.
