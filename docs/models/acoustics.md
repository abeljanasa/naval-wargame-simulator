# Model: Ocean acoustics: sound-velocity profiles, layers, convergence zones

> Phase 4 slice 8. Textbook ray acoustics, simplified to closed forms, with generic values; **not SME-validated**. It refines the transmission loss of the [sonar equations](sonar.md); those equations, the noise model and the Pd law are unchanged.

- **Code:**
  - `crates/sim-sensors/src/acoustics.rs`: profile, paths, convergence zone, reverberation.
  - `crates/sim-core/src/acoustics.rs`: profiles in force, towed arrays, sonobuoys.
  - `crates/sim-core/src/sonar.rs`: the sonar pass that uses them.
- **Tests:**
  - Unit tests in `sim-sensors` (hand calculations).
  - Engine tests in `crates/sim-scenario/tests/acoustics.rs`.
  - Front end: `apps/desktop/src/lib/acoustics.test.ts`.
- **Design log:** [DL-0044](../design-log/DL-0044-acoustics.md)

## Sound-velocity profiles

A profile is sound speed against depth: points from the surface (depth 0) down, speeds 1,400–1,600 m/s, linear between points. Below the last point, speed rises with pressure at 0.017 s⁻¹.

- **Where and when:** a profile has an optional area (circle or polygon, as for weather zones) and optional months (1–12, UTC). Where several apply, the one with the highest id wins.
- **With no profile in force, the Phase 2 law holds unchanged**: spherical to 5 km, cylindrical beyond, plus Thorp absorption. Scenarios without profiles keep their results and hashes.
- **Set by:**
  - the scenario (`sound_profiles`);
  - the white cell (Planning tab, import a CSV of depth and speed): a logged `SetSoundProfiles` order.
- **The mixed layer** is the depth of the greatest speed in the top 500 m, when speed rises from the surface to it. If speed falls from the surface, there is no layer.

## Paths

One-way transmission loss is the **least** of the paths open between source and receiver. Each includes Thorp absorption α(f)·r.

| Path | When | TL |
|---|---|---|
| **Surface duct** | both ends in the layer | 20·log₁₀ s + 10·log₁₀(r/s) beyond the skip distance s = √(8·R·H). R = c₀ / 0.0163 is the ray radius in an isothermal layer; H is the layer depth |
| **Direct** | otherwise, within the direct-path reach | 20·log₁₀ r |
| **Cross-layer** | one end in the layer, one below, within reach | 20·log₁₀ r + 10 dB |
| **Shadow** | beyond the direct-path reach | the direct or cross-layer loss + 30 dB |
| **Convergence zone** | within ±3 % of k × the CZ range, k = 1, 2, 3 | 20·log₁₀ r − 10 dB (focusing gain) |
| **Bottom bounce** | water depth known (terrain pack) and ≥ 50 m | 20·log₁₀ ℓ + 10 dB bottom loss (+5 dB above 5 kHz). ℓ = √(r² + (2d − z_s − z_r)²) |

- **Direct-path reach:** each end's horizon on a ray circle of radius c / |g|, with g the gradient just below the layer: √(2·R·Δz_s) + √(2·R·Δz_r). Δz is the end's distance from the layer depth, at least 10 m. If speed does not fall below the layer, the reach is unlimited.
- **The convergence-zone range** comes from tracing the limiting ray. It leaves the layer depth (or the surface) horizontally and is traced through each linear segment in closed form. A segment of gradient g is an arc of radius c/|g|, so its horizontal run is √(c_top² − c_bottom²)/|g|. The ray goes down until speed again equals the starting speed, then back up. The CZ range is twice the down-going run. **There is no CZ** if the sea floor is shallower than the turning depth, or if the profile never regains the starting speed.

Every sonar attempt with a profile in force logs the path it took (`SonarAttempt.path`).

## Depths

| Who | Depth |
|---|---|
| Hull sonar on a ship | 6 m |
| Ship as a noise source or echo | 5 m |
| Submarine (sensor or target) | its depth, at least 5 m |
| Towed array on a ship | 150 m |
| Towed array on a submarine | 50 m below the boat |
| Dipping sonar | 100 m |
| Sonobuoy | 60 m, or 300 m if laid deep |

## Reverberation (active sonar)

With a profile in force, an active sonar's echo is masked by noise and reverberation together: SE = SL − 2·TL + TS − ((NL − DI) ⊕ RL) − DT. ⊕ is the power sum.

