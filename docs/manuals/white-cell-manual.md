# White-cell manual

For the facilitator and umpires who run a tabletop exercise (TTX). Read the [user manual](user-manual.md) first; this manual covers what only the white cell does.

## 1. Before the exercise

### Room and seats

A typical layout is one machine with several screens, or a projector:

- **Main window:** the control station, on the facilitator's screen. It can sit at any seat and always runs the clock.
- **Seat windows:** one per side's commander. Open them from the white-cell console (§3). Each is locked to its seat and shows only that seat's picture.

The default seats are **White cell**, plus one seat per side, named after it. Add seats as needed: a second white-cell seat for an assistant umpire, or a Blue XO seat on its own screen.

### Scenario

- Load a scenario file under **Scenario**, or build one in the scenario editor (§7).
- The shipped demonstrators are in `data/scenarios/`:
  - `phase3-pilot.json`: the Gate 3 pilot exercise. A choke point under jamming, fuel and an oiler, a minefield and a mine countermeasures vessel, cargo, an anchorage, dusk and a gale. Its decision points are in the [pilot plan](../gate3/ttx-pilot-plan.md) §4;
  - `phase3-ttx-demo.json`: an escalation exercise with events, injects and a scripted umpire brief;
  - `phase3-planning-demo.json`: a choke point with sense and engage zones, a command post afloat with an alternate ashore, ports and a surface action group.
- Check the scenario's **Events** and **Injects** in the console before STARTEX.

### Rehearse

Run the scenario at **Max** once, alone, to see where the decision points fall. Then load it again. A scenario restarts from the beginning every time it is loaded.

## 2. Running the clock

- Only white-cell seats and the main window control time.
- Pause for decisions and briefings (**Space**). Step one second at a time (**.**) to walk through an engagement.
- Typical TTX speeds are **15×** to **60×** in transit, and **1×** to **5×** in contact.

## 3. The white-cell console

Choose the **White cell** seat (toolbar, **Seat**) to see ground truth: every side's units, with no fog of war. The console is the right panel's tabs (also on the **Exercise** menu):
- **Unit**: the selected unit, with **Override** (Repair, Destroy, Remove) and weapons in flight (Hit, Miss);
- **Injects**: the inject library and free messages;
- **Events**;
- **Planning**: command posts, facilities and zones in play;
- **Seats**;
- **Log**: the override log;
- **Review**: replay and AAR.

**Reason (logged)** stays at the top of every console tab.

### Overrides (adjudication)

Every override is logged with your seat name and the **Reason (logged)**. An override without a reason is refused.

| Override | How |
|---|---|
| Repair a unit | Select it, **Repair** (damage to zero, fires out, flooding stopped) |
| Destroy a unit | Select it, **Destroy** (counts as destroyed in the AAR, credited to the white cell) |
| Remove a unit | Select it, **Remove** (out of the exercise, not counted as destroyed) |
| Force a weapon's outcome | Under weapons in flight, **Hit** (full damage, no roll) or **Miss** |

The **Log** tab lists the most recent overrides with time, umpire and reason.

### Injects and messages

- **Injects:** the scenario's library of prepared messages, each with sender, audience, text and your note. **Send** sends one now; the count shows how often it has gone. Scheduled injects are sent by events.
- **Message to:** a free-text message from exercise control to all sides, one side, or the white cell.

### Events

- Every scenario event is listed with its state: on/off, and times fired.
- Untick to stop an event firing; **Fire** runs its actions now, whatever its trigger and conditions.

### Planning layer (live)

- **Facilities:** turn a facility's repair, rearm and refuel services on or off, or **Remove** it.
  - For example, a port is closed by strikes.
  - An alternate command post cannot be removed while another post depends on it; the console says why.
- **Add on the map:** choose sense or engage zone and a radius, then click the map for its centre.
- Every change is one logged order from your seat, so it replays exactly and appears in the AAR. The surface action groups in play are not touched.

### Zone rules (per-zone ROE override)

- Lists every zone of every side. During play you can:
  - change a zone's **weapons control** (safe, tight, free);
  - change its **emissions** (active or silent);
  - **Remove** it.
- Use it to open or close an engage zone at a decision point: *"Blue may now engage in the passage."*
- The change is a logged order from your seat, so it appears in the AAR.

### Command posts

- Lists every side's command posts, **up** or **down** with the reason: jammed, comms knocked out, or lost with its unit.

### Seats

