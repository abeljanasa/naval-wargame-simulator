# DL-0049: Model validation against historical engagements (Gate 4 drafts)

- **Date:** 2026-11-13
- **Phase:** 4, slice 14
- **Sources:**
  - Master prompt §12, §13 (Gate 4: historical replays).
  - The [Phase 4 plan](../plan/phase-4.md) decision 3 and [ADR-0007](../adr/0007-phase-4-region-and-data.md): Operation Praying Mantis and *Hangor* vs *Khukri*.
  - The client's choices (2026-11-13): drafts by the development team for SME review; engagement outcomes as the provisional measures.
  - Public accounts of the engagements (cited in each spec).
  - The Wilson score interval (public statistics literature).
  - Original design otherwise.

## Decisions

- **A spec per engagement, separate from the scenario.** The scenario is an ordinary scenario. The spec holds the record, the sources, the assumptions and the measures, so the SMEs can review and correct the history without touching the model. Both sit in `data/validation/`, out of the shipped scenarios and their golden replays.
- **Outcome measures, as the client chose.** These are units lost, survived or disabled, losses per side, weapons fired, hits, and time to first hit. They can be counted in any engagement from the event log.
- **Verdicts say how surprising the record is, not pass or fail.**
  - A yes/no record is compared through the model's probability of it, with a Wilson interval (sound at 0 and 1, unlike the normal approximation).
  - A count is compared through the 5–95 % range.
  - Every difference goes into a list for the SMEs to explain.
- **The runner is shared.** The Monte Carlo runner gained a generic `run_with`, so validation uses the same parallel, seed-ordered, deterministic runs.
- **Report the findings; do not tune them away.** The drafts revealed two model limitations: no single-platform TMA, and one range per weapon for all target domains. They are reported in the [Gate 4 pack](../gate4/README.md) for the client to decide on. One scenario error was fixed: the A-6Es had loitered over their targets rather than at stand-off.

## Open

- The SMEs' review of both drafts, and their explanations of each difference.
- Single-platform bearings-only TMA; weapon ranges per target domain.
- The *Samuel B. Roberts* mine strike as the alternate engagement.
- The client's own MOEs (decision 5).
