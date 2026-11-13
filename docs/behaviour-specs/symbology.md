# Behaviour spec: map symbology

- **Code:** `apps/desktop/src/lib/symbols.ts`; used by `map.ts`. Tests: `src/lib/ui.test.ts` (*symbols*).
- **Sources:**
  - The public APP-6 / MIL-STD-2525 conventions: frame shape by affiliation and domain; a sea unit's function shown as a short hull or role abbreviation, and air roles as letters.
  - Common public hull classification abbreviations (DDG, FFG, SSK…).
  - Original design otherwise. No symbol set, font or artwork from any product was copied.

## What a symbol shows

1. **Affiliation:** shown by the frame's shape and colour, so it reads without colour.
   - Friend: circle, blue.
   - Hostile: diamond, red.
   - Neutral: square, green.
   - Unknown: quatrefoil, yellow.
2. **Domain:** shown by the frame's extent.
   - Sea surface: the full frame.
   - Air: the upper half.
   - Subsurface: the lower half.
   - Land: the full frame, except that a friendly land unit is a rectangle.
3. **Function** (new): a short text code inside the frame, in bold dark text.

## Function codes

The code comes from what the viewing side knows:
- **own units:** their platform class;
- **contacts:** the class once identified, else the category once classified, else none. A symbol never shows more than the classification ladder ([tracks.md](../models/tracks.md)).

| Code | Platform class (generic database) | Category fallback |
|---|---|---|
| CV, LHD, CG, DDG, FFG, FFL, PG, OPV, MCM, LST | Carrier, amphibious assault ship, cruiser, destroyer, frigate (both), corvette, fast attack craft, offshore patrol vessel, MCM vessel, landing ship | `surface_combatant`: no code. An unknown type shows the frame alone, because no code is better than a wrong one |
| AOR, AKE, ATF, AGS, AGI | Oiler, stores ship, tug, survey ship, intelligence ship | `auxiliary`: A |
| MV, TK, FV, OSV | Container, bulk and ro-ro ships; tanker; trawler; offshore supply vessel | `merchant`: MV |
| SSN, SSGN, SSK, SSM | Nuclear attack, nuclear guided-missile, diesel-electric (with or without AIP), midget submarine | `submarine`: SS |
| F, B | Fighters; maritime strike bomber | `fighter`: F |
| P, W, K, H, U | Maritime patrol, airborne early warning, tanker, helicopter, UAV | `mpa` P, `aew` W, `tanker` K, `helicopter` H, `uav` U |
| ASM, SAM, RDR | Coastal anti-ship missile battery, SAM batteries, radar site | `shore_battery` BTY, `sensor_site` RDR |
| AB, PT | Air bases and airfields, ports | `airbase` AB, `port` PT |

- **Size:** a code of up to two characters is drawn at 24 px on the 64 px symbol, three characters at 17 px, and four at 13 px. On the map the symbol is 36 px for units (up from 28) and 30 px for contacts (up from 24), so the code is readable.
- **Other platforms:** classes outside this table, such as regional or accredited records, fall back to their category, then to no code.
- **Every generic class has a code.** `ui.test.ts` checks the platform database for this, so a new class needs a code in the same change.

## Figures (default)

**View › Unit figures** (on by default; off shows the APP-6 frames and codes above) draws each unit as a figure of what it is (`apps/desktop/src/lib/silhouettes.ts`):

- **Ships and submarines:** plan views (top-down) rotated to the heading, with the bow pointing the way the unit moves.
  - Each class has its own hull length and deck features: gun mounts, superstructure, helicopter decks, flight decks with an island, container rows, hatches, tanker pipework, a submarine's sail and tail fins.
  - Beams are drawn 1.6× true, so a hull reads at map size.
- **Aircraft:** top-down outlines rotated to the heading. Fighter, bomber, maritime patrol, AEW (with rotodome), tanker, helicopter (rotor disc) and UAV.
- **Land sites:** upright icons for missile and SAM batteries, radar sites, airbases and ports.
- **Munitions in flight:** top-down, rotated to the heading. Anti-ship, surface-to-air, cruise and air-to-air missiles; torpedoes, ASW rockets, bombs, gunfire, depth charges, decoys and mines. A homing weapon is outlined in white.
- **Colour and badge:** each figure is filled in its affiliation colour. A small APP-6 frame badge beside the unit's name shows affiliation by shape as well: a circle for friend, a diamond for hostile, a square for neutral, a quatrefoil for unknown, in the frame of the unit's domain. The badge faces the screen and stays beside the label; it is not part of the figure, which turns with the heading.
- **Orientation:** plan-view figures (ships, submarines, aircraft, weapons in flight) lie flat on the map, turned to the heading. When the map is rotated or tilted they turn and tilt with it, so they always point along their heading. Land-site icons and APP-6 frames face the screen. A contact's figure faces the screen too, since a side does not know a contact's course.
- **Contacts:** a contact shows a figure only as far as it is classified (class, then category, then domain). An unclassified contact keeps the APP-6 frame.
- **Coverage:** every generic platform class and every weapon has a figure, and `ui.test.ts` checks this.
- **Size follows the map scale.** Each figure has a nominal size on the ground: ships 9 km, aircraft and land sites 7 km, and munitions 2.8 km (gunfire 1.2 km). The size is exaggerated so it reads at planning scale.
  - Figures shrink as the map zooms out and grow as it zooms in.
  - On screen they stay between 18 and 72 px (munitions 9–30 px).
  - Ships keep their relative sizes, because each hull is drawn at its own length.
- **Contacts** are drawn north-up, because a side does not know a contact's course.

## Weapon outcomes

Hits, misses and weapons shot down show where they happened:
- **Markers:** a filled burst for a hit, a pale ring for a miss, an amber ring for a weapon shot down. Each is labelled for 20 seconds and fades over a minute.
- **Notice:** the newest outcome appears in the status bar for 15 seconds.

Fog of war applies (`sim-core` `outcomes.rs`):
- **The white cell** sees all of them, with names.
- **A side** sees:
  - its own shots, without the target's identity, since it fired at a contact;
  - attacks on its own units, without the firing unit.

## Not in scope

- Echelon, mobility and status amplifiers.
- Graphic (non-text) icons for air and land functions.
- Text modifiers beyond the unit label.

These are left for a later change if the staff ask for them at the pilot.
