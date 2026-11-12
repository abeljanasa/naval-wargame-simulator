# TTX pilot plan (Gate 3)

The last Gate 3 criterion: **a navy staff runs a TTX on the release build** (master prompt §16). This plan is a template. Dates, names and the venue are for the client to fill in.

## 1. Aim and objectives

**Aim:** show that a navy staff can plan, run and debrief a tabletop exercise with the release build, without developer help in the room.

**Objectives:**
1. Install the signed release on the exercise machines, air-gapped.
2. Run a multi-seat exercise: a white cell, Blue and Red seats, injects and at least one override.
3. Use the planning layer: command posts and comms, a choke point with sense and engage zones, a surface action group, fuel and an oiler, mines and mine clearing, an anchorage.
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

- **Machine:** one Windows or macOS machine meeting the [user manual](../manuals/user-manual.md) requirements, with three screens (or two and a projector). A second machine as a cold spare, with the same installer.
- **No network connection.** Install from removable media with the release's SHA-256 sums, following the [installation check](pilot-kit/installation-check.md). The scenarios travel with the installers, in `release-extra/scenarios/`.
- **Handouts:** the [pilot kit](pilot-kit/README.md): seat cards, the white-cell run sheet, the observer's log and the questionnaire.
- **Shipped content only:** the open generic platform database and the shipped scenarios, or a client scenario built with the editor on the day before.

## 4. Scenario

Primary: `data/scenarios/phase3-pilot.json`, *Kent passage pilot*. It uses the whole planning layer in one exercise:
- comms with jamming, a relay and a handover;
- fuel, with an oiler;
- minelaying and mine clearing;
- barrier-patrol, denial and cargo missions;
- an anchorage;
- dusk;
- a gale.

It runs 4 hours of game time, about 75 minutes of play at 4–6× with pauses. Its seats are **White cell**, **Blue command** and **Red command**.

The times below are from the reference run (seed 20261113, no player orders); `pilot_scenario_reaches_every_decision_point` checks that every decision point still happens, in this order. Player orders and white-cell rulings move them.

| Game time | What happens | White-cell action |
|---|---|---|
| 00:00 | STARTEX. Postures unfriendly, weapons tight outside the engage zone. The patrol vessel *Warden* is below bingo fuel and diverts to the oiler | Brief; open the seat windows; point Blue at the fuel state in the unit panel |
| 00:10 | Inject *Minelayer sighted* (automatic) | Watch whether Blue protects or redirects the MCM vessel *Sieve* |
| ~00:15–00:35 | The Red corvettes enter the Kent passage (logged); sense-zone alerts | Pause; Blue reports |
| 00:49 | The supply ship *Tally* loads 800 t at Refuge Cove | — |
| ~00:55 | Dusk (sunset): optical sensors lose range; night shading on the map | — |
| 01:10 | The Red landing ship *Ember* lays 12 mines on the eastern approach, where *Sieve* is sweeping | Ask Blue what it knows: the field is not in Blue's picture |
| ~01:30 | Red inside the engage zone | Send *Jamming warning* (i02). Decide hit or miss on a contested missile (**Hit** or **Miss**, with a reason) |
| ~01:45 | The flagship net is jammed by *Cinder*: command post down; the maritime HQ takes over after 5 minutes; Blue orders in the gap are queued | Show **Command posts** in the console; ask Blue how it would restore the net |
| ~01:50–02:10 | Both Red corvettes sunk in the engage zone; the flagship net comes back | Optional: *Media report* (i04) |
| 02:20 | *Warden* refuelled; back on its denial patrol | — |
| 02:30 | Inject *Gale warning* (automatic) | — |
| 02:40 | Sea state 5: *Sieve* (under 60 m) runs for shelter at Refuge Cove, through the approach Red has mined | Decision point: let it go, hold it with a new order, or rule on a mine strike (in the reference run it strikes a mine at 02:46) |
| 03:07 | *Tally* delivers its cargo, then waits: the anchorage is closed above sea state 4 | — |
| 03:20 | Gale eases; *Tally* goes to the staging anchorage | — |
| any | Optional: the flagship is lost (**Destroy**, reason "inject: mine strike") | Watch command pass to the maritime HQ after 5 minutes |
| 04:00 | ENDEX | **Save record**; replay and AAR |

Back-ups:
- `data/scenarios/phase3-planning-demo.json` (*Choke point planning*): the same passage, smaller, with no fuel, comms jamming or weather. Use it if the staff has less than an hour.
- `data/scenarios/phase3-ttx-demo.json`: an escalation exercise with scripted events and injects.

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
| Problems and requests | [Observer's log](pilot-kit/observer-log.md): time, seat, what happened, what was expected, severity |
| Usability | [Questionnaire](pilot-kit/questionnaire.md): SUS (10 items) plus 5 open questions |

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
| A staff decision needs something the build does not model (air operations, bathymetry) | White-cell adjudication covers it (move, repair, set stores), with a reason. Record each case as a requirement. |
| Crash during play | Save a record at each pause: **Save record** keeps the exercise so far. Reopen it on the spare to review; for a restart, reload the scenario and re-issue orders from the record's timeline. |

## 9. After the pilot

- Write the pilot report: results against §7, the questionnaire, the issues.
- Triage the issues with the client into "fix before release 1.0" and Phase 4.
- Tag release 1.0 once the panel accepts Gate 3.
