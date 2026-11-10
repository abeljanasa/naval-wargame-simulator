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
| [DL-0005](DL-0005-platform-database.md) | Platform database: schema, migrations, validation, 50 generic types | Project spec §3, §9 + original design + generic open-literature ranges |
| [DL-0006](DL-0006-gate1-radar-detection.md) | Gate 1 interim radar detection and contacts | Project spec §6.2, §8 + radar textbook + original design |
| [DL-0007](DL-0007-scenario-platform-references.md) | Scenario units reference platform-database classes | Original design |
| [DL-0008](DL-0008-land-avoidance.md) | Land avoidance and route planning | Project spec §8 + Natural Earth + original design |
| [DL-0009](DL-0009-database-browser.md) | In-app platform database browser | Project spec §9 + original design |
| [DL-0010](DL-0010-tracks.md) | Tracks: measurement error, fusion, error growth, classification | Project spec §8 + textbook estimation + original design |
| [DL-0011](DL-0011-c2-doctrine.md) | Postures, doctrine inheritance, WCS, EMCON, ROE | Project spec §8 + public naval usage + original design |
| [DL-0012](DL-0012-radar-clutter-jamming.md) | Radar clutter, weather loss and noise jamming | Project spec §8 + radar textbook + original design |
| [DL-0013](DL-0013-esm.md) | ESM: intercept, bearings, fixes and identification | Project spec §8 + textbook geometry + original design |
| [DL-0014](DL-0014-sonar.md) | Sonar: passive and active sonar equations | Project spec §8 + underwater acoustics textbook + original design |
| [DL-0015](DL-0015-optical-mad.md) | Visual, EO/IR and MAD | Project spec §8 + solar geometry + original design |
