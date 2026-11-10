# DL-0020: Basic mines: fields, laying and transit risk

- **Date:** 2026-11-10
- **Phase:** 2, slice 11 ([plan](../plan/phase-2.md))
- **Sources:**
  - Master prompt §8 ("Mines (P1 basic, P2 full): laying, fields, transit risk, mine-countermeasures missions").
  - The textbook "simple initial threat" for a uniform random minefield, P = 1 − exp(−ρ·W·d).
  - Platform database mine figures (generic bottom mine: 50 m actuation, 600 kg, Ph 0.5).
  - Original design: circular fields, swept width as twice the actuation radius, 10 s checks, owner immunity (safe lanes), and floods like a torpedo on a hit.
  - No vendor product, database or binary was consulted.
- **Model doc:** [weapons-damage.md](../models/weapons-damage.md) §Mines

## Behaviour

- Fields are created by the scenario or white cell, or laid by a ship from its stores.
- Ships and submarines of other sides moving through a field take a logged risk draw every 10 s; an encounter uses a mine and rolls its Ph and damage.
- The owning side is not at risk and only it (and the white cell) sees the field.
- Gate 2 duel: 200 transits match the hand-calculated 0.471 encounter probability within 4σ.
- Mine countermeasures (P2, full mines) are not in this slice.

## Compatibility

- Save format 12. New commands `CreateMinefield` and `LayMines`; optional `minefields` in scenarios.
- Golden hash `b296e935e3375df2` → `1f07e67a956cb372` (minefield state is hashed; the demo has none).
