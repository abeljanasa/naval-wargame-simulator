-- Migration 0001: initial platform database schema.
-- Forward-only. Never edit a migration once it is merged; add a new one instead.
--
-- Conventions:
--   * SI units in every column (m, m/s, m/s², kg, t, s). Column suffixes state the unit.
--   * Every table holding performance values carries the provenance columns:
--       source_citation, confidence (A–E), classification, is_generic, created_by, updated_at.
--     Link rows (launcher_weapons, platform_comms, magazine_contents, loadout_items) take the
--     provenance of their parent row.
--   * Classification is not constrained here: the accredited database (separate file and
--     pipeline) shares this schema. The open build is policed by `sim-db validate`.

CREATE TABLE nations (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    source_citation TEXT NOT NULL,
    confidence      TEXT NOT NULL CHECK (confidence IN ('A','B','C','D','E')),
    classification  TEXT NOT NULL,
    is_generic      INTEGER NOT NULL CHECK (is_generic IN (0,1)),
    created_by      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
) STRICT;

CREATE TABLE services (
    id              TEXT PRIMARY KEY,
    nation_id       TEXT NOT NULL REFERENCES nations(id),
    name            TEXT NOT NULL,
    branch          TEXT NOT NULL CHECK (branch IN ('navy','air_force','army','coast_guard','civil')),
    source_citation TEXT NOT NULL,
    confidence      TEXT NOT NULL CHECK (confidence IN ('A','B','C','D','E')),
    classification  TEXT NOT NULL,
    is_generic      INTEGER NOT NULL CHECK (is_generic IN (0,1)),
    created_by      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
) STRICT;

CREATE TABLE propulsion (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    kind            TEXT NOT NULL CHECK (kind IN ('nuclear','gas_turbine','codog','codag','diesel',
                        'diesel_electric','aip','turbofan','turboprop','turboshaft','none')),
    fuel_type       TEXT NOT NULL CHECK (fuel_type IN ('marine_diesel','jet','nuclear','none')),
    source_citation TEXT NOT NULL,
    confidence      TEXT NOT NULL CHECK (confidence IN ('A','B','C','D','E')),
    classification  TEXT NOT NULL,
    is_generic      INTEGER NOT NULL CHECK (is_generic IN (0,1)),
    created_by      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
) STRICT;

CREATE TABLE damage_profiles (
    id                        TEXT PRIMARY KEY,
    name                      TEXT NOT NULL,
    damage_points             REAL NOT NULL,
    -- Fraction of damage points lost at which the unit is mission-killed (0..1).
    mission_kill_fraction     REAL NOT NULL,
    fire_susceptibility       REAL NOT NULL,   -- 0..1
    flooding_susceptibility   REAL NOT NULL,   -- 0..1
    source_citation TEXT NOT NULL,
    confidence      TEXT NOT NULL CHECK (confidence IN ('A','B','C','D','E')),
    classification  TEXT NOT NULL,
    is_generic      INTEGER NOT NULL CHECK (is_generic IN (0,1)),
    created_by      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
) STRICT;

CREATE TABLE comms_systems (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    kind            TEXT NOT NULL CHECK (kind IN ('hf','vhf','uhf','satcom','datalink',
                        'underwater_telephone','vlf')),
    range_m         REAL NOT NULL,
    is_datalink     INTEGER NOT NULL CHECK (is_datalink IN (0,1)),
    source_citation TEXT NOT NULL,
    confidence      TEXT NOT NULL CHECK (confidence IN ('A','B','C','D','E')),
    classification  TEXT NOT NULL,
    is_generic      INTEGER NOT NULL CHECK (is_generic IN (0,1)),
    created_by      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
) STRICT;

