# TTX pilot plan (Gate 3)

The last Gate 3 criterion: **a navy staff runs a TTX on the release build** (master prompt §16). This plan is a template. Dates, names and the venue are for the client to fill in.

## 1. Aim and objectives

**Aim:** show that a navy staff can plan, run and debrief a tabletop exercise with the release build, without developer help in the room.

**Objectives:**
1. Install the signed release on the exercise machines, air-gapped.
2. Run a multi-seat exercise: a white cell, Blue and Red seats, injects and at least one override.
3. Use the planning layer: command posts, a choke point with sense and engage zones, a surface action group.
4. Debrief from the replay and AAR tables, and keep the exercise record.
5. Collect what the staff needs changed before release 1.0 is accepted.

## 2. Participants

| Role | Number | Seat |
|---|---|---|
| Exercise director (white cell lead) | 1 | Main window (control station) |
| Assistant umpire | 0–1 | White-cell seat window |
| Blue commander and staff | 2–4 | Blue seat window |
| Red commander | 1–2 | Red seat window |
| Observer (developer, notes only) | 1 | — |
| Client representative (Gate 3 panel) | 1 | — |

## 3. Equipment

- **Machine:** one Windows, macOS or Linux machine meeting the [user manual](../manuals/user-manual.md) requirements, with three screens (or two and a projector). A second machine as a cold spare, with the same installer.
- **No network connection.** Install from removable media with the release's SHA-256 sums.
- **Shipped content only:** the open generic platform database and the shipped scenarios, or a client scenario built with the editor on the day before.

## 4. Scenario

Primary: `data/scenarios/phase3-planning-demo.json`, *Choke point planning*. It runs 3 hours of game time, about 60 minutes of play at 3–5× with pauses.

| Game time | Decision point | White-cell action |
|---|---|---|
| 00:00 | STARTEX, postures unfriendly, weapons tight outside the engage zone | Brief; open seat windows |
| ~00:15 | Red enters the choke point (logged transit) | Inject: intelligence report |
| ~00:30 | Blue sense-zone alert on the first Red corvette | Pause; Blue reports |
| ~01:00 | Second alert; Red closes the engage zone | Optional: close the engage zone (**Zone rules**, weapons safe) and ask Blue for a decision |
| ~01:25 | First engagements in the engage zone | Decide hit or miss on a contested missile (**Hit** or **Miss**, with a reason) |
| ~01:50 | The corvette in the passage fires as the zone's assigned shooter | — |
| any | Optional: the flagship is lost (**Destroy**, reason "inject: mine strike") | Watch command pass to the maritime HQ after 5 minutes; Blue orders in the gap are queued |
| end | ENDEX | **Save record**; replay and AAR |

Back-up: `data/scenarios/phase3-ttx-demo.json` (escalation with scripted events and injects).

## 5. Timeline (one day)

| Time | Activity |
|---|---|
| 08:30–09:30 | Installation from media; check air-gapped (network adapter disabled) |
| 09:30–10:15 | Familiarisation: user manual sections 2–7 at each seat |
| 10:15–10:30 | White-cell rehearsal: start, pause, inject, override, replay |
| 10:30–12:00 | **Exercise run** |
| 12:00–13:00 | Lunch; white cell prepares the AAR (tables, key moments) |
| 13:00–14:00 | **Hot wash** from the replay and AAR tables |
| 14:00–15:00 | Optional: the staff builds or changes a scenario in the editor and runs it for 15 minutes |
| 15:00–15:30 | Questionnaire and close |

## 6. Data collected

| Data | How |
|---|---|
| The exercise record (`.nwsrec`) | **Save record** at ENDEX |
| AAR tables | **Save CSV**, or `nws-batch aar <record> --out <folder>` |
| Replay check | `nws-batch aar` prints `"verified": true`; the app shows *"Replay: matches the recorded run"* |
| Problems and requests | Observer's log: time, seat, what happened, what was expected |
| Usability | Questionnaire: SUS (10 items) plus 5 open questions on the white-cell tasks |

All records stay with the client. They are unclassified, because the build has no classified data, but the client's handling rules apply to what the staff types.

## 7. Success criteria

The pilot passes Gate 3 if:

1. The release installs and runs on the exercise machines with no network.
2. The exercise runs from STARTEX to ENDEX without a crash or a restart.
3. The white cell sends injects, makes at least one override, and controls time for every seat without help.
4. The hot wash uses the replay and at least two AAR tables.
5. The record replays as verified on a second machine.
6. No issue rated **critical** by the staff remains open: "the exercise could not continue", or "the result was wrong and misled the players".

Non-critical issues go to the backlog with the Phase 4 plan.

## 8. Risks

| Risk | Mitigation |
|---|---|
| Installers unsigned (no certificates yet) | Pilot with signed builds only. If the panel agrees, a supervised unsigned run is an option, recorded as such. |
| Exercise machine below specification | Check against the user manual a week before; bring the spare. |
| Staff unfamiliar with the controls | Familiarisation session; the user manual at each seat; keyboard shortcuts on one page. |
| A P1 gap blocks a staff decision (fuel, comms) | White-cell adjudication covers it (move, repair, set stores), with a reason. Record each case as a requirement. |
| Crash during play | Save a record at each pause: **Save record** keeps the exercise so far. Reopen it on the spare to review; for a restart, reload the scenario and re-issue orders from the record's timeline. |

## 9. After the pilot

- Write the pilot report: results against §7, the questionnaire, the issues.
- Triage the issues with the client into "fix before release 1.0" and Phase 4.
- Tag release 1.0 once the panel accepts Gate 3.
