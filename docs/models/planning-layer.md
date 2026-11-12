# Behaviour: Operational planning layer

> Phase 3 slice 8. Master prompt §7.1 (operational planning layer). Original design.

- **Code:**
  - `crates/sim-core/src/planning.rs` (facilities, zones, services, command posts)
  - `crates/sim-core/src/c2.rs` (`doctrine_over`)
  - `crates/sim-scenario/src/lib.rs` (format 5: `facilities`, `zones`, `action_groups`)
  - desktop: the editor's **Planning** tab, `planningLayers` in `map.ts`
- **Tests:**
  - `sim-core/tests/combat.rs`, planning section:
    - engage zones;
    - sense zones (EMCON, alerts);
    - command posts;
    - repair, rearm and berths, with save and resume;
    - validation and authority.
  - `sim-scenario`: format 5 loads, builds and keeps station; bad layers are refused.
  - Vitest: facilities, choke points, action-group clean-up.
  - e2e: a choke point added on the map is sent with the scenario.
- **Demonstrator:** `data/scenarios/phase3-planning-demo.json` (in the cross-OS golden replays)
- **Design log:** [DL-0028](../design-log/DL-0028-planning-layer.md)

## Facilities

Each facility belongs to one side:

```json
{ "id": 2, "name": "Refuge Cove", "side": 1, "kind": "sheltering_port",
  "position": { "lat_deg": -39.05, "lon_deg": 146.55 }, "radius_m": 3000,
  "range_m": null, "berths": 2,
  "services": { "repair": true, "rearm": true, "refuel": true } }
```

| Kind | Effect |
|---|---|
| `command_post` | Orders route through the post (below). Ashore when fixed, or carried by a `unit` (a flagship afloat, an aircraft airborne, a land unit). |
| `sheltering_port`, `anchorage`, `refuelling_point` | Serve the side's units that are **alongside**: inside the facility's water and at ≤ 0.5 m/s, with the `services` the facility offers. |

**Water, depth and holding limits** (slice 13, [DL-0032](../design-log/DL-0032-anchorages-weather.md)):
- **`area`:** a polygon or circle that a fixed facility's water uses instead of `radius_m` round `position`. `position` stays the point units steer for, and the scenario check warns if it is outside the area. A facility carried by a unit (an oiler) always uses its radius.
- **`depth_m`:** a unit whose draft (from the platform database) plus 1 m under-keel clearance exceeds the depth is neither served nor sent there. Units with no known draft fit anywhere. The planner enters the depth; bathymetry is Phase 4.
- **`max_sea_state`:** the holding limit. Above it, no unit is sent there, and units holding there leave.
- **`staging`** (anchorages): AI-run units whose mission is complete go to the nearest staging anchorage of their side that they fit (`HoldingAtAnchorage`). They hold there until they are given a new mission.

**Weather.** Every 60 s, a surface unit with a mission whose sea state is at or above its **weather limit** goes to the nearest sheltering port it fits (`ShelteringFromWeather`):
- under 60 m long: sea state 5;
- 60–120 m: sea state 7;
- 120 m and over: sea state 8.

A live mission is set aside. When the sea state falls below the limit, the unit resumes it (`WeatherEased`). Submarines are not affected. Sea state is scenario-wide (weather zones are Phase 4).

**Worked example:** at sea state 6, a 40 m patrol boat (limit 5) on patrol runs for the nearest port and takes up its patrol again when the sea state drops to 4. A 135 m frigate (limit 8) on the same patrol stays out.

**Command posts.** Once a side has a command post, a **player's** order to a unit goes ahead only if:
- the unit is commanded by a post that is up and has a comms link to it ([comms.md](comms.md): range `range_m`, default `radius_m`; radio horizon for VHF/UHF posts; jamming; relays) — a post commands its `subordinates`, or every unit of its side when the list is empty; or
- that post is down and its `alternate` (another command post of the side, up) has taken over, `handover_s` (default 300 s) after the post went down, and has a link to the unit.

A post carried by a unit is **down** once that unit, having been in play, is destroyed or removed (`CommandPostDown` is logged). A post is also down while it is jammed or its carrier's comms are knocked out, and comes back afterwards ([comms.md](comms.md)). In the gap before the alternate takes over, the order is:
- `orders_in_gap: "lose"` (default): refused, *"Blue flagship command is down; command passes to Blue maritime HQ at 00:47:10"*;
- `"queue"`: refused for now (*"queued: …"*), held, and carried out when the alternate takes over if the unit is then in its range (`QueuedOrderApplied`, or `QueuedOrderDropped` with the reason).

Other refusals: *"U3 cannot be reached from Blue maritime HQ: out of range"* (or *link jammed*, *beyond the radio horizon*, *submerged too deep to receive*), *"U3's command posts are down"*, *"U3 is not assigned to a command post"*. White-cell, scenario, script and AI orders are never limited, and a side without command posts is not limited.

