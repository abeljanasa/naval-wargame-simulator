# Design log

Every implemented feature has an entry recording **where its specification came from** (clean-room rule 5, master prompt §2). Allowed sources:

- public manuals
- marketing pages
- published scenario descriptions
- conference talks
- analysts' behaviour specs (in `/docs/behaviour-specs/`)
- public scientific or engineering literature
- original design

Never cite another product's binaries, databases, save files or internals.

| ID | Feature | Source type |
|---|---|---|
| [DL-0001](DL-0001-sim-core-loop-and-protocol.md) | Fixed-step loop, RNG streams, command/snapshot protocol, saves | Original design |
| [DL-0002](DL-0002-geodesy-and-movement.md) | WGS-84 geodesy and waypoint movement | Public geodesy literature + original design |
| [DL-0003](DL-0003-radar-horizon.md) | Radar horizon | Public radar engineering literature |
| [DL-0004](DL-0004-desktop-map-and-time-controls.md) | Desktop map, unit symbols, time controls, seats | Project spec §10 + public symbology standards + original design |