- Lists every seat. **Open window** opens a window locked to a seat (move it to the commander's screen). **Remove** removes a seat and closes its windows.
- **New seat** adds a seat for a side, or another white-cell seat. Names must be unique.
- The main window's seat and the last white-cell seat cannot be removed.

## 4. Command posts and the planning layer

Scenarios with a planning layer add decision points you can exploit:

- **A lost flagship** takes its command post with it. Orders to its subordinates are refused (or queued) until the alternate takes over after the handover delay. Destroying the flagship (**Destroy**) is a legitimate inject.
- **Sense-zone alerts** reach the side's message log as contacts are gained in a zone. Watch the timeline for when each side first knew.
- **Engage zones with assigned shooters** fire on their own once contacts are inside under weapons free. Close the zone (weapons safe) to hold them.
- **Damaged AI units** divert to the nearest repair port and resume their mission when repaired.
- **Jamming** a command post (move or switch on an enemy jammer near it) takes it down. Its alternate takes over after the handover delay, and the post comes back when the jamming stops.
- **Fuel** (scenarios with the fuel model): AI units low on fuel head for the nearest refuelling point or oiler. Queues at a busy point show in the timeline (*waiting for a berth*). Closing a depot's refuel service is a strong logistics inject.
- **Weather:** raising the sea state (an event, or the scenario) sends small ships to shelter. Ships at an anchorage whose holding limit is exceeded leave it.
- **Action groups:** sinking a leader hands the group to its next ship, with the group's mission.
- **Coverage:** the left panel's **Coverage** tab shows any side's sense-zone coverage and gaps, for planning a decision point.

## 5. Exercise review and AAR

On the **Review** tab (or **Exercise › Review and AAR**), for the control station and white-cell seats:

| Action | What it does |
|---|---|
| **Replay** | Rebuilds the exercise so far and replays it from the start. The live exercise is paused and kept. Every window follows the replay, each for its own seat. Orders are off. |
| Scrubber | Drag to any moment. The time controls play the replay. |
| **Back to live** | Returns every window to the live exercise, still paused. |
| **Save record** | Saves the exercise (`.nwsrec`): the scenario and every order, override and inject. Keep it with the exercise papers. |
| Open a record | Opens a saved record for review on any machine with this build. |
| **AAR tables** | Summary by side, losses with cause, weapons expended against effective, detections, and the timeline. Click a row to jump the replay there. **Save CSV** exports the open table. |

**"Replay: matches the recorded run"** confirms the replay is exactly what happened. A red warning means the record was altered, or was made by a different build; treat that replay's details with care.

A hot wash usually goes:
1. **Summary:** what each side lost and what it used.
2. **Timeline,** filtered to `contact`: who saw whom, and when.
3. **Timeline,** filtered to `order` and `override`: the decisions.
4. **Replay**, scrubbed to the key moments, with each side's window showing what that side saw.

**After the exercise**, run `nws-batch aar <record> --out <folder>` to get all five tables as CSV files. It also checks the record replays exactly.

## 6. Running the TTX pilot

The Gate 3 pilot plan is [docs/gate3/ttx-pilot-plan.md](../gate3/ttx-pilot-plan.md). Print the [pilot kit](../gate3/pilot-kit/README.md) for the day. Its white-cell run sheet walks through the *Kent passage pilot* decision by decision.

## 7. Editing scenarios

**Edit this scenario**, **New scenario** or **Edit a scenario file** on the **File** menu opens the editor beside the map. The map then shows the draft, not the live exercise. The tabs:

- **Overview:** name, description, seed, duration, start time, sea state, weather, visibility.
- **Sides:** names and postures toward each other.
- **Units:** add on the map; platform, position, heading, speed, route, mission, reserve.
- **Mines:** minefields.
- **Points:** reference points.
- **Planning:**
  - facilities (command posts with alternates, ports, anchorages, refuelling points);
  - zones (**Add choke point on the map**), with their rules and shooters;
  - surface action groups.
- **Events:** events, injects, seats and groups as JSON, with templates ([scripting reference](scripting-reference.md)).

**Check** asks the engine to validate the draft. Errors (red) block **Save file** and **Play**; warnings (amber) do not. **Play** replaces the running exercise with the draft.

## 8. Good practice

- Give every override a reason a stranger would understand in the AAR.
- Save a record at the end of every exercise, and after any session you may want to review.
- Pause before injects that change the situation, so every side reads them at once.
- Keep the white-cell screen out of the players' sight: it shows ground truth.