**Diversion for repair.** Every 60 s, a mission-killed unit **with a mission** (AI-run) goes to the nearest port of its side that repairs and that it fits (`DivertedToPort`). Its mission is set aside; it stops on arrival; once fully repaired it takes the mission up again, planning its route afresh. Units without a mission are left to their player. A new mission from a player or the white cell ends any diversion (repair, fuel, weather or staging).

**Services**, checked every 60 s:
- **Berths:** at most `berths` units are served at once (0 = no limit), lowest unit id first. Each unit is served by the first facility (by id) it is alongside.
- **Repair:**
  - Fires are put out and flooding is stopped at once.
  - Damage falls by 2 % of the unit's damage points a minute, so a frigate at 120 DP of 300 is repaired in 20 minutes.
  - When the damage reaches zero, critical hits are cleared and the sensors and maximum speed lost to them are restored from the unit's **loadout** (what it had when it entered the exercise). `UnitRepaired` is logged.
- **Rearm:** after 30 minutes alongside, the unit's magazine is restored to its loadout, once per visit. `UnitRearmed` is logged.
- **Refuel:** ships and submarines with a fuel model take fuel at the point's transfer rate, from its stock; units on a mission below bingo divert to the nearest refuelling point, and a refuelling point carried by a unit is an oiler that serves under way ([fuel.md](fuel.md)).
- A unit that moves off (above 0.5 m/s, or out of the radius) stops being served; its 30 minutes start again on its next visit.

## Zones

Each zone belongs to one side, has an `area` (a circle or polygon, as in event triggers), and a `doctrine` (any of `wcs`, `emcon`, `engage_unfriendly`). It may name a `choke_point` it belongs to.

