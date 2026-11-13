# DL-0041: Choke points inherit depth and lanes

- **Date:** 2026-11-13
- **Phase:** 4, slice 5
- **Sources:**
  - Master prompt §7 (choke points).
  - The [Phase 4 plan](../plan/phase-4.md), slice 5.
  - ADR-0007 (data licensing: lanes from traffic data only where a licensed source exists).
  - Original design otherwise.

## Decisions

- **Lanes are planning-layer data, not a model.** A lane is a band with a name, a width and optionally a traffic figure. Ships are not forced into lanes: routing is unchanged. Lanes inform the white cell, the map and the transit assessment.
- **Traffic only with a source.** The engine refuses a traffic figure without a named source. The importer drops one, and says so. So unlicensed traffic data cannot enter quietly, and the planner's own figures are labelled as estimates. No traffic data is bundled.
- **Transit risk is a three-level indicator.** The levels come from traffic density and under-keel margin, with stated thresholds. A probability would claim a precision the open data cannot support. The thresholds are original design, for discussion, and can be tuned with the client.
- **Assessment as a separate event.** `TransitAssessed` follows `ChokePointTransit` only when there are lanes or terrain packs. Events are not hashed, and the lanes field of the planning layer is omitted when empty, so scenarios without lanes keep their state hashes and golden replays.
- **The depth section runs square to the lane.** Where there is a lane, its first-to-last bearing sets the section. Without one, the section runs across the zones' narrower way. The navigable width for a draft is computed in the app from the section, so the white cell can try drafts without a round trip.
- **The pilots are unchanged.** The Gate 3 pilot scenarios get no lanes, so their golden replays stay as rehearsed.

## Open

- Lanes in the scenario editor. The editor keeps them through an edit, but drawing and editing them is in the live Planning tab only.
- Traffic as moving merchant units generated from lane density (Phase 5, advanced AI).
