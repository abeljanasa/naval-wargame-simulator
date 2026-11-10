-- 0002: radar fields for the Phase 2 radar model (docs/models/radar.md).
-- Both are nullable: only radars use them, and the model has documented defaults when absent.

-- Loss in signal-to-clutter, dB, per sea-state step, for targets low enough to compete with
-- surface clutter. Range scales by 10^(-loss/40).
ALTER TABLE sensors ADD COLUMN clutter_db_per_sea_state REAL;

-- Two-way half-power beamwidth, degrees. A jammer inside half of it is in the main lobe.
ALTER TABLE sensors ADD COLUMN beamwidth_deg REAL;