RL = SL − 2·TL + S + 10·log₁₀(r·θ·c·τ/2), with beamwidth θ = 0.2 rad, pulse τ = 0.1 s and c = 1,500 m/s. The scattering strength S depends on the water:
- in water under 200 m deep, the sea floor scatters: S = −30 dB;
- otherwise the surface does: S = −50 + 3 × sea state.

## Towed arrays

- A towed array is **streamed** at the start of a scenario.
- Streaming or recovering takes **10 minutes**. During that time the array does not listen, and a recovered array does not listen at all.
- It listens only with its unit at or below **15 kn**.
- Order it from the unit panel (*Towed array: Recover / Stream*). The order is logged (`StreamTowedArray`, event `TowedArray`).

## Sonobuoys

- **Who carries them:** units with a sonobuoy sensor (maritime patrol aircraft, ASW helicopters), 16 each.
- **Laying:** a *Lay sonobuoys* order (`LaySonobuoys`, 1–64 points) sends the unit through the points. It drops one buoy within 1 km of each point. Hydrophones go to 60 m, or to 300 m if laid deep.
- **Listening:** a buoy listens passively for its side for **4 hours**, with the sonobuoy's generic figures (1 kHz, DT 10 dB) and a 3 dB directivity index. Each buoy reports a bearing from where it floats, so bearings from two buoys cross into a fix, as ESM bearings do ([esm.md](esm.md)).
- **Seeing them:** a side sees its own buoys on the map, each with its life left; the white cell sees all.
- Buoys do not drift (the open items below).

## Database figures

A sonar may carry its own figures in the database (migration 0003):
- frequency (kHz);
- DI, DT (dB);
- active source level (dB);
- self-noise at 10 kn (dB).

Any figure it has replaces the generic figure of its kind ([sonar.md](sonar.md)). Rule `sensor.sonar_fields` keeps them on sonars, within plausible bounds.

## Worked examples

All use one profile: 1,500 m/s at the surface, 1,501 at 60 m, 1,480 at 1,000 m, 1,515 at 4,000 m.

| Case | Working | Result |
|---|---|---|
| Frigate's hull sonar (6 m) on a submarine at 200 m, 5 km | One end in the layer, one below. R = 1,480/0.0223 ≈ 66 km; reach ≈ √(2R·54) + √(2R·140) ≈ 7 km | **cross-layer**: 20·log₁₀ 5000 + 10 dB + α·r |
| The same at 9 km | beyond the 7 km reach | **shadow**: a further 30 dB |
| Towed array (150 m) on the same submarine at 5 km | both below the layer; reach ≈ 7.8 km | **direct** |

The hand calculation for the CZ and the duct is in the `sim-sensors` tests: CZ 54.4 km for a 60 m layer over a 1,000 m channel axis.

## Verification

| Test | Checks |
|---|---|
| `profile_shape` | speeds between and below points; mixed-layer depth; validation |
| `convergence_zone_matches_the_hand_calculation` | turning depth 2,823.5 m, CZ 54.4 km; no CZ over 2,000 m of water |
| `transmission_loss_by_path_matches_the_hand_calculation` | duct, CZ, shadow and bottom-bounce losses to 1e-9 dB |
| `reverberation_falls_with_range_and_rises_in_shallow_water` | RL trend and the shallow-water bottom |
| `months_from_unix_time` | the civil calendar |
| `a_profile_sends_hull_sonar_across_the_layer_and_the_towed_array_direct` | paths in the running engine; shadow beyond reach |
| `without_a_profile_the_phase_2_law_holds` | no path logged and the old law used |
| `a_recovered_array_is_deaf_until_streamed_and_ready` | the 10-minute transitions |
| `a_ship_without_an_array_cannot_stream_one` | the refusal |
| `sonobuoys_are_laid_listen_for_their_side_survive_a_save_and_expire` | laying, side visibility, listening, save round trip, 4-hour life |

## Limits

- **Range-independent:** one profile along each path (the one at the receiver). There are no fronts or eddies.
- **No ray tracing beyond the limiting ray:**
  - no caustics;
  - no half-channel or deep-sound-channel paths between deep units;
  - no surface-duct leakage as a function of frequency.
- **One bottom loss** for all sediments, and one bounce.
- **Sonobuoys do not drift**, and have no DIFAR/active types or radio-range limits.