CREATE TABLE sensors (
    id                    TEXT PRIMARY KEY,
    name                  TEXT NOT NULL,
    kind                  TEXT NOT NULL CHECK (kind IN ('radar','sonar_hull','sonar_towed',
                              'sonar_dipping','sonobuoy','esm','ir','eo','visual','mad')),
    role                  TEXT NOT NULL,
    band                  TEXT,
    is_active             INTEGER NOT NULL CHECK (is_active IN (0,1)),
    min_range_m           REAL NOT NULL,
    max_range_m           REAL NOT NULL,
    -- Radar: detection range against a target of ref_target_rcs_m2 (range ∝ RCS^¼ from here).
    ref_detect_range_m    REAL,
    ref_target_rcs_m2     REAL,
    -- Highest target altitude the sensor is designed to cover (used for the horizon check).
    max_target_altitude_m REAL NOT NULL,
    scan_period_s         REAL,
    source_citation TEXT NOT NULL,
    confidence      TEXT NOT NULL CHECK (confidence IN ('A','B','C','D','E')),
    classification  TEXT NOT NULL,
    is_generic      INTEGER NOT NULL CHECK (is_generic IN (0,1)),
    created_by      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
) STRICT;

CREATE TABLE weapons (
    id                 TEXT PRIMARY KEY,
    name               TEXT NOT NULL,
    kind               TEXT NOT NULL CHECK (kind IN ('sam','asm','lacm','torpedo','gun','ciws',
                           'aam','asw_rocket','mine','depth_charge','decoy','bomb')),
    guidance           TEXT NOT NULL,
    min_range_m        REAL NOT NULL,
    max_range_m        REAL NOT NULL,
    speed_mps          REAL NOT NULL,
    warhead_kg         REAL NOT NULL,
    base_ph            REAL NOT NULL,      -- 0..1, before modifiers (Phase 2 model)
    -- Comma-separated subset of: surface,subsurface,air,land
    target_domains     TEXT NOT NULL,
    source_citation TEXT NOT NULL,
    confidence      TEXT NOT NULL CHECK (confidence IN ('A','B','C','D','E')),
    classification  TEXT NOT NULL,
    is_generic      INTEGER NOT NULL CHECK (is_generic IN (0,1)),
    created_by      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
) STRICT;

CREATE TABLE platforms (
    id                 TEXT PRIMARY KEY,
    name               TEXT NOT NULL,
    category           TEXT NOT NULL CHECK (category IN ('surface_combatant','submarine',
                           'auxiliary','merchant','mpa','aew','tanker','fighter','helicopter',
                           'uav','shore_battery','sensor_site','airbase','port')),
    domain             TEXT NOT NULL CHECK (domain IN ('surface','subsurface','air','land')),
    service_id         TEXT NOT NULL REFERENCES services(id),
    propulsion_id      TEXT NOT NULL REFERENCES propulsion(id),
    damage_profile_id  TEXT NOT NULL REFERENCES damage_profiles(id),
    length_m           REAL,
    displacement_t     REAL,               -- full load (ships) / max take-off mass (aircraft)
    draft_m            REAL,
    max_speed_mps      REAL NOT NULL,
    cruise_speed_mps   REAL NOT NULL,
    accel_mps2         REAL NOT NULL,
    endurance_m        REAL,               -- range at cruise speed
    max_altitude_m     REAL,               -- aircraft service ceiling
    max_depth_m        REAL,               -- submarines: maximum operating depth (positive down)
    hardpoints         INTEGER,            -- aircraft: weapon stations available to loadouts
    source_citation TEXT NOT NULL,
    confidence      TEXT NOT NULL CHECK (confidence IN ('A','B','C','D','E')),
    classification  TEXT NOT NULL,
    is_generic      INTEGER NOT NULL CHECK (is_generic IN (0,1)),
    created_by      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
) STRICT;

CREATE TABLE platform_signatures (
    platform_id     TEXT NOT NULL REFERENCES platforms(id),
    kind            TEXT NOT NULL CHECK (kind IN ('rcs','ir','acoustic','visual')),
    aspect          TEXT NOT NULL CHECK (aspect IN ('all','front','side','rear')),
    speed_band      TEXT NOT NULL CHECK (speed_band IN ('any','slow','cruise','max')),
    value           REAL NOT NULL,
    unit            TEXT NOT NULL CHECK (unit IN ('m2','db_re_1upa','kw_sr','m')),
    source_citation TEXT NOT NULL,
    confidence      TEXT NOT NULL CHECK (confidence IN ('A','B','C','D','E')),
    classification  TEXT NOT NULL,
    is_generic      INTEGER NOT NULL CHECK (is_generic IN (0,1)),
    created_by      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    PRIMARY KEY (platform_id, kind, aspect, speed_band)
) STRICT;

