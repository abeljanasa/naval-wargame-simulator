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
| `sheltering_port`, `anchorage`, `refuelling_point` | Serve the side's units that are **alongside**: within `radius_m` and at ≤ 0.5 m/s, with the `services` the facility offers. |

**Command posts.** Once a side has a command post, a **player's** order to a unit goes ahead only if:
- the unit is commanded by a post that is up and has a comms link to it ([comms.md](comms.md): range `range_m`, default `radius_m`; radio horizon for VHF/UHF posts; jamming; relays) — a post commands its `subordinates`, or every unit of its side when the list is empty; or
- that post is down and its `alternate` (another command post of the side, up) has taken over, `handover_s` (default 300 s) after the post went down, and has a link to the unit.

A post carried by a unit is **down** once that unit, having been in play, is destroyed or removed (`CommandPostDown` is logged). A post is also down while it is jammed or its carrier's comms are knocked out, and comes back afterwards ([comms.md](comms.md)). In the gap before the alternate takes over, the order is:
- `orders_in_gap: "lose"` (default): refused, *"Blue flagship command is down; command passes to Blue maritime HQ at 00:47:10"*;
- `"queue"`: refused for now (*"queued: …"*), held, and carried out when the alternate takes over if the unit is then in its range (`QueuedOrderApplied`, or `QueuedOrderDropped` with the reason).

Other refusals: *"U3 cannot be reached from Blue maritime HQ: out of range"* (or *link jammed*, *beyond the radio horizon*, *submerged too deep to receive*), *"U3's command posts are down"*, *"U3 is not assigned to a command post"*. White-cell, scenario, script and AI orders are never limited, and a side without command posts is not limited.

**Diversion for repair.** Every 60 s, a mission-killed unit **with a mission** (AI-run) goes to the nearest port of its side that repairs (`DivertedToPort`). Its mission is set aside; it stops on arrival; once fully repaired it takes the mission up again. Units without a mission are left to their player.

**Services**, checked every 60 s:
- **Berths:** at most `berths` units are served at once (0 = no limit), lowest unit id first. Each unit is served by the first facility (by id) it is alongside.
- **Repair:**
  - Fires are put out and flooding is stopped at once.
  - Damage falls by 2 % of the unit's damage points a minute, so a frigate at 120 DP of 300 is repaired in 20 minutes.
  - When the damage reaches zero, critical hits are cleared and the sensors and maximum speed lost to them are restored from the unit's **loadout** (what it had when it entered the exercise). `UnitRepaired` is logged.
- **Rearm:** after 30 minutes alongside, the unit's magazine is restored to its loadout, once per visit. `UnitRearmed` is logged.
- **Refuel:** recorded only. Ships have no fuel model; fuel arrives with air operations (Phase 4).
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

## Surface action groups

```json
{ "name": "SAG 1", "side": 1, "leader": 1,
  "members": [{ "unit": 2, "bearing_deg": 180.0, "range_m": 3000 }] }
```

- When the scenario is built, each member gets an **escort mission** on the leader at its station: bearing relative to the leader's heading, and range. Ordering the leader therefore moves the group.
- The leader and members are put in a group named after the action group, under its `doctrine` (optional), so the group fights under one doctrine.
- **Detach:** clear a member's mission (or give it another). **Add:** give a unit an escort mission on the leader.
- **Checks:**
  - the leader and members are units of the group's side;
  - a member is not the leader, and is in one group only;
  - a member has no mission of its own;
  - each member has a finite bearing and a range > 0.
- After the start, a player can still give a member another mission; it then leaves its station.

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

- **Scenario format 5** adds `facilities`, `zones` and `action_groups`. Formats 1–4 still load.
- **Save format 17** adds the planning layer (with the units being served) and each unit's loadout. Older saves load with no layer and the loadout taken from the unit's current state.
- The layer is set by the logged `SetPlanning` command (scenario or white cell only). It is hashed only when present, so scenarios without one keep their golden hashes.

## Limitations

- No fuel model, so refuelling points refuel nothing yet (Phase 4, air operations).
- No draft limits or runways: they need bathymetry (GEBCO) and air operations (backlog).
- Facilities are not targets: they cannot be attacked or destroyed.
- Command-post range is a plain radius: no comms model, relay, line of sight or jamming of the post. A post is lost only with the unit carrying it; fixed posts cannot be attacked.
- Zone polygons are typed in the scenario file; the editor draws only circles.
- From the desktop, the white cell changes zone weapons control only. Other changes to the layer need the `SetPlanning` command (scenario scripts, batch).
- Anchorages serve units alongside but do not yet draw units to them between missions. Sense-zone coverage and gaps are not drawn. Assigned shooters engage, but there is no coordinated air defence across a group.
