# Gate 4 review pack: historical replays (draft)

Gate 4 (master prompt §12, §13; [Phase 4 plan](../plan/phase-4.md) §1): **SME-built scenarios of historical engagements are run and compared against the recorded outcomes, and the differences are documented.**

> **Status (2026-11-13): drafts for SME review.** The two engagements below were built by the development team from public accounts, as the client asked, so its SMEs can correct them. Nothing here is confirmed. The reports show the first findings: two model limitations that the SMEs and the client should decide on.

## The engagements

| Engagement | Spec and scenario | Report (200 runs) |
|---|---|---|
| *Hangor* and *Khukri*, 9 December 1971 (submarine / ASW) | [data/validation/hangor-khukri-1971](../../data/validation/hangor-khukri-1971/validation.json) | [report](validation/hangor-khukri-1971/report.md) |
| Operation Praying Mantis, 18 April 1988 (surface missile, ship actions) | [data/validation/praying-mantis-1988](../../data/validation/praying-mantis-1988/validation.json) | [report](validation/praying-mantis-1988/report.md) |

Method: [docs/models/validation.md](../models/validation.md). Rerun with `nws-batch validate <spec> --out <dir>`.

## Findings so far

### 1. A lone submarine cannot attack on passive sonar (model limitation)

In every *Hangor* run the boat holds passive bearings on both frigates (signal excess near +96 dB, because the frigates are pinging) but never forms a track, so it never fires. A track from passive bearings needs bearings from **two observers in the same scan** (the ESM and sonar cross-fix, [esm.md](../models/esm.md)). The model has **no single-platform target motion analysis (TMA)**: one platform's bearings over time do not make a position. Submarines in the model therefore attack only on active sonar, radar or periscope, or with a second listener.

*Hangor* attacked on her own passive sonar plot, so the record (Khukri sunk, three torpedoes, one hit) is inconsistent with the model. Recommended: a bearings-only TMA for a single observer, then re-run.

### 2. Guns engage aircraft at their surface range (model and data limitation)

In the Praying Mantis runs, *Sahand*'s gun (the generic 127 mm, `WPN-GUN-127`, 24 km, surface, land and air) shoots down the A-6Es at stand-off ranges. A weapon has **one maximum range for every target domain**, and a medium-calibre gun's anti-aircraft reach is far shorter than its surface range. As a result:

- US losses (record 0, model mean 2.9);
- Iranian hits (record 0, model 3.1);
- *Sahand* is rarely sunk (record yes, model 3 %) and *Sabalan* rarely disabled (record yes, model 8 %), because the A-6Es carrying the Harpoons, Skippers and bombs are lost first.

Recommended: a range per target domain on weapons (database and model), then re-run.

### Consistent so far

- *Joshan* is sunk by SAG Charlie in every run.
- US missile expenditure is close to the record (mean 12.8 against 12).
- *Sabalan* is almost never sunk (record: crippled, not sunk).
- *Kirpan* and *Hangor* survive.

## Definition of done (Gate 4)

| Criterion | Evidence | Status |
|---|---|---|
| Monte Carlo statistics for scenario MOEs, near-linear with cores | `nws-batch mc` ([Phase 4 plan, slice 1](../plan/phase-4.md)) | Done (slice 1) |
| Two historical engagements, chosen with the SMEs, run as scenarios | The table above | **Drafts.** The client's SMEs confirm or correct the scenarios, records and assumptions |
| Each with its outcome distribution compared with the record | The reports | Done for the drafts |
| Every difference explained | Each report's "Differences to explain" | **Open.** The findings above are the development team's first reading; the SMEs explain each difference as a model limitation, data gap or historical uncertainty |
| Regional database covering the replays' and the PAR's order of battle, with citations and confidence | [Data dictionary](../db/data-dictionary.md#regional-database-phase-4-slice-13), [DL-0048](../design-log/DL-0048-regional-database.md) | **Two tranches:** Philippine Navy and Coast Guard, PLAN (Southern Theater) and China Coast Guard, and the historical platforms; then the Vietnam People's Navy and Coast Guard, US Navy and Coast Guard, and JMSDF and Japan Coast Guard (88 platforms in all). Other navies next, as the client prioritises |
| Terrain, bathymetry and imagery data offline | Slices 2–6 | Done; imagery waits on the client's tile pack |
| Duel tests extended to the new models | [Duel-test index](../gate2/duel-tests.md#phase-4-models) | Done |
| All CI checks green with the data packs loaded | PR #4 checks | At the gate |

## What the client decides

1. **The SMEs review the two drafts:** the positions, timings, forces, simplifications and the recorded numbers (each report lists its assumptions and sources).
2. **Whether to fix the two limitations before the gate** (single-platform TMA, ranges per target domain), or to record them as known limitations.
3. **The measures of effectiveness** (decision 5): the engagement outcomes used here are provisional.
4. **The next database tranche** (after Vietnam, the United States and Japan): which other navies the PAR scenarios need.
