# Gate 3 sign-off: TTX release 1.0

The panel's go/no-go record (master prompt §13). Each line is checked against its evidence in the [Gate 3 pack](README.md). When every line is met, the panel signs. `phase-3/ttx-release` (PR #3) is then merged and release 1.0 tagged (pilot plan §9), and Phase 4 may start ([plan](../plan/phase-4.md)).

## Definition of done (§16)

| ☐ | Criterion | Evidence | Who closes it |
|---|---|---|---|
| ☐ | All P1 features in §7 work, with the client's Phase 4 deferrals (imagery, GEBCO/SRTM, air operations) accepted | Gate 3 pack, P1 table | Panel (accept the deferrals) |
| ☐ | The planning layer in §7.1 works | Gate 3 pack §7.1 table; the pilot scenario's decision-point test | Panel |
| ☐ | A white cell can run a multi-seat exercise with injects, overrides and AAR | White-cell manual; packaged-app rehearsal of the run sheet; the pilot | Panel, after the pilot |
| ☐ | Signed installers for Windows and macOS (ADR-0006) | A release workflow run with no `UNSIGNED` warning | Client provides the certificates as Actions secrets; developer runs the release |
| ☐ | The installers run air-gapped | Completed [installation checks](pilot-kit/installation-check.md) for Windows and macOS, on disconnected machines, with the signed build | Client's IT or the developer, on the client's machines |
| ☐ | All CI checks are green | PR #3 checks green on the release commit | GitHub billing restored; developer pushes and confirms |
| ☐ | Manuals are complete | User manual, white-cell manual, scripting reference; reviewed by the pilot staff | Panel |
| ☐ | A navy staff has run a TTX pilot on the release build | The [pilot report](pilot-report-template.md), with criteria 1–6 met | Client's staff; developer writes the report |

## Also for the panel

| ☐ | Item |
|---|---|
| ☐ | Phase 4 decisions confirmed ([ADR-0007](../adr/0007-phase-4-region-and-data.md)): region, imagery, Gate 4 engagements (subject-matter experts named), data packs |
| ☐ | Measures of effectiveness for Phase 4 named (Phase 4 plan, decision 5) |
| ☐ | Issues triaged "fix before 1.0" are fixed and re-checked |

## Decision

| | |
|---|---|
| Decision | Go / no-go / go with conditions |
| Conditions | |
| Date | |
| Panel (roles) | |
