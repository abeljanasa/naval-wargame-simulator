# Model validation against historical engagements

- **Phase:** 4, slice 14 ([DL-0049](../design-log/DL-0049-model-validation.md)).
- **Code:** `apps/batch/src/validate.rs` (`nws-batch validate`), on the Monte Carlo runner (`apps/batch/src/mc.rs`).
- **Data:** `data/validation/<engagement>/` holds a scenario and a validation spec.
- **Reports:** `docs/gate4/validation/<engagement>/`.

Gate 4 asks for historical engagements run as scenarios: each has its outcome distribution over N replications compared with the record, and every difference explained ([Phase 4 plan](../plan/phase-4.md) §1).

## The validation spec

`validation.json` (format `nws-validation`):

| Field | Meaning |
|---|---|
| `name`, `summary` | The engagement, and what happened |
| `scenario` | The scenario file, relative to the spec |
| `runs` | Replications (default 200); seeds run from the scenario's seed upward |
| `status` | "DRAFT for SME review" until the client's SMEs confirm it |
| `sources` | Open sources for the record, each with a URL or published reference |
| `assumptions` | What the scenario assumes where the record is silent, and what it leaves out |
| `measures` | What to compare, each with the record |

Measures (the engagement-outcome MOEs the client chose on 2026-11-13; provisional):

| Kind | Fields | Value in a run |
|---|---|---|
| `unit_lost` | `unit` | yes if the unit was destroyed |
| `unit_survives` | `unit` | yes if it was not |
| `unit_disabled` | `unit` | yes if destroyed or mission-killed at the end |
| `losses` | `side` | units of the side destroyed |
| `weapons_fired` | `side`, `weapons` (optional) | weapons launched by the side, only of the listed kinds if given |
| `hits` | `side` | hits by the side's weapons |
| `first_hit_s` | `side` | game seconds to the side's first hit; absent in runs with none |

A yes/no measure's record is `true` or `false`; the others' is a number. A spec with the wrong type of record is refused.

## Comparison

- **Yes/no measures:** the model's probability of the recorded outcome, P(record) = k/n, with a 95 % Wilson score interval. The record is
  - **consistent** with the model when P(record) ≥ 10 %;
  - **unlikely** when 2 % ≤ P(record) < 10 %;
  - **inconsistent** when P(record) < 2 %.
- **Counts and times:** the model's mean and its 5th–95th percentile range (nearest rank) over the runs in which the measure occurred. The record is **consistent** inside the range and **outside** it otherwise; a measure that never occurred is reported as such.

These thresholds are original design. They say how surprising the record would be if the model were right; history is one draw, so a recorded outcome the model gives 20 % is not a failure. They are not a pass mark: **every** difference is listed for the SMEs to explain as a *model limitation*, a *data gap* or *historical uncertainty*.

### Hand values

| Test | Hand value |
|---|---|
| `wilson_interval_by_hand` | 8 of 10: centre (0.8 + 0.19208)/1.38416 = 0.71675, half-width 0.22658: 0.4902–0.9433 |
| `yes_no_verdicts_by_the_probability_of_the_record` | 30/100 consistent, 5/100 unlikely, 1/100 inconsistent |
| `counts_compare_with_the_five_to_ninety_five_range` | 1…20: mean 10.5, 5 % = 1 (rank 1), 95 % = 19 (rank 19) |

## Outputs

`nws-batch validate data/validation/<engagement>/validation.json --out <dir> [--runs N] [--seed S] [--threads T] [--db file]` writes:

- `report.md`: the status banner, the comparison table, the differences to explain (with their notes), the assumptions and the sources;
- `validation.json`: the same, for tools;
- `runs.csv`: every run's value of every measure, by seed.

Runs are independent and deterministic, so the reports are reproducible exactly from the seed range they print.

## Limits

- The measures are counted from events; a weapon's target is not checked, so `weapons_fired` counts all of a kind.
- `unit_disabled` reads the status at the end of the run.
- Thresholds and measures are provisional until the client confirms its MOEs (Phase 4 decision 5).
