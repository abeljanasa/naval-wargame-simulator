# User manual

For players: the commanders and staff at a side's seat. The white cell's own tasks are in the [white-cell manual](white-cell-manual.md); scenario authors should also read the [scripting reference](scripting-reference.md).

> Naval Wargame Simulator, TTX release 1.0 (Phase 3). Unclassified open build: the shipped platform database is generic and has no real-world values.

## 1. Installing and starting

- **Windows:** run the `.msi` (or the setup `.exe`). **macOS:** open the `.dmg` and drag the app to Applications.
- **No network needed.** The map, coastline, platform database and scenarios are built in. The app makes no network connection, so it runs on an air-gapped machine.
- **First start:** the built-in demonstrator scenario loads, paused, with this window at the first side's seat.

Your workstation needs:
- Windows 10/11, or macOS 12 or later;
- 8 GB RAM;
- a GPU with WebGL 2 (any integrated GPU since about 2016).

## 2. The screen

| Area | What it shows |
|---|---|
| **Menu bar** | **File**: open a scenario; new, edit or edit-from-file in the scenario editor; install a terrain pack. **View**: night shading, dark theme, unit figures, depth shading, depth contours, relief, terrain packs, the left-panel tabs, the platform database. **Exercise**: run or pause, step, review and AAR; for the white cell, each console tab. **Help**: keyboard shortcuts. The scenario name is on the right |
| **Toolbar** | Run/pause, step and time compression; game time, achieved speed (e.g. `60×`) or `paused`, sea state and weather; **REPLAY** while a replay is being reviewed; your **Seat** |
| **Left panel** | Tabs: **Picture** (your units and contacts), **Messages** (with a count of unread messages), **Coverage** (when your side has sense zones) |
| **Right panel** | **Unit**: the selected unit (details, orders, engage). For the white cell, also **Injects**, **Events**, **Planning**, **Seats** and **Log**; for seats that run the clock, **Review**. A side seat sees this panel only while a unit is selected |
| **Status bar** | Refused orders and other errors, map-click prompts, and key hints; on the right, the depth or height under the cursor when a terrain pack covers it |
| **Map** | Your units, contacts with their 95 % error ellipses, ESM bearing lines, your weapons in flight, the enemy weapons your sensors track (rings: red missiles, blue torpedoes), minefields, reference points, your side's planning layer (zones and facilities, with their areas), sense coverage, and the night side of the earth |

**Map symbols.** Units are drawn as figures of what they are, seen from above and turned to their heading: ship and submarine hulls, aircraft outlines, and icons for land sites. They scale with the map, smaller as you zoom out. Each is coloured by affiliation, with a small affiliation badge beside its name: a circle for friend, a diamond for hostile, a square for neutral, a quatrefoil for unknown. Figures lie flat on the map, so they keep pointing along their heading when you rotate or tilt the view (right-drag). Missiles, torpedoes and other weapons in flight are drawn the same way. A contact shows a figure once your side has classified it. **View › Unit figures** switches to APP-6 frames. **Hits, misses and weapons shot down** show as a marker where they happened, and as a notice in the status bar: your own shots, and attacks on your units. With *Unit figures* off, frames follow APP-6 conventions by domain: a full frame for surface, the upper half for air, the lower half for subsurface. Colour and shape show affiliation. The code inside the frame shows the unit's type, such as `DDG`, `FFG`, `SSK`, `MV`, `F` (fighter) or `H` (helicopter). For a contact it appears once your side has classified it that far. The full list is in [symbology.md](../behaviour-specs/symbology.md). A short line ahead of each unit shows where it will be in 6 minutes. A dashed stub shows the start of a unit's route; select the unit to see all of it. ESM bearing lines are drawn 40 km long. Where labels would overlap, the less important one is hidden until you zoom in. The order, from most important, is: the selected unit, units, contacts, facilities, minefields, zones, reference points. Sense zones are blue outlines, engage zones red. Facilities are labelled `CP` (command post), `PORT`, `ANCH` (anchorage) or `RAS` (refuelling point).

**Terrain packs.** With a terrain pack installed, the map shades the sea by depth, pale inshore and dark offshore. It also draws depth contours, fewer as you zoom out, and hill shading over land. Each layer can be turned off in the **View** menu. The status bar shows the depth or height under the cursor. **View › Terrain packs…** lists the packs, what they cover, and whether this exercise runs with them. Depths are for planning, not for navigation.

- **Packs:** a world layer at 4′ (about 7.4 km) comes with the app, built from the current GEBCO grid for each release. A regional pack (the Philippine area at 15″, about 460 m) is installed separately. Where both cover, the regional pack is used. An older copy of the world layer installed by hand is set aside in favour of the one shipped with the app.
- **Installing one:** the white cell installs a pack with **File › Install terrain pack…**, choosing the `.nwsterrain` file **and** its `.sha256` file together.
  - The app checks the pack against the `.sha256` file before copying it in, and refuses a damaged or altered copy.
  - No network is used.
