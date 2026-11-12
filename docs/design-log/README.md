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
| [DL-0016](DL-0016-weapons.md) | Weapons as entities and engagements | Project spec §8 + platform database + original design |
| [DL-0017](DL-0017-ph-damage.md) | Probability of hit and damage | Project spec §8 + platform database + original design |
| [DL-0018](DL-0018-point-defence.md) | Point defence and soft kill | Project spec §8 + platform database + original design |
| [DL-0019](DL-0019-missions.md) | Basic missions and automated tasking | Project spec §7, §8 + original design |
| [DL-0038](DL-0038-terrain-packs.md) | Terrain and bathymetry packs: format, sampling, install, map layers | Project spec §6, §14 + GEBCO 2020 + netCDF classic spec + original design |
| [DL-0037](DL-0037-db-editing-and-rehearsal.md) | Database record editing, symbol function codes, fixes from the pilot rehearsal | Project spec §9, §10 + ADR-0004 + original design |
| [DL-0036](DL-0036-desktop-facilitation.md) | Event forms, polygon drawing, live planning edits, day/night shading | Project spec §10 + original design |
| [DL-0035](DL-0035-action-groups.md) | Surface action groups in the core | Project spec §7.1 + original design |
| [DL-0034](DL-0034-weapon-tracks.md) | Weapons in flight as sensor targets | Project spec §7, §8 + the detection models + original design |
| [DL-0033](DL-0033-coverage.md) | Sense-zone layers, coverage and gaps | Project spec §7.1 + the detection models + original design |
| [DL-0032](DL-0032-anchorages-weather.md) | Anchorage areas, depth and holding limits, staging, sheltering from weather | Project spec §7.1 + original design |
| [DL-0031](DL-0031-missions.md) | Support, mining, mine-clearing, cargo, barrier and denial missions | Project spec §7, §7.1, §8 + textbook mine-warfare threat formula + original design |
| [DL-0030](DL-0030-fuel.md) | Fuel and refuelling | Project spec §7, §7.1, §8 + propulsion cube law + platform database + original design |
| [DL-0029](DL-0029-comms.md) | Comms links and their disruption | Project spec §7, §7.1, §8 + free-space propagation + original design |
| [DL-0028](DL-0028-planning-layer.md) | Operational planning layer | Project spec §7.1 + original design |
| [DL-0027](DL-0027-scenario-editor.md) | Scenario editor | Project spec §7, §13 + original design |
| [DL-0026](DL-0026-replay-aar.md) | Replay and after-action review | Project spec §13 + RFC 4180 + original design |
| [DL-0025](DL-0025-multi-seat.md) | Multi-seat play | Project spec §10, §13 + original design |
| [DL-0024](DL-0024-lua.md) | Sandboxed Lua scripting | Project spec §7, §12 + mlua (MIT) + original design |
| [DL-0023](DL-0023-injects.md) | Injects and white-cell facilitation | Project spec §10 + original design |
| [DL-0022](DL-0022-events.md) | Scenario event engine | Project spec §7 + original design |
| [DL-0021](DL-0021-adjudication.md) | White-cell adjudication overrides | Project spec §10 + original design |
| [DL-0020](DL-0020-mines.md) | Basic mines: fields, laying, transit risk | Project spec §8 + textbook mine-warfare threat formula + platform database |
