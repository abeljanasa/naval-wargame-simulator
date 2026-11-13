# Changelog

## [Unreleased]

### Added (Phase 4, slice 5: choke points, depths and lanes)

- **Shipping lanes** in the planning layer and in scenarios (`lanes`): a centreline, a width, a choke point, and traffic in ships per day.
  - A traffic figure must name its source (a licensed data set, a chart, or "planner's estimate"), or it is refused.
  - The white cell draws lanes on the map or imports them from GeoJSON; an imported traffic figure without a source is dropped.
  - Lanes show on the map for every seat.
- **Choke points' water:** least, typical and deepest depth, a section across the choke point (square to its lane), and the navigable width for a draft. All are in the white cell's Planning tab.
- **Transit risk:** each transit of a choke point with lanes or terrain packs is assessed (`TransitAssessed`): lane, traffic, water under the keel, draft, and low, moderate or high risk.
  - The AAR has a new *Transits* table and `transits.csv`.
- **Unchanged:** without lanes, the planning layer hashes as before, and the golden replays are unchanged.

### Added (Phase 4, slice 4: terrain masking)

- **Terrain masking** (terrain packs): land between the two ends of a line of sight blocks radar, ESM and optical detection, line-of-sight radio links, and sense coverage.
  - The test is one 4/3-earth path profile, sampled every 500 m.
  - Land sites and posts ashore stand on the ground, so a radar on a hill sees over lower land.
- **Land clutter** for aircraft at or below 150 m above land; they were treated as over the sea before.
- **Unchanged:** without packs nothing changes, and the golden replays are unchanged.

### Added (release: world terrain layer from its online source)

- **The installers carry the 4′ world terrain layer**, built by each release from GEBCO on NOAA ERDDAP (`scripts/terrain/world-layer.sh`, the new `terrain` job in the release workflow), so the map stays current.
  - A manual release run takes the GEBCO dataset id and credit, so a newer grid is a run away.
  - The pack also ships beside the installers, in `release-extra/terrain/`, with its `.sha256`, the download's SHA-256 and the date (`SOURCE.txt`).
- **`nws-batch terrain build --source --credit`** records the data set and its credit in the pack.

### Added (Phase 4, slice 3: shallow water and depths)

- **Routing by draft:**
  - With terrain packs, ships and submarines are routed round water shallower than their draft plus 1 m: 3 × 3 samples per cell of the 1/30° planning grid, with a one-cell clearance like the coast's.
  - Harbour ends of a leg (2 cells) are left to the planner's waypoints.
  - When no route is deep enough, the order still stands, routed round land only, with a `RouteShallow` warning to the seat.
- **Shoal guard, off the navigable graph:**
  - A unit with no charted route (none, finished, uncharted, or with a shallow leg) stops rather than move into water shallower than its draft (`ShoalAhead`).
  - A unit on a charted route is trusted.
  - Seats now see "stopped: land ahead" and "stopped: N m of water ahead" warnings for their own units.
- **Chart depths for ports and anchorages:** the deepest water in the facility's area, used for draft checks unless the planner sets a depth.
  - The white cell's Planning tab shows each chart depth and takes an override.
- **Depth along the route:** `Simulation::route_depths`, shown in the unit panel as a *Water* row and a profile against the draft.
- **`nws-batch run | mc | aar --terrain <pack>…`** runs, and replays, with terrain packs.
- **Save format 26** (`Route::charted`); format 25 saves still load.
- **Unchanged:** without packs, everything behaves as before, and the golden replays are unchanged.

### Added (Phase 4, slice 2: terrain and bathymetry packs)

- **Terrain packs** (`.nwsterrain`):
  - Integer-grid heights from GEBCO 2020, whose land heights are SRTM15+. The packs and the raw downloads stay outside the repository; their SHA-256 sums are in the data dictionary.
  - Two packs are built: the Philippine area at 15″ (30.5 MB) and the world at 4′ (17.5 MB). The world layer measured 64.5 MB at 2′, so 4′ is the installer layer.
- **`nws-batch terrain build | info | sample`:**
  - builds a pack from a GEBCO netCDF-3 file (the reader is our own, so there is no C library);
  - averages blocks for coarser layers;
  - writes a `.sha256` file next to the pack.
- **Engine:**
  - Deterministic bilinear sampling, with a 20,000-sample golden on both CI operating systems.
  - The finest covering pack wins.
  - The terrain id is in the state hash when packs are set; runs without packs keep their hashes and every golden replay is unchanged.
  - Exercise records store the terrain id, and a replay refuses packs that differ, naming the ones it needs.
- **Desktop:**
  - **File › Install terrain pack…** checks the pack against its `.sha256` file before copying it in.
  - Packs are read at start-up, and one that no longer matches its `.sha256` is set aside with the reason.
  - **View › Depth shading / Depth contours / Relief** draw layers below the coastline and every unit.
  - **View › Terrain packs…** lists what is installed. The status bar shows the depth or height under the cursor, and the map credits GEBCO.
- **New direct dependencies:** `flate2` (pure-Rust backend) and `sha2`. Both were already in the lockfile, so no new crates.

### Added (Phase 4, slice 1: Monte Carlo batch)

- **`nws-batch mc <scenario> --runs N [--seed S] [--threads T] --out <dir>`** runs a scenario for N seeds in parallel. Each run is independent and deterministic, so the results don't depend on the thread count, and a test checks this.
- **Default measures of effectiveness**, until the client names its own:
  - per side: losses, time to first detection, weapons fired per kill, choke-point transits;
  - for the whole run: cargo delivered.
- **Outputs:**
  - `runs.csv`: one row per seed, with its final hash;
  - `summary.csv`: n, mean, SD, 95 % CI, min, median, max;
  - `summary.json`: the same, plus a ten-bin histogram per measure.
- **Speed:** 20 four-hour runs of the Mindoro pilot take 89 s on 12 cores.
- **`runs.parquet`** (Apache Parquet): `seed`, `final_hash`, and one nullable Float64 column per measure.
- **New dependencies:** `parquet`, `arrow-array` and `arrow-schema` 60.1 (Apache-2.0, compression features off). `cargo deny` passes.
- **Scaling:** 12 one-hour runs take 2.8 s on 1 thread and 0.9 s on 12, with identical results.

