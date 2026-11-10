# Gate 2: duel-test index

Gate 2 ("duel tests pass", master prompt §13) asks for a duel test against hand calculation for every model that affects detection or combat (§12). This page lists them. All run in CI on every change (`cargo test --workspace`, Windows and Linux), together with the determinism matrix that compares state hashes across operating systems.

- **Statistical duels:** run many seeded, independent engagements and require the observed rate to lie within **4 standard deviations** of the hand-calculated binomial mean.
- **Hand-value tests:** check each formula against numbers worked by hand in its model doc.

## Statistical duels

| Model | Test | Hand calculation | Runs | Tolerance |
|---|---|---|---|---|
| Radar Pd (Gate 1) | `duel_at_r50_detects_half_the_time` | Pd = 1/(1 + 1⁸) = 0.5 | 4000 scans | 4σ |
| Radar Pd | `duel_at_eight_tenths_r50_matches_hand_value` | 1/(1 + 0.8⁸) = 0.8563 | 2000 | 4σ |
| Radar Pd | `duel_at_one_point_two_r50_matches_hand_value` | 1/(1 + 1.2⁸) = 0.1887 | 2000 | 4σ |
| Missile engagement | `duel_hit_rate_matches_acquisition_times_ph` | 0.9 seeker × 0.8 Ph = 0.72 | 300 | 4σ |
| Mines | `mine_transit_risk_matches_the_simple_initial_threat` | 1 − exp(−20/(π·2000²)·100·4000) = 0.471 | 200 | 4σ |

## Hand-value tests by model

| Model (doc) | Tests | Key hand values |
|---|---|---|
| Geodesy and movement ([geodesy-and-movement.md](../models/geodesy-and-movement.md)) | `sim-geo` Vincenty tests; movement tests | Geoscience Australia reference line to 1 mm |
| Radar horizon ([radar-horizon.md](../models/radar-horizon.md)) | `sim-geo` horizon tests | 4.12 (√25 + √10) = 33.63 km |
| Tracks ([tracks.md](../models/tracks.md)) | `polar_covariance_hand_values`, `fusion_hand_values`, `growth_adds_reach_squared`, `first_detection_reports_the_single_measurement_error` | 174.5 m cross-range at 20 km; P' = 0.8 |
| C2 ([c2-doctrine.md](../models/c2-doctrine.md)) | `release_rule_truth_table`, `most_specific_level_wins_per_field` | full WCS × affiliation table |
| Radar clutter, jamming ([radar.md](../models/radar.md)) | `clutter_and_weather_scale_range_by_tenth_power_over_forty`, `jamming_hand_values`, `burn_through_hand_value`, engine radar tests | ×0.7718 at 4.5 dB; burn-through 2.50 km |
| ESM ([esm.md](../models/esm.md)) | `intercept_range_hand_values`, `two_perpendicular_bearings_fix_hand_value`, `identification_ladder` | fix σ 246.8 m |
| Sonar ([sonar.md](../models/sonar.md)) | `thorp_and_transmission_loss_hand_values`, `passive_towed_array_against_a_quiet_submarine`, `active_hull_sonar_at_ten_km`, `passive_towed_array_matches_the_sonar_equation` | SE −10.57 dB passive; +24.46 dB active |
| Visual, EO/IR, MAD ([optical-mad.md](../models/optical-mad.md)) | `sun_elevation_hand_values`, `optical_r50_hand_values`, `identification_bands_and_mad`, engine tests | 21 km by day, 2.1 km at night; MAD slant 650 m |
| Weapons, Ph, damage ([weapons-damage.md](../models/weapons-damage.md)) | `ph_hand_values`, `frigate_hand_values`, `a_missile_flies_acquires_and_rolls_its_hand_calculated_ph`, `a_hit_mission_kills_a_frigate_and_destroys_a_small_ship`, `fires_burn_until_damage_control_puts_them_out` | Ph 0.4536; 220/450 DP = mission kill |
| Point defence, soft kill | `point_defence_rules_hand_values`, `sams_engage_incoming_missiles_with_the_hand_calculated_ph`, `two_sam_channels_limit_a_four_missile_raid`, `decoys_and_jamming_lower_ph_at_impact` | intercept Ph 0.675; Ph 0.448 with chaff and jamming |
| Mines | `simple_initial_threat_hand_values` | P 0.4710 crossing; 0.01579 per step |
| Missions ([missions.md](../models/missions.md)) | `patrol_points_hand_values`, `escort_station_and_speed`, `automated_engagement_follows_the_release_rule` | 10 km due east; station on 135° |

## Whole-system checks

| Check | What it shows |
|---|---|
| `gate1_demo_units_move_and_detect` | Gate 1 scenario: units move and both sides detect |
| `gate2_demo_detects_engages_defends_and_damages` | Gate 2 scenario: both sides fire; point defence and decoys engage; hits do damage; a unit is destroyed; ships run the minefield risk; the neutral is never engaged |
| `golden_replays` | every shipped scenario reproduces its recorded final state hash |
| Determinism matrix (CI) | the same hashes on Windows and Linux for 5 seeds of every scenario |
| Save/resume tests | saving mid-run (including with missiles in flight) and resuming matches an uninterrupted run bit for bit |

## The Gate 2 demonstration scenario

`data/scenarios/phase2-surface-action.json`: Blue (air-defence destroyer and multi-role frigate) against Red (two missile corvettes) in Bass Strait by day, both weapons free on patrol and escort missions, with a Red minefield and a neutral merchant clear to the west. With its own seed (golden hash recorded in `tests/golden/`):

- **Weapons fired:** both sides fire anti-ship missiles, 24 in all.
- **Point defence:** layered point defence (86 interceptor shots, 8 decoys) defeats every one.
- **Gunfire:** Blue's destroyer closes and sinks both corvettes by gunfire inside an hour.
- **Damage:** fires break out and are fought down along the way.
- **Minefield:** Blue's ships take 103 minefield transit draws without an encounter.

These numbers come from one seed; other seeds play out differently, and each is reproducible exactly.
