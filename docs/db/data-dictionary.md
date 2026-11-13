# Platform database: data dictionary

- **Schema:** `crates/sim-db/migrations/` (version 3)
- **Source data:** `data/db/open/`
- **Format decision:** [ADR-0004](../adr/0004-platform-database-format.md)

Units are SI in the database: m, m/s, m/s², t, kg, s. Authoring JSON uses kn, nm and km where noted, and the builder converts. 1 kn = 1852/3600 m/s; 1 nm = 1852 m.

## Provenance (every value table)

| Column | Meaning |
|---|---|
| `source_citation` | Where the value came from. Must not be empty. |
| `confidence` | A–E, see below. |
| `classification` | Must be `UNCLASSIFIED` in the open build. Other markings are only valid in the accredited database. |
| `is_generic` | 1 for a generic class, 0 for a specific real system. The open build warns on 0. |
| `created_by` | Team or role that entered the record. Never a personal name. |
| `updated_at` | ISO date, `YYYY-MM-DD`. |

Value tables are listed in `sim_db::PROVENANCE_TABLES`. Link rows (`launcher_weapons`, `magazine_contents`, `loadout_items`, `platform_comms`) inherit their parent's provenance.

### Confidence scale

| Grade | Meaning |
|---|---|
| A | Official or manufacturer-published figure for this exact item, or a definitional constant |
| B | Several independent, credible open sources agree |
| C | A single credible open source |
| D | **Generic class estimate:** a representative value inside the range open literature gives for the class, not tied to any real system |
| E | Placeholder or notional; must be replaced before analytical use |

## Generic values

Phase 1 ships only generic classes (`GEN-*`), all with confidence D. Each value is chosen to sit inside the range typical of its class in general open literature (reference books, encyclopaedic overviews, textbooks). It is **not** taken from any one platform and must not be read as describing one.

- No vendor database was used. Clean-room rule 2 bans Command PE's DB3000/CWDB and any other proprietary database.
- Where a class spans a wide range, a mid-range value was chosen.
- Analysts who need specific systems must use the regional database (Phase 4) or the accredited database.

## Tables

| Table | Key | Purpose |
|---|---|---|
| `nations` | `id` | Owning nation (`GEN` = generic) |
| `services` | `id` | Navy, air force, army, coast guard, civil |
| `propulsion` | `id` | Propulsion type and fuel |
| `damage_profiles` | `id` | Damage points, mission-kill fraction, fire and flooding susceptibility (0–1). Used by the Phase 2 damage model |
| `comms_systems` | `id` | Comms kind, range (m), data-link flag |
| `sensors` | `id` | Kind, role, band, active/passive, min and max range (m), radar reference range (m) at a reference RCS (m²), highest target altitude covered (m); radars also carry clutter loss per sea-state step (dB) and beamwidth (°), added in migration 0002; sonars may carry frequency (kHz), directivity index, detection threshold, active source level and self-noise at 10 kn (dB), added in migration 0003 ([acoustics](../models/acoustics.md)) |
| `weapons` | `id` | Kind, guidance, min and max range (m), speed (m/s), warhead (kg), base Ph (0–1), target domains |
| `platforms` | `id` | Category, domain, service, propulsion, damage profile, dimensions, speeds (m/s), acceleration (m/s²), endurance (m), ceiling (m), max depth (m), hardpoints |
| `platform_signatures` | (platform, kind, aspect, speed band) | RCS (m²), IR (kW/sr), acoustic source level (dB re 1 µPa @ 1 m), visual (m) |
| `mounts` | `id` | A fitted sensor or launcher, with its height above waterline, ground or airframe datum (m; negative = below the waterline, e.g. a sonar dome) |
| `launcher_weapons` | (mount, weapon) | Which weapons a launcher can fire |
| `magazines` / `magazine_contents` | `id` / (magazine, weapon) | Stowage capacity and default load |
| `loadouts` / `loadout_items` | `id` / (loadout, weapon) | Aircraft weapon loads, limited by `platforms.hardpoints` |
| `platform_comms` | (platform, comms) | Fitted comms |
| `ecm_systems` | `id` | ECM equipment (migration 0004): kind (`noise_jammer`, `deception_jammer`) and strength as J/N in a victim receiver 1 km away (dB). A unit jams with its platform's strongest ECM unless its scenario gives it a jammer |
| `platform_ecm` | (platform, ecm) | Fitted ECM |