### Added (map)

- **Units drawn as figures of what they are** (default; **View › Unit figures**, off for APP-6 frames):
  - plan views of ships and submarines with their deck features, rotated to their heading;
  - top-down outlines for aircraft, rotated to their heading;
  - icons for land sites.
  - Figures are sized on the ground, so they scale with the map zoom, kept between 18 and 72 px on screen.

  Each figure has an affiliation colour and badge. A contact shows a figure only as far as it is classified.
- **Munitions in flight drawn as figures:** missiles, torpedoes, rockets, bombs, gunfire, decoys. A side's tracks on incoming weapons show as missile or torpedo figures.
- **Hit and miss feedback:** a marker where each outcome happened, and a status-bar notice. Fog of war applies:
  - the white cell sees all, with names;
  - a side sees its own shots without the target's identity, and attacks on its own units without the shooter.
- **Engine:** `sim-core` `outcomes.rs`, a display-only outcome feed. It is saved (save format **25**, so replay seeks match) but not hashed, so golden replays are unchanged. The snapshot gains `outcomes`.
- **Tests:**
  - a figure for every platform class and weapon;
  - notice wording;
  - `hit_and_miss_feedback_follows_fog_of_war`;
  - the packaged-app test waits for an outcome notice.

### Changed (platforms)

- **The application targets Windows and macOS only** (client decision, 2026-11-12, ADR-0006).
  - No Linux installers. The release builds MSI/NSIS and `.app`/`.dmg`.
  - CI tests and checks determinism on Windows and macOS. The packaged-app test runs on Windows.
  - Linux steps are removed from the workflows, the test script and the docs. The tooling jobs (format, cargo deny, database validation, benchmarks, offline build, front end) moved to Windows runners, so no CI job runs on Linux.

### Fixed

- **A loaded scenario showed an empty map until the clock ran.** The scenario's units and settings are tick-0 commands, so before Run or Step the white cell saw no units and the default sea state.
  - The desktop now runs the first second when it loads a scenario. It opens at `D+0 00:00:01`, with the starting picture, and time-zero events have fired.
  - The review scrubber starts there too.
  - Determinism, records and replays are unchanged: it is the tick a straight run starts with.
  - Found by the new packaged-app test.
- **A window that started slowly could miss its starting picture** and show no units until something moved. The window now starts listening for frames before it asks for its session, and asking makes the backend send a full picture next. Loading a scenario or a record asks again, so a full picture that arrives before the load reply is not lost. Found by the packaged-app test on Linux CI.

- **White-cell actions sent while paused did nothing until the clock ran.** That covered injects, messages, overrides, and planning and zone edits, which the run sheet uses during pauses for decisions. The host now runs one second when the white cell acts while paused, so the action shows at once. Player orders still wait for the clock. Found by rehearsing the run sheet.
- **The review scrubber ran past the panel's edge** and added a horizontal scrollbar.
- **Typing in a text area triggered keyboard shortcuts.** A space paused or ran the clock, a digit changed the compression, and the character was lost. That affected Lua scripts, event JSON and more. Shortcuts now ignore every text entry. Found while testing the record editor.

### Added (scenarios)

- **`data/scenarios/phase3-pilot-mindoro.json`**, *Mindoro Strait pilot*: the pilot exercise in the Philippine Area of Responsibility (ADR-0007). It has the same planning-layer features and decision points as Kent passage. Red is a fictional force with generic platforms. It has a golden replay and `mindoro_pilot_reaches_every_decision_point`, and is in the clean-scenario check. The pilot plan §4 and the run sheet cover it.

### Added (database editing)

- **Sensors and weapons are editable too:** **View › Platform database…** has a Platforms / Sensors / Weapons switch. Sensors and weapons open straight in the record editor and save to `sensors.json` or `weapons.json` (`sim_db::edit::RecordKind`).

### Added (signing, ADR-0008)