- **When a pack takes effect:** an exercise that has not started takes the pack at once; otherwise the next scenario you open does. A running exercise keeps the terrain it started with, so its record still replays.
- **Records and terrain:** a record made with terrain packs replays only on a machine with the same packs installed. The app names the packs it needs.

**Weather.** Weather zones show on the map as pale outlines, named with their sea state. Inside one, its weather applies: rough seas slow small ships and raise sea clutter, rain weakens radar, fog shortens the range of lookouts and cameras, and low cloud hides aircraft above it from the eye and from EO/IR (not from radar). Outside every zone, the scenario's weather (the sea state in the toolbar) holds. The unit panel's *Weather* row says what a selected unit is in.

**Sonar and sound.** Where the white cell has set a sound-velocity profile, sound bends: a ship's hull sonar hears poorly below the surface layer and not at all in the shadow beyond a few kilometres, while a towed array below the layer, a deep sonobuoy or a far convergence zone may hear what the hull sonar cannot. Each sonar detection in the log says which way the sound went. A ship's *Towed array* row says whether the array is streamed. *Recover* or *Stream* it there; either takes ten minutes, and it listens only at 15 kn or less. A patrol aircraft's or helicopter's *Sonobuoys* row shows how many are left. *Lay on the map* lets you click drop points; tick *deep* for hydrophones at 300 m, then *Lay*. Your buoys show on the map as small rings with their hours of life left, and their bearings cross to fix a submarine.

**Imagery.** When the client's licensed imagery is installed, **View › Imagery** shows it under the picture (the land fill hides and the depth shading dims so it reads), and **View › Imagery packs…** lists the packs and lets you choose one. The imagery's credit is shown with the map. Nothing is downloaded: the imagery comes from files on this machine. The white cell installs a pack with **File › Install imagery pack…** (`.mbtiles` or `.pmtiles`, up to 1 GB); larger packs are copied into the folder the dialog shows.

**Air operations.** If your side has airbases, the **Air** tab lists each one: its runways or deck, its room and its aviation fuel, and the aircraft on the ground with their readiness (ready, alert 15, alert 60, maintenance), what they are doing (turning round, launching in so many minutes), their fuel and their loadout. To fly a sortie, choose the mission (patrol, strike a contact, hold a station, fly to a point), leave the loadout to suit the mission or pick one, and press *Launch*; for a point, click it on the map. The aircraft takes off after its alert delay, any rearming (two minutes a weapon) and its turn on the runway. In the air it shows on the map, and its *Air* row in the unit inspector says what it is doing and its bingo fuel; *Recall* brings it home. It comes home by itself when its mission is done, its weapons are spent, its target is gone or its fuel reaches bingo, and goes to one of your tankers instead if one is nearer. On landing it refuels from the base and turns round (20 minutes, longer for big aircraft, plus two minutes a weapon). An aircraft that runs out of fuel is lost, and so are aircraft on a ship that is sunk.

**Ships keep clear of each other.** Ships and submarines give way by the rules of the road, to their own side as well as to others. A ship giving way turns to starboard, or slows if turning will not do, then comes back to its route or heading on its own; you see it on the map. The event log records each time. A ship cannot see another side's submerged submarine and can run it down. A collision stops both and damages each by the closing speed and the other's size: a small craft hit by a large ship comes off far worse. Order the ships on again when you are ready.

**Shallow water.** With terrain packs, ships and submarines are routed round water shallower than their draft plus 1 m. Harbour approaches, the first and last few kilometres of a leg, are left to your waypoints.
- **No deep-enough route:** the order still goes through, routed round land only, and the status bar warns you: "*X*: no route deep enough for its draft…".
- **Off its planned route:** a ship off its planned route (none, finished, or not planned for its draft) stops rather than steam into water shallower than its draft. The status bar says so ("*X* stopped: 5 m of water ahead…"); the white cell decides any damage.
- **The unit panel's *Water* row** shows the depth under the unit, the least depth on the rest of its route and how far ahead, and its draft. A small profile shows the sea bed against a dashed draft line, and the row turns amber when the water ahead is too shallow.
- **Shipping lanes** show on the map as amber bands, named, with their traffic in ships a day where it is known. The white cell draws or imports them.
- **Ports and anchorages:** they take their depth from the chart (the deepest water in their area) unless the planner sets one. In the white cell's Planning tab, each shows its chart depth; type a depth to override it, or clear the box to go back to the chart.

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
- The unit panel shows its class, position, heading, speed, route, mission, doctrine, stores and damage. It also shows, where they apply:
  - **Group:** its surface action group, with **Detach**, or **join a group…**;
  - **Comms:** its link to its command post, or why orders will not get through;
  - **Fuel:** in scenarios with the fuel model.
- **Esc** clears the selection.

### Orders