## Validation rules

Run `cargo run -p sim-db -- validate`. Add `--strict` to treat warnings as errors. Every rule has a seeded-fault test in `crates/sim-db/tests/db.rs`.

| Rule | Severity | Checks |
|---|---|---|
| `provenance.citation_missing` | error | Empty `source_citation` |
| `provenance.created_by_missing` | error | Empty `created_by` |
| `provenance.updated_at_format` | error | `updated_at` is not `YYYY-MM-DD` |
| `classification.open_build` | error | Any record not marked `UNCLASSIFIED` in the open build |
| `provenance.not_generic` | error | A specific (non-generic) record whose citation has no URL, `doi:` or ISBN: a specific record must cite its open source |
| `fk.dangling` | error | Any reference to a missing row |
| `sensor.beyond_horizon` | error | A radar, ESM, IR, EO or visual sensor's max range exceeds the 4/3-earth radar horizon between its mount and a target at the sensor's highest covered altitude. Airborne mounts are assessed at the platform ceiling |
| `sensor.range_order` | error | `0 ≤ min_range < max_range` |
| `sensor.reference_range` | error | Radar reference range and RCS must be given together, both positive, with reference range ≤ max range |
| `sensor.radar_fields` | error | `clutter_db_per_sea_state` (≥ 0) and `beamwidth_deg` (0 < b ≤ 360) only on radars |
| `ecm.values` | error | ECM strength outside 0–100 dB J/N at 1 km |
| `sensor.sonar_fields` | error | Sonar figures only on sonars (hull, towed, dipping, sonobuoy), within frequency 0.01–100 kHz, DI 0–40 dB, DT −10–30 dB, source level 150–250 dB, self-noise 20–120 dB |
| `sensor.radar_defaults` | warning | A radar without `clutter_db_per_sea_state` or `beamwidth_deg`; the radar model's defaults (1.5 dB, 2°) apply |
| `sensor.radar_reference` | error | A radar without a reference range and RCS (needed by the Phase 2 detection model) |
| `weapon.range_order` | error | `0 ≤ min_range < max_range` |
| `weapon.base_ph` | error | Base Ph outside [0, 1] |
| `weapon.values` | error | Negative speed or warhead |
| `weapon.target_domains` | error | Unknown or empty target domains |
| `weapon.no_launcher` | error | A magazine holds a weapon that no launcher on the same platform can fire |
| `weapon.unused` | warning | A weapon fitted to no launcher and no loadout |
| `launcher.empty` | warning | A launcher that lists no weapons |
| `launcher.no_ammunition` | warning | A launcher can fire a weapon no magazine on the platform holds |
| `magazine.capacity` / `magazine.quantity` | error | Capacity or quantity ≤ 0 |
| `magazine.over_capacity` | error | Contents exceed capacity |
| `loadout.quantity` | error | Loadout quantity ≤ 0 |
| `loadout.no_hardpoints` / `loadout.over_hardpoints` | error | A loadout on a platform without hardpoints, or one exceeding them |
| `platform.speed` | error | Speeds must satisfy `0 ≤ cruise ≤ max` |
| `platform.accel` | error | A mobile platform with acceleration ≤ 0 |
| `platform.dimensions` | error | Non-positive length, displacement, draft, endurance; negative hardpoints |
| `platform.depth` | error | A submarine needs a max depth; only submarines may have one |
| `platform.altitude` | error | An aircraft needs a ceiling; only aircraft may have one |
| `platform.category_domain` | error | Category inconsistent with domain (e.g. a submarine not subsurface) |
| `damage.fractions` | error | Damage points ≤ 0 or fractions outside [0, 1] |
| `signature.value` | error | Negative signature value |
| `platform.no_rcs` / `platform.no_acoustic` | warning | Missing signature; detection will fall back to a default |

## Regional database (Phase 4 slice 13)

Specific classes, for the Philippine Area of Responsibility ([ADR-0007](../adr/0007-phase-4-region-and-data.md)) and the Gate 4 engagements ([DL-0048](../design-log/DL-0048-regional-database.md)). Open sources only, UNCLASSIFIED.

