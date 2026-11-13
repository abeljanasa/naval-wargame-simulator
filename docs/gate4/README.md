# Gate 4 review pack: historical replays (draft)

Gate 4 (master prompt §12, §13; [Phase 4 plan](../plan/phase-4.md) §1): **SME-built scenarios of historical engagements are run and compared against the recorded outcomes, and the differences are documented.**

> **Status (2026-11-13): drafts for SME review.** The two engagements below were built by the development team from public accounts, as the client asked, so its SMEs can correct them. Nothing here is confirmed. The first runs found two model limitations; both are now fixed and the reports are regenerated. The remaining differences are listed below for the SMEs to explain.

## The engagements

| Engagement | Spec and scenario | Report (200 runs) |
|---|---|---|
| *Hangor* and *Khukri*, 9 December 1971 (submarine / ASW) | [data/validation/hangor-khukri-1971](../../data/validation/hangor-khukri-1971/validation.json) | [report](validation/hangor-khukri-1971/report.md) |
| Operation Praying Mantis, 18 April 1988 (surface missile, ship actions) | [data/validation/praying-mantis-1988](../../data/validation/praying-mantis-1988/validation.json) | [report](validation/praying-mantis-1988/report.md) |

Method: [docs/models/validation.md](../models/validation.md). Rerun with `nws-batch validate <spec> --out <dir>`.

## Findings

### Fixed: a lone submarine could not attack on passive sonar

In the first *Hangor* runs, the boat held passive bearings on both frigates but never formed a track, so it never fired. A track from bearings needed two observers in the same scan; one platform's bearings over time made no position.

**Fixed (2026-11-13, [DL-0050](../design-log/DL-0050-single-observer-tma.md), [tma.md](../models/tma.md)):** single-observer target motion analysis. A maximum-likelihood fit of one observer's bearings over up to 15 minutes ranges the target once the observer has manoeuvred.

| Measure (200 runs) | Record | Before | After |
|---|---|---|---|
| Khukri sunk | yes | 0 % | 77 % |
| Indian ships lost | 1 | 0 | mean 1.6 |
| Hits by Pakistan | 1 | 0 | mean 3.4 |

### Fixed: guns engaged aircraft at their surface range

In the first Praying Mantis runs, *Sahand*'s 127 mm gun (24 km) shot down the A-6Es at their stand-off stations: one maximum range served every target domain.

**Fixed (2026-11-13, [DL-0051](../design-log/DL-0051-weapon-target-ranges.md)):** a range per target domain. The generic 127 mm-class gun now reaches aircraft at 12 km, the 76 mm-class at 8 km. These are generic estimates for SME review.

| Measure (200 runs) | Record | Before | After |
|---|---|---|---|
| Sahand sunk | yes | 3 % | 29 % |
| Sabalan disabled | yes | 8 % | 60 % |
| US losses to Iranian fire | 0 | mean 2.9 | mean 1.2 |
| Hits by Iran | 0 | mean 3.1 | mean 1.5 |

The A-6Es still lost are lost later in the action, when they close inside the frigates' short-range weapons to drop bombs.

### Differences still to explain

- **Hangor fires more torpedoes than the record:** mean 7.1 against 3. The model keeps firing at a track until it is lost, and the scenario models no torpedo failures; the first torpedo of the record is reported to have failed.
- **Hangor's first hit comes later than the estimate:** mean 1,238 s against an estimated 600 s. The model needs several minutes of bearings across a turn before it ranges. The record's time is itself an estimate for the SMEs.
- **Kirpan survives less often than the record:** 21 %. In the model, Hangor keeps attacking the second frigate; historically Kirpan withdrew. Consistent at the 10 % threshold, but worth the SMEs' view.
- **Iran fires more missiles than the record:** 3.9 against 2. Every Iranian launcher with a target in range fires.

### Consistent

- **Praying Mantis:** *Joshan* is sunk by SAG Charlie in every run, and US missile expenditure is close to the record (mean 14.2 against 12). Sabalan disabled, Sabalan not sunk, US losses and Iranian hits are consistent.
- **Hangor–Khukri:** Khukri is sunk, *Hangor* survives every run, and Indian losses and Pakistani hits are consistent.

## Definition of done (Gate 4)

| Criterion | Evidence | Status |
|---|---|---|
| Monte Carlo statistics for scenario MOEs, near-linear with cores | `nws-batch mc` ([Phase 4 plan, slice 1](../plan/phase-4.md)) | Done (slice 1) |
| Two historical engagements, chosen with the SMEs, run as scenarios | The table above | **Drafts.** The client's SMEs confirm or correct the scenarios, records and assumptions |
| Each with its outcome distribution compared with the record | The reports | Done for the drafts |
| Every difference explained | Each report's "Differences to explain" | **Open.** The findings above are the development team's first reading; the SMEs explain each difference as a model limitation, data gap or historical uncertainty |
| Regional database covering the replays' and the PAR's order of battle, with citations and confidence | [Data dictionary](../db/data-dictionary.md#regional-database-phase-4-slice-13), [DL-0048](../design-log/DL-0048-regional-database.md) | **Three tranches:** Philippine Navy and Coast Guard, PLAN (Southern Theater) and China Coast Guard, and the historical platforms; the Vietnam People's Navy and Coast Guard, US Navy and Coast Guard, and JMSDF and Japan Coast Guard; the navies of Malaysia, Indonesia, Australia, South Korea and Taiwan with the Malaysian, Indonesian and Taiwanese coast guards (113 platforms in all) |
| Terrain, bathymetry and imagery data offline | Slices 2–6 | Done; imagery waits on the client's tile pack |
| Duel tests extended to the new models | [Duel-test index](../gate2/duel-tests.md#phase-4-models) | Done |
| All CI checks green with the data packs loaded | PR #4 checks | At the gate |

## What the client decides

1. **The SMEs review the two drafts:** the positions, timings, forces, simplifications and the recorded numbers (each report lists its assumptions and sources).
2. **The two fixes:** whether their assumptions stand. These are the TMA thresholds and the generic gun anti-air ranges, 12 km and 8 km.
3. **The measures of effectiveness** (decision 5): the engagement outcomes used here are provisional.
4. **Any further database tranche:** which other navies or agencies the PAR scenarios need (for example Singapore, Thailand, Brunei).