| Order | How |
|---|---|
| **Route** | **Plan new route…**, click the map for each waypoint (**Backspace** removes the last), then **Enter** to send or **Esc** to cancel. Ships and submarines plan round the coastline. |
| **Speed** | **Ordered speed (kn)**, then **Set**. |
| **Emissions** | **Emissions**: active, or silent (radar and jammer off). Silence hides you from ESM but blinds your radar. |
| **Weapons control** | **Weapons**: safe, tight (engage only hostile contacts), free (engage anything not friendly or neutral). Inherit uses the group's or side's setting. |
| **Engage** | Choose a **Target** contact, a **Weapon** and **Rounds**, then **Fire**. |
| **Mission** | **Patrol here** (a 30 km patrol round the unit); **Clear** ends the mission. On a group's leader, this sets the whole group's mission. Other missions are set in the scenario or by events: support, mining, mine clearing, cargo, barrier patrol, denial (`docs/models/missions.md`). |
| **Groups** | **Detach** takes a ship out of its group. **join a group…** puts it 3 km astern of the group's leader. |

**Refused orders.** The engine checks every order when it carries it out. A refusal appears in red with the reason, for example:
- *"weapons control status Tight forbids engaging C3"*;
- *"C3 at 182.0 km is outside WPN-ASM's 5.0–150.0 km"*;
- *"U2 cannot be reached from Blue maritime HQ: link jammed"* (or *out of range*, *beyond the radio horizon*, *submerged too deep to receive*).

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
  - **Command posts:** your orders reach a unit only over a comms link from a command post that is up (`docs/models/comms.md`). A link fails if:
    - the unit is out of range;
    - the post is VHF/UHF only and the unit is beyond the radio horizon;
    - an enemy jammer is too close;
    - the unit is a submarine deeper than 20 m.

    Relay units extend a post's reach. If the post is lost or jammed, an alternate takes over after a delay, and orders in the gap are refused or queued until then.
  - **Ports and anchorages:** a unit stopped there (at ≤ 1 kn) is repaired and, after 30 minutes, rearmed, if the facility offers it. Units must fit the facility's depth. Ships that have finished their mission wait at a staging anchorage. In heavy weather, small ships run for a sheltering port.
  - **Fuel** (if the scenario uses it): ships burn more the faster they go. A unit on a mission below 30 % goes to refuel at the nearest refuelling point or oiler. Out of fuel, a ship stops (`docs/models/fuel.md`).
  - **Sense coverage:** tick a sense zone on the **Coverage** tab to see which parts of it your sensors can watch now (green) and where the gaps are (red). With terrain packs, the areas hills and islands hide from your sensors show as gaps. A zone with a **coverage goal** shows it, with the units the engine has sent to watch it: every five minutes the engine sends your free ships, submarines and parked aircraft (best first) to keep the zone at its goal, and says when it has nothing left to send. A unit you give another order is no longer tasked, and the engine sends another in its place.
  - **Surface action groups:** members keep station on the leader and share its mission. Their air defence shares one picture and does not double up on the same missile.

## 8. Messages

- **Messages** shows messages to your side: exercise control, injects (with a title), event messages and sense-zone alerts.
- The newest are at the bottom. The last 100 are kept.

## 9. The platform database

- **View › Platform database…** opens the database browser. Search by id, name or category.
- Each record shows its figures, its source citation, its confidence and its classification marking.
- The shipped database is generic: its figures are illustrative, not real.

**Editing a record** (for the data manager):

1. Choose **Platforms**, **Sensors** or **Weapons** at the top of the browser. Open a platform and choose **Edit record**; a sensor or weapon opens straight in the editor.
2. Set the record's **Source citation**, **Confidence** (A–E) and **Updated** date. Leave a field empty to use the file's default.
3. Edit the record's figures in **Record (JSON)**. It is the text of the source file, so field names and units are as in `docs/db/data-dictionary.md`.
4. Watch the check under the editor. Every change is checked against the whole database, as `sim-db validate` does:
   - **Does not load:** a typo in a field name, or broken JSON. The message gives the line.
   - **Errors**, such as a weapon that does not exist: these block saving.
   - **Warnings** on the record, which do not block saving.
5. **Changes** lists the lines you changed.
6. **Save to source file** saves the whole JSON file (for example `surface-combatants.json`). Put it in `data/db/open/platforms/` for review.
   - The running app keeps its built-in database until it is rebuilt with the file, so an exercise in progress is not changed.
   - **Discard changes** restores the record.

## 10. After the exercise

The white cell runs the replay and the after-action review (AAR). At a side seat during a review, your window follows the replay, showing your side's picture at the scrubber position, and orders are off until the white cell returns to the live exercise.

## 11. Trouble

| Problem | What to do |
|---|---|
| The map is blank | Check the GPU supports WebGL 2; update its driver. |
| "Cannot connect to the simulation engine" | Restart the app. |
| Time does not move | The white cell has paused it, or your window does not control time. |
| An order seems ignored | Look for a red refusal in the status bar at the bottom: the reason is given. |
| A record will not open | The app opens only `.nwsrec` records from this build or older. A record made with terrain packs needs the same packs installed (the message names them). |
| No depth shading on the map | No terrain pack covers the area, or the layers are off in **View**. **View › Terrain packs…** shows what is installed and any pack that was set aside, and why. |
| A terrain pack is "set aside" | It no longer matches its `.sha256` file: the copy is damaged or altered. Install a good copy. |
