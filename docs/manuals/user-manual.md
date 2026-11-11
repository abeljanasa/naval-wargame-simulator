# User manual

For players: the commanders and staff at a side's seat. The white cell's own tasks are in the [white-cell manual](white-cell-manual.md); scenario authors should also read the [scripting reference](scripting-reference.md).

> Naval Wargame Simulator, TTX release 1.0 (Phase 3). Unclassified open build: the shipped platform database is generic and has no real-world values.

## 1. Installing and starting

- **Windows:** run the `.msi` (or the setup `.exe`). **Linux:** run the `.AppImage`, or install the `.deb` or `.rpm`. **macOS:** open the `.dmg` and drag the app to Applications.
- **No network needed.** The map, coastline, platform database and scenarios are built in. The app makes no network connection, so it runs on an air-gapped machine.
- **First start:** the built-in demonstrator scenario loads, paused, with this window at the first side's seat.

Your workstation needs:
- Windows 10/11, macOS 12 or later, or a recent Linux (glibc 2.35 or later);
- 8 GB RAM;
- a GPU with WebGL 2 (any integrated GPU since about 2016).

## 2. The screen

| Area | What it shows |
|---|---|
| **Header** | Scenario name; game time; achieved speed (e.g. `60×`) or `paused`; sea state and weather; time controls; **REPLAY** while a replay is being reviewed; the light/dark theme switch |
| **Side panel** | Scenario, Seat, Units, Contacts, the selected unit, Messages, the platform database; for the white cell, the white-cell console and exercise review |
| **Map** | Your units, contacts with their 95 % error ellipses, ESM bearing lines, weapons in flight, minefields, reference points, and your side's planning layer (zones and facilities) |

**Map symbols.** Frames follow APP-6 conventions by domain: a full frame for surface, the upper half for air, the lower half for subsurface. Colour and shape show affiliation. A short line ahead of each unit shows where it will be in 6 minutes. Sense zones are blue outlines, engage zones red. Facilities are labelled `CP` (command post), `PORT`, `ANCH` (anchorage) or `RAS` (refuelling point).

## 3. Your seat

- A **seat** is a player position: one side's commander, or a white-cell seat.
- A window opened for you by the white cell is **locked** to your seat. It shows only your side's picture, and its time controls are off: the white cell runs the clock.
- At the main window, choose a seat in **Seat**.

## 4. Time

| Control | Key |
|---|---|
| Run / pause | **Space** |
| Compression 1×, 2×, 5×, 15×, 30×, 60×, Max | **1**–**7** |
| Step one second (while paused) | **.** |

Only the white cell (and the main window) controls time. In a seat window the buttons are greyed out.

## 5. Your units

- **Units** lists your side's units. Click one, in the list or on the map, to select it.
- The unit panel shows its class, position, heading, speed, route, mission, doctrine, stores and damage.
- **Esc** clears the selection.

### Orders

| Order | How |
|---|---|
| **Route** | **Plan new route…**, click the map for each waypoint (**Backspace** removes the last), then **Enter** to send or **Esc** to cancel. Ships and submarines plan round the coastline. |
| **Speed** | **Ordered speed (kn)**, then **Set**. |
| **Emissions** | **Emissions**: active, or silent (radar and jammer off). Silence hides you from ESM but blinds your radar. |
| **Weapons control** | **Weapons**: safe, tight (engage only hostile contacts), free (engage anything not friendly or neutral). Inherit uses the group's or side's setting. |
| **Engage** | Choose a **Target** contact, a **Weapon** and **Rounds**, then **Fire**. |
| **Mission** | **Patrol here** (a 30 km patrol round the unit); **Clear** ends the mission. |

**Refused orders.** The engine checks every order when it carries it out. A refusal appears in red with the reason, for example:
- *"weapons control status Tight forbids engaging C3"*;
- *"C3 at 182.0 km is outside WPN-ASM's 5.0–150.0 km"*;
- *"U2 is out of range of every command post"*.

## 6. Contacts

- **Contacts** lists what your side believes is out there: contact number, classification (from "surface contact" up to the class), and how long ago it was last seen.
- Your knowledge is **uncertain**. The ellipse on the map holds the true position 95 % of the time, and grows while nothing is seeing the contact.
- ESM bearing lines show emitters your ships have intercepted. Bearings from two ships can be crossed into a fix.
- A contact not seen for a while is dropped.

## 7. Weapons, damage and the planning layer

- Missiles fly to the target's estimated position; their seeker must acquire it. The enemy may shoot them down or decoy them.
- Hits cause damage, fires and flooding. A unit can be **mission-killed** (afloat but unable to fight) or destroyed.
- **Zones and facilities** (`docs/models/planning-layer.md`):
  - **Sense zone:** your units inside it follow its emissions and weapons rules, and you are alerted (in Messages) when your side gains a contact inside it.
  - **Engage zone:** targets inside it are engaged under its rules, for example weapons free against unfriendly contacts in a choke point.
  - **Command posts:** your orders reach only units within range of a command post that is up. If the post is lost with its ship, an alternate takes over after a delay. Orders in the gap are refused, or queued until then.
  - **Ports and anchorages:** a unit stopped there (at ≤ 1 kn) is repaired and, after 30 minutes, rearmed, if the facility offers it.

## 8. Messages

- **Messages** shows messages to your side: exercise control, injects (with a title), event messages and sense-zone alerts.
- The newest are at the bottom. The last 100 are kept.

## 9. The platform database

- **Platform database** opens a read-only browser. Search by id, name or category.
- Each record shows its figures, its source citation, its confidence and its classification marking.
- The shipped database is generic: its figures are illustrative, not real.

## 10. After the exercise

The white cell runs the replay and the after-action review (AAR). At a side seat during a review, your window follows the replay, showing your side's picture at the scrubber position, and orders are off until the white cell returns to the live exercise.

## 11. Trouble

| Problem | What to do |
|---|---|
| The map is blank | Check the GPU supports WebGL 2; update its driver. |
| "Cannot connect to the simulation engine" | Restart the app. |
| Time does not move | The white cell has paused it, or your window does not control time. |
| An order seems ignored | Look for a red refusal under the panel: the reason is given. |
| A record will not open | The app opens only `.nwsrec` records from this build or older. |
