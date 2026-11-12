# TTX pilot report (template)

Fill this in within a week of the pilot ([pilot plan](ttx-pilot-plan.md) §9). It is the evidence for the last Gate 3 criterion, and it feeds the [panel sign-off](sign-off.md). The report and its attachments stay with the client.

## 1. The pilot

| | |
|---|---|
| Date and venue | |
| Release build | Version, installer names and SHA-256 sums (from `SHA256SUMS-<os>.txt`) |
| Signed? | Yes / no. If no, the panel's agreement to a supervised unsigned run |
| Machines | OS and version of each exercise machine and of the spare |
| Scenario | `phase3-pilot.json` (*Kent passage pilot*), or another, and why |
| Participants | Roles and numbers (no names in this report if the client's rules forbid them) |

## 2. Results against the success criteria (pilot plan §7)

| # | Criterion | Met? | Evidence |
|---|---|---|---|
| 1 | Installs and runs with no network | | Installation-check sheets, one per machine |
| 2 | STARTEX to ENDEX with no crash and no restart | | Observer's log |
| 3 | The white cell sends injects, makes at least one override, and controls time for every seat without help | | Override log in the record; observer's log |
| 4 | The hot wash uses the replay and at least two AAR tables | | Which tables; screenshots or the CSV files |
| 5 | The record replays as verified on a second machine | | *"Replay: matches the recorded run"* on the spare, or `nws-batch aar` printing `"verified": true` |
| 6 | No issue rated critical is still open | | Issue list below |

**Overall:** pass / fail.

## 3. What happened

A short narrative of the exercise:
- the decisions taken at each decision point of the run sheet;
- where the white cell adjudicated;
- the time each phase took against the plan.

## 4. Issues

From the observer's log. Severity:
- **C:** the exercise could not continue, or the result was wrong and misled the players.
- **M:** a workaround, or more than 5 minutes lost.
- **m:** minor.

| # | Game time | Seat | What happened | Expected | Sev | Type (B/U/R/D) | Triage: fix before 1.0 / Phase 4 / won't fix |
|---|---|---|---|---|---|---|---|
| 1 | | | | | | | |

## 5. Usability (questionnaire)

| Role | Respondents | SUS mean | SUS range |
|---|---|---|---|
| White cell | | | |
| Blue | | | |
| Red | | | |
| All | | | |

**Scoring:**
1. For odd items, the item score is the answer − 1; for even items, it is 5 − the answer.
2. Add the ten item scores and multiply by 2.5.

**Open questions:** the main themes, with representative quotes.

## 6. Recommendation to the panel

- Accept Gate 3: yes / no / with conditions.
- **Fixes before release 1.0:** the issues triaged "fix before 1.0".
- **Into Phase 4:** the requests that belong to the [Phase 4 plan](../plan/phase-4.md).

## Attachments

- The exercise record (`.nwsrec`).
- The AAR CSV files (`nws-batch aar <record> --out <folder>`).
- The installation-check sheets.
- The observer's log.
- The questionnaires.
