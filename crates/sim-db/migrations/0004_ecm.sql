-- 0004: ECM equipment as database records (Phase 4 slice 13, docs/db/data-dictionary.md).
-- A platform's jammers; a unit spawned from the platform jams with the strongest unless its
-- scenario gives it a jammer of its own.

CREATE TABLE ecm_systems (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    kind            TEXT NOT NULL CHECK (kind IN ('noise_jammer','deception_jammer')),
    -- Jammer-to-noise ratio it produces in a victim receiver 1 km away, dB (radar and comms).
    jn_db_at_1km    REAL NOT NULL,
    source_citation TEXT NOT NULL,
    confidence      TEXT NOT NULL CHECK (confidence IN ('A','B','C','D','E')),
    classification  TEXT NOT NULL,
    is_generic      INTEGER NOT NULL CHECK (is_generic IN (0,1)),
    created_by      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
) STRICT;

CREATE TABLE platform_ecm (
    platform_id TEXT NOT NULL REFERENCES platforms(id),
    ecm_id      TEXT NOT NULL REFERENCES ecm_systems(id),
    PRIMARY KEY (platform_id, ecm_id)
) STRICT;
