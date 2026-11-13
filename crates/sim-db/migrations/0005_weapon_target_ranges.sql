-- 0005: a weapon's range against one target domain, where it differs from its maximum range
-- (Gate 4 finding: a gun reaches aircraft at a fraction of its surface range;
-- docs/db/data-dictionary.md, DL-0051).

CREATE TABLE weapon_target_ranges (
    weapon_id       TEXT NOT NULL REFERENCES weapons(id),
    target          TEXT NOT NULL CHECK (target IN ('surface','subsurface','air','land')),
    max_range_m     REAL NOT NULL,
    source_citation TEXT NOT NULL,
    confidence      TEXT NOT NULL CHECK (confidence IN ('A','B','C','D','E')),
    classification  TEXT NOT NULL,
    is_generic      INTEGER NOT NULL CHECK (is_generic IN (0,1)),
    created_by      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    PRIMARY KEY (weapon_id, target)
) STRICT;
