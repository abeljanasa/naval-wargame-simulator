# Model: Comms links and their disruption

> Phase 3 slice 10. Original design; **not SME-validated**. A planning-level link model for a TTX: it decides whether an order gets through. It is not a radio-propagation model.

- **Code:** `crates/sim-c2/src/comms.rs` (hop rule), `crates/sim-core/src/comms.rs` (stations and jammers in the world), `crates/sim-core/src/planning.rs` (`command_authority`, posts going down and coming back)
- **Tests:** unit tests in `sim-c2` (hand values); engine tests in `crates/sim-core/tests/combat.rs` (comms section)
- **Design log:** [DL-0029](../design-log/DL-0029-comms.md)

## What it does

Once a side has a command post ([planning-layer.md](planning-layer.md)), a **player's** order to a unit goes ahead only if a post that commands the unit is up and has a **link** to it. White-cell, scenario, script and AI orders are not limited, and a side without command posts is not limited.

A link is either direct, or through the post's **relays**: units of its side that pass its traffic on. A relay counts once the post, or an earlier relay, reaches it. The order then goes through the first relay (in list order) that reaches the unit.

## One hop

A hop from a transmitter to a receiver is up when all of these hold, checked in this order (the first failure is the reason):

| Check | Fails as |
|---|---|
| Both ends can take traffic: comms not knocked out by a critical hit | `knocked_out` |
| Neither end is a submarine deeper than 20 m (no VLF modelled) | `submerged` |
| Distance ≤ the post's `range_m` (default `radius_m`) | `out_of_range` |
| Distance ≤ the radio horizon between the antennas, for a `line_of_sight` (VHF/UHF) post | `beyond_horizon` |
| J/S < 0 dB at the receiver (the traffic) and at the transmitter (the acknowledgement) | `jammed` |

**Antenna heights:** a post ashore uses `antenna_m` (default 30 m); a post carried by a unit, and every unit, uses its own: ship 25 m, land unit 10 m, aircraft its altitude, submarine 2 m (mast raised, at 20 m or shallower).

**Radio horizon:** the 4/3-earth horizon used for radar, 4.12 km × (√h₁ + √h₂), with heights in metres. Posts default to beyond line of sight (HF or satellite), which has no horizon limit. An airborne post gains range from its altitude.

## Jamming

Every unit with a jammer whose EMCON is active ([radar.md](radar.md)) also jams links. It jams a station of another side that is within the radio horizon between the jammer and that station. For one jammer at a receiver:

J/S = J₁ − S₁ − A + 20·log₁₀(d_s / d_j)

| Symbol | Meaning |
|---|---|
| J₁ | the jammer's `jn_db_at_1km` (the same figure it uses against radar) |
| S₁ | the link's signal at 1 km, 60 dB (constant) |
| A | the post's `anti_jam_db` (default 10 dB) |
| d_s | length of the signal path |
| d_j | the jammer's distance from the receiver |

Several jammers add in power, in unit-id order. A hop is jammed when the total reaches 0 dB.

**Worked example:**
- **Jammer near the receiver:** a red jammer with J₁ = 60 dB is 30 km from a blue frigate, whose post is 100 km away. J/S = 60 − 60 − 10 + 20·log₁₀(100/30) = +0.46 dB, so the link is jammed. Move the jammer out to 40 km and J/S = −2.04 dB, so the link holds.
- **Two jammers:** two such jammers at 50 km give −3.98 + 3.01 = −0.97 dB, so the link still holds.

## Posts going down and coming back

A post is **down** when one of these happens:
- **Lost:** the unit carrying it is destroyed or removed (`CommandPostDown`). It stays down.
- **Disrupted:** checked every tick (`CommandPostDisrupted`, with the reason).
  - **Jammed:** J/S at the post exceeds 0 dB for a unit at the edge of its range (d_s = `range_m`), so it can no longer hear its own net.
  - **Knocked out:** its carrier's comms are knocked out by a critical hit.

  When the disruption ends (the jammer falls silent or moves off, or a full repair clears the critical), the post is **back up** (`CommandPostRestored`).

While a post is down, its alternate takes over after `handover_s`, and orders issued in the gap are lost or queued, as in [planning-layer.md](planning-layer.md).

## Refusals

When no acting post has a link, the order is refused with the first post's reason. For example: *"U3 cannot be reached from Blue maritime HQ: link jammed"*, or *"… beyond the radio horizon"*.

## In the picture

- **`comms`:** for each of the viewer's units, when its side has command posts. Gives the status, the post, and the relay used.
- **`posts`:** each of the viewer's command posts, up or down, with the reason and the time it went down.

The white cell sees every side's. The desktop shows a **Comms** line in the unit panel, and **Command posts** in the console.

## Files

Command-post fields (all optional):

```json
{ "kind": "command_post", "range_m": 300000, "line_of_sight": false,
  "antenna_m": 30, "anti_jam_db": 10, "relays": [12, 14] }
```

## Limitations

- **Orders only.** Picture sharing still reaches every unit of the side. Data-link sharing through comms links is Phase 5 ("comms and cyber effects").
- **No propagation detail:** no frequency bands, skywave, terrain screening, or VLF to deep submarines.
- **Jammers:** a jammer jams all of a station's links equally; no directional or spot jamming.
- **No draws:** a link is up or down, never a probability. A TTX facilitator can see why.
