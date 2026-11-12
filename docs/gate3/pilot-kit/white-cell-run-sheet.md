# White-cell run sheet: Kent passage pilot

Scenario `data/scenarios/phase3-pilot.json`: 4 hours of game time, about 75 minutes of play. The times are from the reference run. Player orders and your rulings move them, so watch the **Messages** and **Log** tabs rather than the clock.

## Before STARTEX (10:15)

- [ ] **File › Open scenario…** → `phase3-pilot.json`. The toolbar shows *Phase 3 — Kent passage pilot*, `D+0 00:00:01`, paused, with every unit on the map. The STARTEX message is already in **Messages**.
- [ ] **Seat** → *White cell*. Check the right panel shows Injects, Events, Planning, Seats, Log and Review.
- [ ] **Seats** tab: **Open window** for *Blue command* and *Red command*, and move each to its screen. Each shows only its own side.
- [ ] **Events** tab: eight events, all enabled.
- [ ] **Injects** tab: four injects (*Minelayer sighted*, *Jamming warning*, *Gale warning*, *Media report*).
- [ ] **Planning** tab: command posts *Blue flagship command* (up) and *Blue maritime HQ* (up).
- [ ] Type a first reason in **Reason (logged)**, e.g. `pilot: rehearsal`. Every override needs one.

## Run

| ☐ | Time | What happens | Your action |
|---|---|---|---|
| ☐ | 00:00 | STARTEX message. *Warden* (OPV) below bingo fuel, diverts to the oiler | Brief both sides. Run at **30×** (key **5**). Ask Blue to find *Warden*'s fuel state |
| ☐ | 00:10 | *Minelayer sighted* goes to Blue automatically | Note what Blue does with *Sieve* (MCMV) |
| ☐ | ~00:15–00:35 | Red corvettes enter the Kent passage; Blue sense-zone alerts | **Pause** (Space). Blue reports its picture |
| ☐ | ~00:55 | Dusk: night shading on the map; optical ranges drop | — |
| ☐ | 01:10 | *Ember* (Red) lays 12 mines on the eastern approach | Ask Blue what it knows. The field is not in Blue's picture |
| ☐ | ~01:30 | Red inside the engage zone (message to you) | Slow to **5×** (key **3**). **Injects** → send *Jamming warning*. Rule on a contested missile: **Unit** → weapons in flight → **Hit** or **Miss** |
| ☐ | ~01:45 | Flagship net jammed: *Blue flagship command* down | **Planning** tab: show the post down. After 5 minutes the maritime HQ takes over; Blue orders in the gap are queued. Ask Blue how it would restore the net |
| ☐ | ~01:50–02:10 | Red corvettes sunk; the flagship net comes back | Optional: send *Media report* |
| ☐ | 02:20 | *Warden* refuelled; back on patrol | — |
| ☐ | 02:30 | *Gale warning* goes to all automatically | — |
| ☐ | 02:40 | Sea state 5: *Sieve* runs for shelter through the mined approach | **Pause.** Decide: let it go, have Blue hold it, or rule a mine strike (**Destroy**, reason `inject: mine strike`) |
| ☐ | 03:07 | *Tally* delivers 800 t; waits for the gale to ease | — |
| ☐ | 03:20 | Gale eases; *Tally* goes to the anchorage | Run at **60×** (key **6**) to ENDEX |
| ☐ | 04:00 | ENDEX | **Pause. Review › Save record** to the exercise folder |

**Optional, any time:** lose the flagship (**Unit** → *Anvil* → **Destroy**, reason `inject: mine strike`). Command passes to the maritime HQ after 5 minutes.

## Hot wash (13:00)

- [ ] **Review › Replay**. The banner reads *"Replay: matches the recorded run"*.
- [ ] **AAR tables › Summary**: losses and weapons used by side.
- [ ] **Timeline** filtered to `contact`: who saw whom, and when.
- [ ] **Timeline** filtered to `order` and `override`: the decisions, with your reasons.
- [ ] Scrub the replay to the jamming (~01:45) and the gale (02:40), with the Blue window showing what Blue saw.
- [ ] **Save CSV** for the Summary and the Timeline.
- [ ] **Back to live** before closing.

## Keys

| Key | Does |
|---|---|
| **Space** | Run or pause |
| **1**–**7** | 1×, 2×, 5×, 15×, 30×, 60×, Max |
| **.** | Step one second (paused) |