- **`scripts/signing/new-selfsigned-codesign.ps1`** makes the self-signed Windows code-signing certificate (`CN=Naval Wargame Simulator`, 3 years). It exports the PFX and the public `.cer`, and sets the Actions secrets. The client types the password; it is never printed.
- **Release:** the Windows signing step deletes the PFX even when signing fails, then checks that every installer carries a signature. macOS builds are ad-hoc signed.
- **Docs:** trusting the certificate on exercise machines (Group Policy or `Import-Certificate`) is in `releasing.md` and the installation check.
- **First signed release** ([run 43222859373](https://github.com/abeljanasa/naval-wargame-simulator/actions/runs/43222859373)): the MSI and NSIS installers are signed by `CN=Naval Wargame Simulator` and timestamped; the macOS app is ad-hoc signed.

### Decided (Phase 4, ADR-0007)

- **Region of interest:** the whole Philippine Area of Responsibility. Full-resolution data covers 3°N–27°N, 111°E–137°E, including the Kalayaan Island Group.
- **Imagery:** client-licensed offline tile pack, imported by the app; none in the repository.
- **Gate 4 engagements** (proposals for the client's subject-matter experts to confirm): Operation Praying Mantis (1988) and *Hangor* vs *Khukri* (1971), with the *Samuel B. Roberts* mine strike (1988) as the alternate.
- **Data packs:** a coarse global layer in the installer, and a full-resolution regional pack installed separately with a SHA-256 check.
- **Still open:** the measures of effectiveness.

### Added (database editing)

- **Edit a platform record in the app** (**View › Platform database… › Edit record**, DL-0037). It covers:
  - provenance fields (citation, confidence, date) and the record's JSON;
  - live validation against the whole database, the same checks as `sim-db validate`;
  - a diff against the original.
- **Save to source file** downloads the edited JSON source file for review. Only the edited record's text changes in it (`sim_db::edit`). The running app keeps its built-in database until it is rebuilt.

### Changed (map)

- **Less clutter, found in the rehearsal screenshots:**
  - **Overlapping labels** are decluttered on screen, by priority. The order is the selected unit, units, contacts, facilities, minefields, zones, reference points. The hidden ones show as you zoom in.
  - **Zone names** sit on the zone's northern edge instead of its centre, where they collided with reference points ("Pakent passagene").
  - **Routes:** the selected unit's whole route; every other unit's next leg as a 15 km stub.
  - **ESM bearing lines** are drawn 40 km long, down from 150 km.
- **Function codes inside unit and contact symbols:** `DDG`, `FFG`, `SSK`, `MV`, `F`, `H` and so on, from the platform class, or the category for a contact classified only that far. An unclassified contact shows the frame alone. The spec is in `docs/behaviour-specs/symbology.md`. Every generic class has a code, and a test checks it.
- **The map frames the units a seat sees** when a scenario's first picture arrives, and again when the seat changes. It used to open at a fixed zoom, where the pilot's units were an unreadable cluster of labels.

### Added (packaged-app test)

- **`npm run e2e:app`** drives the built app over WebDriver (tauri-driver), with no new dependencies. It covers the real engine, database, map, scenario loading, the white-cell seat and running at Max. It also rehearses the white-cell run sheet: console tabs, events, sending an inject, command posts, a locked seat window, a logged override and a verified replay. `E2E_SHOTS=<folder>` saves a screenshot at each step. CI runs it on Windows (*Packaged app end to end*); see `docs/build/releasing.md`.

### Added (Phase 3, slice 20: pilot kit)

- **`docs/gate3/pilot-kit/`**, printable handouts for the TTX pilot:
  - the air-gapped installation check;
  - a seat card for players;
  - the white-cell run sheet for the Kent passage pilot;
  - the observer's log;
  - the questionnaire (SUS and five open questions).
- **Release workflow:** artefacts now carry the shipped scenarios (`release-extra/scenarios/`) and `SHA256SUMS-<os>.txt` for every installer and scenario, written after signing.

### Added (planning)

- **Draft Phase 4 plan** (`docs/plan/phase-4.md`) for the Gate 3 panel. It covers 14 slices, the client decisions they need (region, imagery licence, historical engagements, data pack size, MOEs) and the risks. It is linked from the Gate 3 pack and the backlog.

### Added (Phase 3, slice 19: pilot scenario)

- **`data/scenarios/phase3-pilot.json`**, *Kent passage pilot*: the Gate 3 pilot exercise, with three seats, four injects and eight events. It uses:
  - comms jamming of the flagship net, with a relay and a handover to the HQ ashore;
  - fuel, with an oiler that refuels a patrol vessel below bingo;
  - Red minelaying and Blue mine clearing on the eastern approach;
  - barrier-patrol, denial and cargo missions;
  - a staging anchorage with a sea-state limit, dusk, and a gale that sends small craft to shelter.
- **Golden replay** for it. The existing golden hashes are unchanged.
- **Tests:** `pilot_scenario_reaches_every_decision_point` checks the pilot plan's decision points in order. The clean-scenario check now covers both planning scenarios.
- **Pilot plan** §4 rewritten around it. The planning demonstrator is now the short back-up.

### Changed (desktop layout)

- **Menu bar** (File, View, Exercise, Help) for occasional actions. The platform database opens in a dialog.
- **Toolbar:** time controls, the clock and the seat picker.
- **Tabbed side panels:**
  - Left: Picture, Messages (with an unread count), Coverage.
  - Right: Unit, plus Injects, Events, Planning, Seats, Log and Review where the seat has them.
- **Status bar:** errors and map-pick prompts, instead of the bottom of a scrolling sidebar.
- **Manuals** updated for the new layout.

### Changed (Phase 3, slice 18: Gate 3 pack update)

- **Gate 3 pack** (`docs/gate3/README.md`): the P1 gaps are closed in phase, and the client's Phase 4 deferrals are recorded with their reasons.
- **Backlog:** closed items are ticked, and a Phase 4 section lists what was deferred.
- **Manuals:**
  - User manual: comms, fuel, groups, sense coverage, weapon tracks, day/night.
  - White-cell manual: the live planning layer, command-post state, jamming, fuel, weather and group injects.

### Added (Phase 3, slice 17: desktop editing and facilitation)

- **Event forms** in the scenario editor (DL-0036):
  - The trigger, with area triggers drawn on the map as a circle or polygon.
  - Conditions and actions, routes drawn on the map, and Lua as script text.
  - JSON stays available for injects, seats and groups.
- **Polygon drawing** for zones and facility areas (from slice 13); event areas here.
- **White-cell planning layer in play:**
  - Toggle a facility's services, or remove it.
  - Add a sense or engage zone on the map, set a zone's emissions, or remove it.
  - Each change is one logged `SetPlanning` (`order_planning`). The groups in play are kept.
- **Day/night shading** on the map at the game time, from the engine's solar model, with a **Day/night** switch.
- **e2e smoke:** the event form, and scenario format 6.

### Added (Phase 3, slice 16: surface action groups in the core)

- **Action groups are engine objects** (`docs/models/planning-layer.md` §Surface action groups, DL-0035):
  - **One mission**, run by the leader (`AssignGroupMission`).
  - **Succession:** the first member takes over when the leader is lost or leaves (`GroupLeaderChanged`).
  - **Stations:** members keep station every AI period.
  - **In play:** units detach (`DetachFromGroup`) and join (`JoinGroup`).
- **Coordinated air defence:**
  - Members fire on any group member's weapon track (shared picture).
  - At most one SAM per threat is in the air from the group at a time (fire deconfliction).
- **Desktop:**
  - The unit panel shows the group, with **Detach** and **join a group…**.
  - **Patrol here** on a leader sets the group's mission.
  - New commands `order_group_mission`, `order_detach`, `order_join`.
- **Picture:** `action_groups`.

### Changed (slice 16)

- **`SetPlanning`** takes optional `action_groups`. When they are omitted, the groups in play are kept.
- **Save format 24.**

### Added (Phase 3, slice 15: weapons in flight as sensor targets)

- **Weapon tracks** (`docs/models/weapons-damage.md` §Weapons as sensor targets, DL-0034):
  - Each side tracks other sides' weapons in flight, every tick, with no draws:
    - missiles by radar (0.1 m²), IR (plume), EO and visual (5 m body), and ESM (active seekers homing);
    - torpedoes by hull, towed and dipping sonar (150 dB).
  - Tracks are logged (`WeaponDetected`, in the AAR timeline), saved and hashed.
- **Point defence** also fires on a missile that the defender's own non-radar sensors track, so a radar-silent ship with IR can defend itself.
- **Picture:** `weapon_tracks`. The desktop draws them as rings: red for missiles, blue for torpedoes.

### Changed (slice 15)

- **Save format 23.** Golden hashes are unchanged: no weapon is in flight at the end of a shipped scenario, and their outcomes did not move.

### Added (Phase 3, slice 14: sense-zone layers and coverage)

- **Zone layers** (`outer`, `middle`, `inner`), and sensor types for sense zones (`docs/models/planning-layer.md` §Coverage, DL-0033).
- **Coverage query** (`Simulation::zone_coverage`): a 24 × 24 grid over a sense zone. It shows which cells the side's working sensors of the zone's types reach at nominal range against reference targets. It is a query: nothing changes and nothing is hashed.
- **Desktop:**
  - **Sense coverage:** tick a zone to draw covered and gap cells on the map (green and red), with the percentage covered, refreshed every 3 s.
  - The scenario editor sets each zone's layer and sensor types.
  - Map labels show the layer.
- **Command:** `zone_coverage`. A side's seat sees only its own zones.

### Changed (slice 14)

- **Save format 22.**

### Added (Phase 3, slice 13: anchorages and weather)

- **Facility water and limits** (`docs/models/planning-layer.md`, DL-0032):
  - A polygon or circle `area` that replaces the radius.
  - `depth_m` against each unit's draft (from the platform database) plus 1 m clearance.
  - A holding limit, `max_sea_state`.
- **Staging anchorages:** AI-run units whose mission is complete hold at the nearest one they fit until tasked again (`HoldingAtAnchorage`).
- **Sheltering from weather:** surface units with a mission at or above their weather limit go to the nearest sheltering port they fit (`ShelteringFromWeather`). They resume when the sea state eases (`WeatherEased`). The limit is sea state 5, 7 or 8 by length.
- **Desktop scenario editor:**
  - Draw polygon areas for facilities.
  - Draw polygon zones, or turn a polygon back into a circle.
  - Facility depth, holding limit and staging.
- **Map:** facility areas are drawn as polygons.
- **Scenario check:** a warning when a facility's point is outside its area.

### Changed (slice 13)

- **Diversions:** a new mission ends any diversion. A unit resuming plans its route afresh.
- **Save format 21.**

### Added (Phase 3, slice 12: more missions)

- **Six missions** (`docs/models/missions.md`, DL-0031):
  - **Support:** hold a station, engaging nothing.
  - **Mining:** lay a field at a point (`MissionMinesLaid`).
  - **Mine clearing:** sweep an area at 8 kn. It removes mines from other sides' fields (`MineSwept`), at a tenth of the normal risk.
  - **Cargo:** load, carry and unload, 30 minutes at each end (`CargoLoaded`, `CargoDelivered` with the tonnes).
  - **Barrier patrol:** back and forth between two points.
  - **Denial:** patrol a choke point's zones and engage only contacts inside them.
- **Engagement:** support, mining, mine-clearing and cargo units no longer engage on their own (point defence still protects them).
- **AAR timeline:** mines laid and swept, cargo loaded and delivered, failed missions.
- **Desktop:**
  - The unit panel describes every mission.
  - The scenario editor sets every mission: stations, centres and points on the map, the mine count, the tonnage, and the choke point.

### Changed (slice 12)

- **Save format 20:** mission progress (phase and start tick). Older saves load.

### Added (Phase 3, slice 11: fuel and refuelling)

- **Fuel for ships and submarines** (`docs/models/fuel.md`, DL-0030):
  - Opt-in per scenario (`"fuel": true`, scenario format 6).
  - Sized from the platform database: capacity 10 % of displacement; endurance and cruise speed from the database.
  - Burn by speed (cube law plus a 10 % hotel load). Out of fuel means no propulsion (`FuelExhausted`).
- **Refuelling:**
  - **Bingo:** AI-run units below 30 % go to the nearest refuelling point of their side with fuel left (`DivertedForFuel`), and resume their mission when full (`UnitRefuelled`).
  - **Points:** each has a transfer rate (`transfer_t_per_h`, default 300 t/h) and a stock (`fuel_t`; none means unlimited).
  - **Oilers:** a refuelling point carried by a unit. It refuels ships under way, which keep company 2 km ahead.
  - **Queues:** berths make queues (`WaitingForBerth`).
- **Desktop:**
  - A Fuel line in the unit panel.
  - Scenario editor: the fuel-model switch, each unit's starting fuel, and refuelling points' carrier, stock and rate.
- **Platform database API:** `PlatformSummary` gains `draft_m`, `endurance_m` and `displacement_t`.

### Changed (slice 11)

- **Formats:** save format 19, scenario format 6. Older files load.
- **Resuming a mission:** a unit resuming after repair now clears its route and speed, so the mission AI plans afresh.

### Added (Phase 3, slice 10: comms links and disruption)

- **Comms links from command posts** (`docs/models/comms.md`, DL-0029):
  - A player's order needs a link from a post that commands the unit: range, radio horizon for VHF/UHF posts, jamming, relays, no reception below 20 m, and comms critical hits.
  - Jamming uses each unit's jammer while its EMCON is active: J/S at both ends of a hop, several jammers adding in power.
  - A jammed post, or one whose carrier's comms are knocked out, goes down: its alternate takes over after the handover delay. The post comes back when the disruption ends (`CommandPostDisrupted`, `CommandPostRestored`, both in the AAR timeline).
  - Posts default to beyond line of sight (HF or satellite), so existing scenarios behave as before. An airborne line-of-sight post gains range from its altitude.
- **Picture:** each unit's link (status, post, relay) and each command post's state.
- **Desktop:**
  - A Comms line in the unit panel, and Command posts in the console.
  - Scenario editor: link type, antenna, anti-jam margin, subordinates and relays for command posts.
- **Scenario check:** subordinates and relays must be units of the post's side.
- **Phase 3 plan:** slices 10–18 close the in-phase P1 gaps. Imagery, GEBCO/SRTM-dependent items and air operations move to Phase 4.

### Changed (slice 10)

- **Save format 18.**
- **Refusal text:** *"is out of range of every command post"* became *"cannot be reached from {post}: out of range"*.
- **Golden:** the planning demonstrator's golden hash, because facilities now hash with their link fields. Its behaviour is unchanged.

### Added (Phase 3, slice 1: white-cell adjudication)

- **Phase 3 plan** (`docs/plan/phase-3.md`): white cell and injects first, at the client's choice.
- **`Adjudicate` command** (white cell only, reason required; `docs/models/adjudication.md`, DL-0021):
  - Force a weapon in flight to hit or miss, set or repair damage, destroy or remove a unit, move a unit, set stores.
  - Every override is logged with umpire and reason before it takes effect, saved, and replays exactly.
- **Desktop white-cell console:** reason box; Repair, Destroy and Remove for the selected unit; Hit and Miss for weapons in flight; the override log.

### Added (Phase 3, slice 2: event engine)

- **`sim-events` crate and the event engine** (`docs/models/events.md`, DL-0022):
  - Triggers: time, every, area entered, unit destroyed, contact gained.
  - Conditions: event fired or not, unit alive or gone, units alive, time window, probability.
  - Actions: message, posture, doctrine, environment, speed, route, mission, spawn a reserve, enable an event.
  - Checked once per tick in id order. Firings, held events and failed actions are logged.
- **Scenario format 3:** `events` and `reserve` units.
- **Message logs per side** in every picture.
- **Desktop Messages panel.**
- **New demonstrator:** `data/scenarios/phase3-ttx-demo.json`, an escalation exercise.

### Added (Phase 3, slice 3: injects)

- **Inject library in scenarios** (`docs/models/injects.md`, DL-0023): role-play sender, audience, text, and a white-cell note.
- **Scheduled injects:** the `send_inject` event action.
- **White-cell `Facilitate` command:** send an inject or a free-text message, fire an event, turn an event on or off. Logged with the umpire.
- **White-cell picture:** the inject library with send counts, and every event's state.
- **Desktop white-cell console:**
  - Injects (Send).
  - Events (enable, Fire).
  - Message to.
  - Inject titles in the Messages panel.

### Added (Phase 3, slice 4: Lua scripting)

- **Lua 5.4 conditions and actions on events** (`docs/models/lua-scripting.md`, DL-0024):
  - Built on mlua, vendored, so it works air-gapped.
  - A `sim` API: read time, events, units and counts; draw from the simulation stream; ask for messages, injects, event switches, postures, speeds and reserves.
- **Sandbox:**
  - Only `table`, `string`, `math` and `utf8`; no file, OS, module, debug, coroutine, `load` or `pcall` access.
  - Text only; 1 M instructions and 16 MiB per run; 64 KiB per script.
  - Escape tests cover all of this.
- **Determinism replacements:** sorted `pairs`, address-free `tostring` and `format`, `libm` maths, `sim.random()`, no finalizers or weak tables.
- **Errors:** script errors are logged, and the run continues. Syntax errors stop the scenario loading.
- **Demonstrator:** gains a scripted umpire brief, which the cross-OS golden replay runs.

### Added (Phase 3, slice 5: multi-seat play)

- **Seat table** (`docs/models/multi-seat.md`, DL-0025):
  - White-cell and side seats, declared in the scenario or defaulted (one white-cell seat plus one per side).
  - Set by the logged `SetSeats` command.
  - The core refuses player commands from the wrong seat or side, and umpire commands not from a white-cell seat.
- **Desktop seat windows:**
  - The main window is the control station and may sit at any seat.
  - The white cell opens windows locked to a seat, each with its own picture.
  - Only the main window and white-cell seats run the clock.
- **Seat management** in the white-cell console: add, remove, open a window.

### Added (Phase 3, slice 6: replay and AAR)

- **`sim-aar` crate** (`docs/models/replay-aar.md`, DL-0026):
  - **Exercise records** (`.nwsrec`): seed, coastline, command log, final hash, and the scenario.
  - **Replay:** rebuilt from the record and checked against the final hash, with keyframes for seeking.
  - **AAR tables:** side summary, losses with cause and credit, weapons expended against effective (fired, hits, missed, shot down, kills), detections, and a timeline. CSV export (RFC 4180, formula-safe).
- **Desktop exercise review:**
  - Replay the live exercise, or open a saved record.
  - Scrubber; every seat window follows the replay; orders are refused until Back to live.
  - AAR overlay with click-to-jump and Save CSV.
- **`nws-batch aar`:** replays a record, or runs a scenario and records it, then writes the five CSV tables. It fails if the replay does not match.
- **Save format 16:** hit rolls record the firing unit and side (golden hashes unchanged).

### Added (Phase 3, slice 7: scenario editor)

- **Desktop scenario editor** (`docs/models/scenario-editor.md`, DL-0027):
  - Edit the loaded scenario, start a new one, or edit a file.
  - Forms for the overview and environment; sides and postures; units (platform, position, heading, speed, route, mission, reserve); minefields; and reference points.
  - Map picking for positions, routes, patrol centres, destinations and minefields.
  - Events, injects, seats and groups as JSON, with event templates.
  - The map shows the draft while editing.
- **Validation by the engine:** `Scenario::check` lists every error and warning at once. Errors block Save and Play.
- **Scenario format 4:** `reference_points`, shown on the map to the seats that may see them.

### Added (Phase 3, slice 8: operational planning layer)

- **Planning layer** (`docs/models/planning-layer.md`, DL-0028):
  - **Command posts** ashore or carried by a unit, with subordinates and range. When a post's unit is lost, an alternate takes over after a handover delay, and orders in the gap are lost or queued.
  - **Damaged units divert:** mission-killed units on a mission go to the nearest repair port, and resume the mission when repaired.
  - **Engage-zone shooters** engage contacts inside the zone on their own.
  - **Choke-point transits** are logged.
  - **Action-group doctrine.**
  - **White-cell per-zone weapons control override.**
  - **Sheltering ports, anchorages and refuelling points:** repair, rearm and berths for units alongside. A full repair restores the sensors and speed lost to critical hits.
  - **Sense zones:** doctrine for own units inside, and alerts on new contacts.
  - **Engage zones:** per-zone WCS and ROE for targets inside.
  - **Choke points:** a sense zone round an engage zone.
  - **Surface action groups:** members keep station on the leader.
- **Scenario format 5** (`facilities`, `zones`, `action_groups`); **save format 17** (planning layer, unit loadouts).
- **Desktop:**
  - The planning layer is drawn on the map for each seat.
  - Planning tab in the scenario editor.
  - Alerts, repairs and rearming in the AAR timeline.
- **New demonstrator:** `data/scenarios/phase3-planning-demo.json`, with its golden replay. Existing golden hashes are unchanged.

### Added (Phase 3, slice 9: Gate 3 pack)

- **Manuals** (`docs/manuals/`):
  - user manual;
  - white-cell manual;
  - scripting reference for scenario authors. Its JSON examples are checked against the engine types by `sim-scenario/tests/docs.rs`.
- **Gate 3 review pack** (`docs/gate3/README.md`): the definition of done mapped to evidence, with the P1 and §7.1 gaps listed for the panel.
- **TTX pilot plan** (`docs/gate3/ttx-pilot-plan.md`).
- **macOS** is back in the CI (lint, test, determinism) and release matrices.

### Changed

- Save format 15 (events, injects, messages, seats).
- Golden hashes: phase 1 `dd942f76e85bd4f2`, phase 2 `cb360f9a36c8bc87`, phase 3 `db033be444af12f8`.
- Desktop: `set_viewer` is replaced by `set_seat`. Time-control commands now return errors to seats that may not use them.
- CI: the air-gapped job builds and tests `sim-events`.
- Save format 13 (adjudication log).

### Added (Phase 2, slice 12: Gate 2 pack)

- **Combat demo scenario** `data/scenarios/phase2-surface-action.json`: Blue destroyer and frigate against two Red missile corvettes in Bass Strait, weapons free, with a Red minefield and a neutral merchant. It is included in the golden replays and the cross-OS determinism matrix.
- **Gate 2 scenario test** `gate2_demo_detects_engages_defends_and_damages`: both sides fire, point defence and decoys engage, hits do damage, a unit is destroyed, the minefield is run, and the neutral is untouched.
- **Duel-test index** `docs/gate2/duel-tests.md`: every model's duel and hand-value tests.

### Added (Phase 2, slice 11: basic mines)

- **Minefields** (`docs/models/weapons-damage.md` §Mines, DL-0020):
  - Created by the scenario or white cell, or laid from a ship's stores.
  - Ships and submarines of other sides moving through a field run the simple-initial-threat risk P = 1 − exp(−ρWd) every 10 s. An encounter rolls the mine's Ph and damage.
  - The owning side is immune and sees its fields; the desktop draws them.
- Duel test: 200 transits match the hand-calculated 0.471 encounter rate.

### Changed

- Save format 12 (older saves still load). Golden hash is now `1f07e67a956cb372`.

### Added (Phase 2, slice 10: basic missions)

- **`sim-ai` crate and AI system** (`docs/models/missions.md`, DL-0019):
  - Patrol (random points over a circle, logged draws), ferry, escort station-keeping and strike missions, run every 10 s.
  - Tasked units engage automatically: nearest contact first, with the first weapon that passes every `Engage` check, release rule included.
- `AssignMission` command; optional `mission` on scenario units.
- **Desktop:** the unit panel shows the mission and offers "Patrol here" and "Clear".

### Changed

- Save format 11 (older saves still load). Golden hash is now `b296e935e3375df2`.

### Added (Phase 2, slice 9: point defence and soft kill)

- **Point defence** (`docs/models/weapons-damage.md`, DL-0018):
  - Ships with a radiating radar detect incoming missiles and engage them after a reaction time.
  - SAMs defend the area; guns and CIWS defend their own ship. Fire channels are limited, so saturation emerges.
  - Interceptors resolve at the intercept time with a logged Ph draw. Point defence works under any WCS.
- **Soft kill:**
  - Targets fire matched decoys (chaff or IR against missiles, countermeasures against torpedoes); the database decoy effect lowers Ph.
  - A radiating jammer on the target lowers an active radar seeker's Ph by × 0.8. Hit rolls log the countermeasure modifier.

### Changed

- Save format 10 (older saves still load).

### Added (Phase 2, slices 7 and 8: weapons, Ph and damage)

- **`sim-weapons` crate and combat engine** (`docs/models/weapons-damage.md`, DL-0016, DL-0017):
  - **`Engage` orders** are checked against the shooter's condition, magazine, target domain, track quality for the guidance type, range, and the WCS/ROE release rule (the white cell may override).
  - **Missiles and torpedoes** fly toward the side's current track estimate. A seeker gets one acquisition draw when the real target is in range, then homes. Semi-active and command weapons need the track held to impact.
  - **Guns, CIWS, bombs, depth charges and rockets** resolve at once.
  - **Ph** = base × size × manoeuvre × sea-state modifiers.
  - **Damage:** damage points from the warhead, and critical hits on sensors, propulsion, weapons, flight deck or comms. Fire and flooding burn every minute until damage control wins. Units become mission-killed or destroyed.
  - Every draw is logged with its inputs.
  - **Gate 2 duel test:** 300 engagements match the hand-calculated 0.72 hit rate.
- **Database queries** for a platform's stores (magazines, or the first loadout for aircraft), weapon figures and damage profile. The scenario loader sends the weapon catalogue.
- **Desktop:**
  - The selected unit shows its condition, damage, criticals and stores, plus an Engage form (target, weapon, rounds).
  - Weapons in flight appear on the map.
  - Refused orders show the engine's reason.

### Changed

- Save format 9 (older saves still load).
- Golden hash is now `1e874e39cf5d2dbe`.

### Added (Phase 2, slice 6: visual, EO/IR and MAD)

- **Visual, EO/IR and MAD** (`docs/models/optical-mad.md`, DL-0015):
  - Daylight from solar elevation at the scenario's start time.
  - Optical range set by the sensor, target size, light and visibility. IR works at night and sees further through haze.
  - Close looks identify to category or class, and class reveals affiliation.
  - Visual gives positions; EO and IR give precise bearings that join the fix pool.
  - MAD on aircraft finds submarines within about a kilometre of slant range.
- Environment gains a start time (UTC) and visibility. Units carry their category and length from the database.

### Changed

- Save format 8 (older saves still load).
- The demo starts at 2026-11-10 00:00 UTC, in daylight. Golden hash is now `a6d33e91f79e97ea`.

### Added (Phase 2, slice 5: sonar)

- **Sonar** (`docs/models/sonar.md`, DL-0014):
  - Passive SE = SL − TL − (NL − DI) − DT and active (noise-limited) sonar equations. Pd = Φ(SE / 5 dB).
  - Spherical-then-cylindrical spreading with Thorp absorption.
  - Speed-dependent radiated noise and self-noise; ambient noise set by sea state.
  - Active sonar transmits under EMCON active, and a pinging ship is heard at its ping level.
  - Active detections feed tracks. Passive bearings join the ESM pool and triangulate into fixes.
  - Dipping sonar needs a low hover; sonobuoys wait for missions.
- Every sonar attempt logs each term of the sonar equation.

### Changed

- Save format 7 (older saves still load). `UnitSpawn.radiated_noise_db` and `SensorSpec.active` are added.
- Golden hash is now `66e6a556f1ac1068`.

### Added (Phase 2, slice 4: ESM)

- **ESM** (`docs/models/esm.md`, DL-0013):
  - Passive intercept of radiating radars and jammers, out to 2.5× the radar's reference range within the horizon. Draws and bearing errors are logged.
  - Bearings from two or more platforms in one scan triangulate into a fix that feeds or creates a track.
  - An emitter library from the platform database identifies the carrier to class, category or domain. Class-level identification reveals affiliation.
- **Engine:** `SetEmitterLibrary` command, issued by the scenario loader. Contacts carry category and class. Snapshots carry the side's bearing lines.
- **Desktop:** ESM bearing lines on the map; contacts named by class, category or domain.
- **Performance:** an exact chord-distance pre-check and per-unit geodesic reuse in the radar and ESM passes. 1,000 units run at 451× real time and 10,000 at 5.0× (budgets 60× and 1×). Results are unchanged.

### Changed

- Save format 6 (older saves still load).
- Golden hash is now `e32c602bbebfedbb`.

### Added (Phase 2, slice 3: radar)

- **Radar model** (`docs/models/radar.md`, DL-0012):
  - Sea and land clutter shorten detection range against low targets, scaled by sea state.
  - A scenario-wide weather loss applies to every radar.
  - Noise jamming, self-screening and stand-off, with main-beam and sidelobe J/N. Burn-through emerges as range closes.
  - Jammers radiate only while EMCON is active.
- **Environment:** `SetEnvironment` (scenario and white cell) sets sea state and weather. Every side sees it, and the desktop header shows the sea state.
- **Database migration 0002:** radar `clutter_db_per_sea_state` and `beamwidth_deg`, filled for all 15 generic radars, with two new validation rules.
- **Scenario format:** optional `environment` and per-unit `jammer`.
- Detection events log clutter loss, weather loss and J/N.

### Changed

- Save format 5 (older saves still load).
- The demo scenario runs in sea state 3. Golden hash is now `271dd129bbb9548d`.

### Added (Phase 2, slice 2: command and control)

- **`sim-c2` crate:** postures, doctrine inheritance (unit, then group, then side), the WCS weapons-release rule (safe, tight, free) with the ROE `engage_unfriendly` setting, and EMCON (model doc `docs/models/c2-doctrine.md`, DL-0011).
- **Engine:**
  - `SetPosture` (scenario and white cell only) and `SetDoctrine` (side, group or unit) commands.
  - EMCON silent switches a unit's radar off.
  - Snapshots carry each unit's doctrine, the viewer's postures, and contact affiliation once a track is identified.
- **Scenario format:** optional `postures`, side `doctrine`, `groups`, and unit `group` and `doctrine`.
- **Desktop:** weapons (WCS) and emissions (EMCON) selectors on the selected unit, showing what is inherited. Contacts are coloured by affiliation once identified.

### Changed

- Save format 4 (older saves still load).
- The demo scenario sets both sides neutral and Blue weapons safe. Golden hash is now `d46bae3f7ea350a8`.

### Added (Phase 2, slice 1: tracks)

- **Phase 2 plan** (`docs/plan/phase-2.md`) and **ADR-0005**: model crates hold pure functions, and `sim-core` runs them.
- **`sim-sensors` crate:** the radar Pd model moved here, plus measurement error, track fusion and the classification ladder.
- **Tracks:**
  - A detection reports a position with drawn range and bearing error, logged on the detection event.
  - Each side fuses every detection of a target into one track (Kalman update). The error grows between updates by the domain's speed bound.
  - Radar classifies tracks to their domain.
  - Model doc `docs/models/tracks.md` (DL-0010).
- **Desktop:** contacts show the estimated position, a 95 % error ellipse on the map, their domain and their error in the Contacts panel.

### Changed

- Save format 3 (v2 saves still load).
- Golden hash for the Phase 1 scenario is now `8a1f96e397c0a0b7`: detections draw their measurement errors.

### Added (Phase 1, close-out)

- **Turn rates:** units turn at ω = min(cap, v/r_min) and reach a waypoint within their turning radius. Generic class values come from scenario resolution (ships r_min = 2.5 × length).
- **Land avoidance:** Natural Earth 50m land rasterised at 1/30°, A* routes round the coast with a coastal penalty and string-pulling, and a grounding guard. Blocked routes are rejected and logged (DL-0008).
- **Desktop:**
  - APP-6-style frames per domain (surface, air, subsurface, land) and affiliation.
  - Light theme alongside the default dark theme.
  - Load a scenario from a file.
  - Snapshot diffs on the UI stream: a full picture first, then only changed and removed units.
  - **Platform database browser:** search, record detail with citation, confidence and marking, and validation feedback (DL-0009).
- **Front-end tests:** Vitest unit tests and a Playwright end-to-end smoke test against a mocked backend, both in CI.

### Changed

- CI and release builds skip macOS for now (the jobs are commented, not deleted).
- Golden hash for the Phase 1 scenario is now `7ad884a980341eb1` (turn rates and land mask).

### Added (Phase 1, remaining delivery items)

- **Criterion benchmarks** (`cargo bench -p sim-scenario --bench engine`), with a PR CI job that fails on a > 15 % regression against the base commit, measured on the same runner.
- **Map rendering benchmark** (`npm run bench:map`): the production map code with 10,000 symbols at 30 Hz updates reaches 59.2 fps on integrated graphics.
- **Map decluttering:** above 1,500 units, name labels and heading leaders are drawn only for the selected unit; with every label drawn it fell to 27.8 fps. Symbol URLs are cached, and the label glyph set is fixed.
- **npm licence audit** of production dependencies (`npm run licenses`, in CI).
- The synthetic benchmark scenario moved to `sim_scenario::synthetic`. The output is unchanged (bench hash `14c631676919c2af`).

### Fixed

- The map's coastline URL is now root-relative, so pages outside the site root can load it.

### Added (Phase 1, delivery)

- **Performance:** an exact latitude broad phase in detection with bit-identical results. 1,000 units run at 203× real time and 10,000 at 2.2× (budgets: 60× and 1×). Measure with `nws-batch bench`.
- **Release scan:** `sim-db scan <dirs>` checks database files against `data/db/release-allowlist.txt`, recognising them by header, and fails on any record or JSON classification other than UNCLASSIFIED.
- **Dependency policy** (`deny.toml`): no GPL/AGPL/LGPL; `cargo deny check` passes.
- **CI:** dependency audit, a performance gate, and an air-gapped (vendored, `--offline`) build.
- **Release workflow:** MSI/NSIS, DMG, AppImage/deb/rpm installers, signed when secrets are configured, with the release scan and CycloneDX SBOMs (Rust and npm).
- **Docs:** `docs/build/performance.md`, `releasing.md`, `air-gapped.md`.

### Added (Phase 1, Gate 1: units move and detect)

- **Scenario format v2:**
  - A unit can name a platform class (`"platform": "GEN-FFG"`) that supplies its performance, sensors and RCS. Inline values override it, and v1 files still load.
  - The resolved values travel in the spawn command, so replays don't depend on the database.
- **Embedded open database** (`sim_db::embedded`): the desktop app and batch runner need no data files. `nws-batch --db <file>` loads a built database instead.
- **Interim radar detection model:**
  - Runs every 5 s: radar horizon, RCS^¼ range scaling, Pd = 1/(1+(r/R₅₀)⁸), one seeded draw per scan.
  - Every draw is logged with its inputs.
  - Per-side contacts are dropped after 300 s without a detection.
  - Side snapshots show contacts without target ids.
  - Duel tests check it against hand calculation (model doc `docs/models/radar-detection-gate1.md`, DL-0006).
- **Desktop:** contacts on the map and in a Contacts panel, and the unit's class in the details panel.
- **Demo scenario:** the bulk carrier now transits eastbound, so the two ships meet and detect each other. A Gate 1 test asserts both move and both detect.

### Changed

- **Save format v2:** adds sensors, signatures and contacts. v1 saves still load.
- **Golden hash re-blessed** (`92931f90968e0415`): engine state now includes platforms, sensors, RCS, contacts and detection draws, and the demo scenario uses database classes.

### Added (Phase 1, platform database)

- **`sim-db`:**
  - SQLite schema (migration 0001) with provenance columns on every value table.
  - Forward-only migrations tracked by `user_version`; newer and outdated schemas are refused.
  - A JSON source loader (ADR-0004).
  - A validation suite of 34 rules, each with a seeded-fault test.
  - `PlatformDb` loading through `DbSource::Open`. `DbSource::Accredited` sits behind the `accredited` feature.
  - CLI: `validate`, `build`, `migrate`, `list`.
- **Open generic database** (`data/db/open`): 50 platforms, 33 sensors, 27 weapons, 7 comms systems, 12 propulsion types and 18 damage profiles. All are generic and UNCLASSIFIED, with confidence D.
- **Docs:** ADR-0004, the data dictionary (`docs/db/data-dictionary.md`), DL-0005.

### Added (Phase 1, first tranche)

- **ADRs:** ADR-0001 (tech stack), ADR-0002 (Svelte 5 UI), ADR-0003 (determinism strategy).
- **Workspace and lints:** a Cargo workspace with `forbid(unsafe_code)`, and clippy rules that ban non-deterministic APIs (`HashMap`, the wall clock, `std` transcendental maths).
- **`sim-geo`:** WGS-84 Vincenty direct and inverse on `libm`, and the radar horizon, tested against published reference values.
- **`sim-core`:**
  - ECS world with a fixed 1 s tick and per-subsystem ChaCha8 RNG streams.
  - Versioned commands with authorisation by seat, plus a command log and replay.
  - Viewer-filtered snapshots, the state hash, MessagePack saves with a version check, and waypoint movement.
- **`sim-scenario`:** the JSON scenario format (v1), validation, determinism tests and golden-replay tests.
- **`nws-batch`:** a headless runner that prints final state hashes.
- **Desktop app (Tauri 2 + Svelte 5 + MapLibre + deck.gl):**
  - Offline Natural Earth coastline, unit symbols, routes and heading leaders.
  - Pause, step, time compression (1×–60×, max), seat or white-cell view, route planning and ordered speed.
  - Typed bindings generated with tauri-specta.
- **CI:** fmt, clippy and tests on Windows, macOS and Linux; a cross-OS determinism hash comparison; front-end typecheck, lint and build.