-- A fitted item: a sensor or a weapon launcher.
CREATE TABLE mounts (
    id              INTEGER PRIMARY KEY,
    platform_id     TEXT NOT NULL REFERENCES platforms(id),
    name            TEXT NOT NULL,
    kind            TEXT NOT NULL CHECK (kind IN ('sensor','launcher')),
    sensor_id       TEXT REFERENCES sensors(id),
    -- Height above the waterline / ground (ships, sites) or above the airframe datum (aircraft).
    height_m        REAL NOT NULL,
    source_citation TEXT NOT NULL,
    confidence      TEXT NOT NULL CHECK (confidence IN ('A','B','C','D','E')),
    classification  TEXT NOT NULL,
    is_generic      INTEGER NOT NULL CHECK (is_generic IN (0,1)),
    created_by      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    CHECK ((kind = 'sensor') = (sensor_id IS NOT NULL))
) STRICT;

-- Which weapons a launcher mount can fire.
CREATE TABLE launcher_weapons (
    mount_id   INTEGER NOT NULL REFERENCES mounts(id),
    weapon_id  TEXT NOT NULL REFERENCES weapons(id),
    PRIMARY KEY (mount_id, weapon_id)
) STRICT;

CREATE TABLE magazines (
    id              INTEGER PRIMARY KEY,
    platform_id     TEXT NOT NULL REFERENCES platforms(id),
    name            TEXT NOT NULL,
    capacity        INTEGER NOT NULL,      -- rounds / cells / weapons
    source_citation TEXT NOT NULL,
    confidence      TEXT NOT NULL CHECK (confidence IN ('A','B','C','D','E')),
    classification  TEXT NOT NULL,
    is_generic      INTEGER NOT NULL CHECK (is_generic IN (0,1)),
    created_by      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
) STRICT;

-- Default load of a magazine.
CREATE TABLE magazine_contents (
    magazine_id  INTEGER NOT NULL REFERENCES magazines(id),
    weapon_id    TEXT NOT NULL REFERENCES weapons(id),
    quantity     INTEGER NOT NULL,
    PRIMARY KEY (magazine_id, weapon_id)
) STRICT;

-- Aircraft weapon loadouts, limited by platforms.hardpoints.
CREATE TABLE loadouts (
    id              TEXT PRIMARY KEY,
    platform_id     TEXT NOT NULL REFERENCES platforms(id),
    name            TEXT NOT NULL,
    role            TEXT NOT NULL,
    source_citation TEXT NOT NULL,
    confidence      TEXT NOT NULL CHECK (confidence IN ('A','B','C','D','E')),
    classification  TEXT NOT NULL,
    is_generic      INTEGER NOT NULL CHECK (is_generic IN (0,1)),
    created_by      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
) STRICT;

CREATE TABLE loadout_items (
    loadout_id  TEXT NOT NULL REFERENCES loadouts(id),
    weapon_id   TEXT NOT NULL REFERENCES weapons(id),
    quantity    INTEGER NOT NULL,
    PRIMARY KEY (loadout_id, weapon_id)
) STRICT;

CREATE TABLE platform_comms (
    platform_id  TEXT NOT NULL REFERENCES platforms(id),
    comms_id     TEXT NOT NULL REFERENCES comms_systems(id),
    PRIMARY KEY (platform_id, comms_id)
) STRICT;

CREATE INDEX idx_mounts_platform ON mounts(platform_id);
CREATE INDEX idx_magazines_platform ON magazines(platform_id);
CREATE INDEX idx_loadouts_platform ON loadouts(platform_id);
CREATE INDEX idx_platforms_category ON platforms(category);