| File | Contents |
|---|---|
| `platforms/regional-ph.json` | Philippine Navy: `PHL-FF-RIZAL`, `PHL-FFG-MALVAR`, `PHL-PS-DELPILAR`, `PHL-PG-ACERO`. Philippine Coast Guard: `PHL-CG-MAGBANUA`, `PHL-CG-PAROLA` |
| `platforms/regional-cn.json` | PLAN: `CHN-FFG-054A`, `CHN-DDG-052D`, `CHN-FS-056A`, `CHN-SSK-039A`. China Coast Guard: `CHN-CG-ZHAOTOU`, `CHN-CG-056` |
| `platforms/historical.json` | Gate 4: `PAK-SSK-HANGOR`, `IND-FF-KHUKRI`, `IRN-FF-SAAM`, `IRN-PTG-KAMAN`, `USA-CG-BELKNAP`, `USA-FFG-OHP`, `USA-FF-KNOX`, `USA-A6E` |
| `weapons.json` | Named weapons, added where the records need them: C-802A, YJ-83, YJ-18, HHQ-16, HHQ-9, Mistral, VL MICA, Spike NLOS, Harpoon 1, SM-1MR, Skipper, Sea Killer 2, Limbo |
| `reference.json` | Nations and services (PHL, CHN, USA, IRN, PAK, IND), and ECM records (generic ship and aircraft jammers; the Type 054A's and 052D's) |

How the values were set:

- **Published figures** (dimensions, displacement, speeds, endurance, fits, weapon ranges) come from the cited source, confidence C.
- **Signatures** are not published. They are copied from the matching generic class and carry their own provenance, confidence D.
- **Sensors:** a named radar or sonar uses the generic record of its kind; the record's citation names the real system.
- **Weapon Ph** is the generic class's, confidence D. Jammer strengths are estimates, D.
- **Each record cites its source and the date accessed.** Records without an open source (for example the Type 818 cutter) were left out.

## Terrain and bathymetry sources (Phase 4 slice 2, ADR-0007)

| Source | Use | Terms | Credit to show |
|---|---|---|---|
| GEBCO 2020 Grid, 15 arc-seconds, served by NOAA CoastWatch ERDDAP (`griddap/GEBCO_2020`) | Elevation of sea floor and land: the Philippine-area pack (3–27°N, 111–137°E, full resolution) and the coarse global layer | Free to use and redistribute; not for legal use (it may contain inaccuracies); no warranty (dataset licence text) | "GEBCO Compilation Group (2020) GEBCO 2020 Grid (doi:10.5285/a29c5465-b138-234d-e053-6c86abc040b9)" |
| SRTM15+ (`griddap/srtm15plus`) | Land heights inside GEBCO (GEBCO's land is SRTM15+); a cross-check only | Public domain (US Government) | "SRTM15+ (Tozer et al., 2019)" |

- **Not in the repository.** The raw downloads and the built packs live outside it (`%USERPROFILE%\nws-data\`), and the packs ship as separate files with SHA-256 sums.
- **Not for navigation.** Depths are for planning at TTX scale, not for navigation, as the GEBCO licence says.

**Downloads and built packs (2026-11-13).** Each pack is built with `nws-batch terrain build` ([model](../models/terrain-and-bathymetry.md)). Every file is checked by SHA-256.

| File | From | SHA-256 |
|---|---|---|
| `gebco2020_par_15s.nc` (72.0 MB) | `https://coastwatch.pfeg.noaa.gov/erddap/griddap/GEBCO_2020.nc?elevation[(3.0):1:(27.0)][(111.0):1:(137.0)]` | `59ceadfa2e005a6b35815926cededc48d45d2e86cf5e1fa84a4554db8a8555b0` |
| `gebco2020_global_2m.nc` (116.8 MB) | `…/GEBCO_2020.nc?elevation[(-89.99):8:(89.99)][(-179.99):8:(179.99)]` (every 8th cell: 2′) | `5b7c6f80a663d8219a68c061fcede83df69966c91997e01bb4843d7029191077` |
| `gebco2020-par-15s.nwsterrain` (30.5 MB) | `terrain build gebco2020_par_15s.nc --name gebco2020-par-15s` | `0dfd348c955e5214368321fcc1ae3ebffe33112ad6a4c285fee6f4f6f16498ff` |
| `gebco2020-global-4m.nwsterrain` (17.5 MB) | `terrain build gebco2020_global_2m.nc --name gebco2020-global-4m --average 2` | `c2db90ca0eecfa6a232ecb2798b9e083cc119971fbed4df0cdb25c27228653ad` |

The build is reproducible: the same download and command give the same pack, byte for byte, and so the same id.