| Kind | Effect |
|---|---|
| `sense` | The side's own units **inside** it take the zone's doctrine fields, above their own, their group's and their side's (for example, radars on, or silent to listen). When the side gains a contact on a unit inside the zone, an alert goes to the side's message log (*"C3 detected in Passage sense zone"*), and `ZoneAlert` is logged. |
| `engage` | The side engages **targets believed to be inside** it (the track's estimated position) under the zone's doctrine, above the shooter's own. A weapons-free engage zone lets a weapons-tight force engage unfriendly contacts there and nowhere else; a weapons-safe zone holds fire into it. **Assigned shooters** (`shooters`): every AI period (10 s), each of the side's contacts believed inside and not already under attack (contact-number order) is engaged by the first assigned shooter that can, one round, at most once per shooter per period, under the zone's rules (`ZoneEngage`). |

- Where zones of the same side and kind overlap, the one with the lowest id applies.
- The doctrine shown for a unit in the unit panel is the one in force where it is.
- Zone rules bind players and the AI alike; the white cell's overrides are not bound by them.

A **choke point** is a sense zone round an engage zone, sharing a `choke_point` name. Every AI period, another side's unit entering any zone of the choke point is logged (`ChokePointTransit`): the transit record for the AAR. The editor's **Add choke point on the map** makes a 40 km sense zone round a 15 km weapons-free engage zone (engage unfriendly: yes).

**Layers** (slice 14, [DL-0033](../design-log/DL-0033-coverage.md)): a zone may carry a `layer` (`outer`, `middle`, `inner`), shown on the map. A sense zone may list the sensor types it is for (`sensors`, for example `["radar", "esm"]` for an outer layer, `["sonar_hull", "sonar_towed"]` for an inner one). Typical layout: sense far out, engage only in the inner layers.

### Coverage

For any sense zone, the side (or the white cell) can ask for its **coverage** now. This is a query: it changes nothing, draws nothing, and is not hashed. In this TTX release players task the sensors themselves; automatic sensor tasking is Phase 4.
- **Grid:** the zone's bounding box is cut into 24 × 24 cells, and the cells whose centre is inside the zone are kept.
- **Covered:** a cell is covered when some unit of the zone's side has a working sensor of one of the zone's types (every type when it lists none) whose **nominal range** reaches the cell's centre. Otherwise the cell is a **gap**.

| Sensor | Nominal range (capped at the sensor's maximum range) |
|---|---|
| Radar, EMCON active | R₅₀ against a 1 000 m² ship, with sea clutter and weather loss ([radar.md](radar.md)), within the radar horizon to a 10 m target |
| ESM | the radar horizon to a 15 m emitter |
| Visual, EO, IR | optical R₅₀ against a 100 m ship in the light at the sensor, within visibility and the horizon to a 10 m target ([optical-mad.md](optical-mad.md)) |
| Hull, towed, dipping sonar (usable) | passive R₅₀ (signal excess 0) against a 115 dB submarine at 5 kn ([sonar.md](sonar.md)) |
| MAD | MAD R₅₀ |
| Sonobuoys | none: they are not laid yet |

The ranges are nominal: where a typical contact can be expected, not a guarantee. Detection itself stays the probabilistic models.

**Worked example:** a frigate with a 25 m radar, 30 km west of the centre of a 20 km radar layer. The radar horizon to a 10 m ship is 4.12 × (√25 + √10) = 33.6 km, nearer than the radar's R₅₀. So the zone is covered out to 33.6 km from the frigate, and the far side of the layer is a gap (about 55 % covered in the test).

**Desktop:** **Sense coverage** lists the seat's sense zones. Ticking one draws its cells on the map, covered in green and gaps in red, with the percentage covered, refreshed every 3 s.

## Surface action groups

```json
{ "name": "SAG 1", "side": 1, "leader": 1,
  "members": [{ "unit": 2, "bearing_deg": 180.0, "range_m": 3000 }] }
```

Action groups are engine objects (slice 16, [DL-0035](../design-log/DL-0035-action-groups.md)), sent with the planning layer, saved and hashed. The group moves and fights as one:
- **Stations:** every AI period (10 s), each member that is not diverted (repair, fuel, weather) keeps station on the leader through an **escort mission**: bearing relative to the leader's heading, and range. Ordering the leader moves the group.
- **One mission:** the group's `mission` (optional in the file; set in play with `AssignGroupMission`, or **Patrol here** on the leader in the desktop) is run by the leader. An order for the group's mission passes through the command posts like an order to its leader.
- **Doctrine:** the leader and members are put in a group named after the action group, under its `doctrine` (optional).
- **Succession:** when the leader is lost or leaves, the first member (in list order) leads and takes the group's mission (`GroupLeaderChanged`). The other members keep station on it. A group with no units left is dissolved (`GroupDissolved`).
- **Detach and join in play:**
  - `DetachFromGroup` takes a unit out: no mission, no group (`UnitDetached`).
  - `JoinGroup` puts a unit of the side into a group at a station (`UnitJoined`). The desktop joins 3 km astern.
  - A unit is in one group at most.
- **Shared air-defence picture:** a member's point defence fires on a missile that any unit of the group tracks this tick. For example, a radar-silent ship fires on its mate's radar track.
- **Fire deconfliction:** at most one **area-defence (SAM)** shot per threat is in the air from the group at a time. Members do not double up on the same missile, so their fire channels go further against a raid. Guns and CIWS still defend their own ship.
- **Checks:**
  - the leader and members are units of the group's side;
  - a member is not the leader, and is in one group only;
  - a member has no mission of its own;
  - each member has a finite bearing and a range > 0.

**Worked example:** two frigates, 1 km apart, each with SAMs and 2 fire channels, against one incoming missile. Apart, both fire at the first chance: two SAMs in the air. As a group, the first defender (unit-id order) fires and its mate holds, so the mate's channels stay free for the next threat.

## Desktop

- **White-cell console, Zone rules:** set each zone's weapons control during the exercise (per-zone ROE override). It is sent as a logged `SetPlanning` from the umpire, so it replays and appears in the AAR.
- **Map:** zones as outlines (sense blue, engage red) with names, and facilities as labelled marks (`CP`, `PORT`, `ANCH`, `RAS`) with their radius.
  - Each seat sees its own side's layer; the white cell sees every side's.
  - The editor draws the draft's layer while editing.
- **Editor, Planning tab:**
  - **Facilities:** add on the map; set name, kind, side, radius, services, berths; for command posts, the command range, the unit carrying it, the alternate, the handover delay and what happens to orders in the gap; move or remove.
  - **Zones:** **Add choke point on the map**; set name, kind, side, circle radius and centre, doctrine (weapons control, emissions, engage unfriendly), and an engage zone's shooters.
  - **Action groups:** leader, and members with bearing and range.
- **AAR timeline:** sense-zone alerts, zone engagements, choke-point transits, command posts lost, queued orders released or dropped, diversions, repairs and rearming.

## Formats

- **Scenario format 5** adds `facilities`, `zones` and `action_groups`. Formats 1–4 still load. Facility `area`, `depth_m`, `max_sea_state` and `staging` (slice 13) are optional fields.
- **Save format 17** adds the planning layer (with the units being served) and each unit's loadout. Older saves load with no layer and the loadout taken from the unit's current state.
- The layer is set by the logged `SetPlanning` command (scenario or white cell only). It is hashed only when present, so scenarios without one keep their golden hashes.

## Limitations

- Depths are entered by the planner. Depths from bathymetry, and runways, wait for GEBCO data and air operations (Phase 4).
- Facilities are not targets: they cannot be attacked or destroyed. Fixed command posts can be jammed ([comms.md](comms.md)) but not attacked.
- Weather is one scenario-wide sea state; weather zones are Phase 4.
- From the desktop, the white cell changes zone weapons control only. Other changes to the layer need the `SetPlanning` command (scenario scripts, batch).
- Coverage uses nominal ranges against reference targets, from a scenario-wide sea state; it does not model terrain masking (Phase 4) or tasking.
- Action groups share their air-defence picture and deconflict SAM fire; they do not yet plan formations or assign sectors.
