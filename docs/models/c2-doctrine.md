# Model: Command and control: postures, doctrine, WCS, EMCON, ROE

> Phase 2 slice 2. Original design built from the master prompt §8 C2 list and common naval usage of the terms. Not SME-validated.

- **Code:** `crates/sim-c2/src/lib.rs` (rules), `crates/sim-core/src/c2.rs` (state), `crates/sim-core/src/sim.rs` (commands), `crates/sim-core/src/detection.rs` (EMCON)
- **Tests:** unit tests in `sim-c2`; engine tests in `crates/sim-core/tests/detection.rs` (C2 section); scenario tests in `sim-scenario`
- **Design log:** [DL-0011](../design-log/DL-0011-c2-doctrine.md)

## Postures

How one side regards another: friendly, neutral, unfriendly, hostile. Postures are one-directional (A may hold B hostile while B holds A unfriendly). A side is always friendly to itself. Pairs not set are neutral.

Only the scenario and the white cell may set postures (`SetPosture`). A side sees its own postures; the white cell sees all.

## Doctrine and inheritance

The client names the weapons control states **safe, tight and free** (the master prompt's "hold" is renamed "safe"; files that still say `"hold"` load as safe).

Doctrine has three fields, each settable on a side, a group (named per side) or a unit:

| Field | Values | Default |
|---|---|---|
| `wcs` (weapons control status) | safe, tight, free | tight |
| `emcon` (emission control) | active, silent | active |
| `engage_unfriendly` (ROE) | true, false | false |

Each field resolves independently: **unit, then group, then side, then the default**. `SetDoctrine` replaces the settings at one level; a field left unset there inherits. A player may set doctrine on its own side, its groups and its units; the scenario and white cell may set any.

## Weapons release rule

`may_engage(doctrine, known)` decides whether doctrine allows firing at a track. `known` is the side's knowledge of the track's affiliation, which exists only once the track is identified to **class** level (a class belongs to a known navy); below that it is `None`.

| WCS | Not identified | Friendly | Neutral | Unfriendly | Hostile |
|---|---|---|---|---|---|
| Safe | no | no | no | no | no |
| Tight | no | no | no | if ROE allows | yes |
| Free | yes | no | no | if ROE allows | yes |

"If ROE allows" means `engage_unfriendly` is true. The rule is used by the engagement model (slice 7); weapon range, track quality and fire channels are checked there. Self-defence under Safe arrives with point defence (slice 9).

## EMCON

When a unit's resolved EMCON is **silent**, its active sensors do not transmit. In this slice that means its radars make no detection attempts and consume no random draws. Passive sensors (ESM, passive sonar) are unaffected when they arrive. Slice 4 (ESM) uses the same state to decide which units are emitting.

## Scenario format

Optional fields, so existing v2 scenario files still load:

```json
"sides": [
  { "id": 1, "name": "Blue",
    "postures": [{ "toward": 2, "posture": "hostile" }],
    "doctrine": { "wcs": "free", "emcon": "silent" } }
],
"groups": [{ "side": 1, "name": "TG1", "doctrine": { "wcs": "safe" } }],
"units": [{ "id": 1, "group": "TG1", "doctrine": { "emcon": "active" }, "...": "..." }]
```

Validation rejects a posture toward the side itself or an undeclared side, a group on an undeclared side, duplicate groups, and a unit in a group its side does not declare.

## Worked example

Blue sets `wcs: free, emcon: silent, engage_unfriendly: true` on the side, group TG1 sets `wcs: safe`, and unit A in TG1 sets `emcon: active`. Unit A resolves to WCS **safe** (group), EMCON **active** (unit), engage unfriendly **true** (side). Its radar transmits, and it may not fire at anything.

## Limitations

- Two EMCON states; graded states (for example navigation radar only) are not modelled.
- No comms links: doctrine changes take effect at once on every unit. Command posts and comms come in Phase 3 and Phase 5.
- Postures are not inferred from events (for example being fired on); only the scenario and white cell change them.
