-- 0003: acoustic figures of sonars (Phase 4 slice 8, docs/models/acoustics.md).
-- All nullable: only sonars use them, and the model's generic figures by kind apply when absent.

-- Centre frequency of the processing band, kHz (absorption, bottom loss).
ALTER TABLE sensors ADD COLUMN sonar_frequency_khz REAL;
-- Directivity index, dB.
ALTER TABLE sensors ADD COLUMN sonar_di_db REAL;
-- Detection threshold, dB.
ALTER TABLE sensors ADD COLUMN sonar_dt_db REAL;
-- Active source level, dB re 1 uPa at 1 m (active sonars).
ALTER TABLE sensors ADD COLUMN sonar_source_level_db REAL;
-- Own-ship self-noise at 10 kn, dB.
ALTER TABLE sensors ADD COLUMN sonar_self_noise_db REAL;
