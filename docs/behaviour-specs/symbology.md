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

## Not in scope

- Echelon, mobility and status amplifiers.
- Graphic (non-text) icons for air and land functions.
- Text modifiers beyond the unit label.

These are left for a later change if the staff ask for them at the pilot.
