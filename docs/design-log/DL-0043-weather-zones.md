# DL-0043: Weather zones

- **Date:** 2026-11-13
- **Phase:** 4, slice 7
- **Sources:**
  - Master prompt §6 (environment).
  - The [Phase 4 plan](../plan/phase-4.md), slice 7.
  - ITU-R P.838 (public recommendation: specific attenuation of rain, power-law form).
  - Original design otherwise.

## Decisions

- **Zones over the scenario-wide environment, not instead of it.** The existing environment is the weather outside every zone, so every existing scenario is "one global zone" with no migration, and its hash is unchanged. Zones add rain, wind and low cloud, which exist only in zones.
- **The higher id wins where zones overlap.** It is simple, deterministic and easy to explain: the zone drawn last is on top.
- **Each effect uses the weather where it acts:**
  - clutter at the target;
  - noise at the listener;
  - Ph at the target;
  - sheltering at the unit and the facility;
  - rain and visibility along the path.

  Path effects sample 16 points, enough at zone scale (tens of kilometres) and cheap.
- **Rain loss from a generic X-band power law.** The open database has no radar frequencies, so one generic coefficient set at about 10 GHz is used. The specific attenuation follows the ITU-R P.838 power-law form. A per-radar frequency is a later refinement, once the database carries frequency bands.
- **Low cloud blocks the eye and EO/IR, not radar.** The rule is one end below the cloud base at the path's middle and the other above it. It captures the main tactical effect (aircraft above an overcast are not seen visually) without a cloud-cover model.
- **Speed penalty tied to the existing weather limit.** The same per-length limit already decides sheltering, so the two agree: slower one step before the limit, much slower at it.
  - It applies under the scenario-wide sea state too, for consistency.
  - It does not change any shipped scenario's golden replay; checked.
- **The white cell's weather is a logged order.** A change goes through `SetWeather` like any override, so a replay reproduces it.

## Open

- Wind effects on aircraft, sonobuoy drift and missile flight.
- Rain clutter on radar, and frequency-dependent rain loss.
- Weather moving over time, other than by events or the white cell.
