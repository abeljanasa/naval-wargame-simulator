# DL-0004: Desktop map, unit symbols, time controls, seats

- **Date:** 2026-11-10
- **Sources:**
  - The time-compression steps (pause, 1×, 2×, 5×, 15×, 30×, 60×, max) come from master prompt §10.
  - The unit frames follow the public APP-6 / MIL-STD-2525 convention: friend circle, hostile diamond, neutral square, unknown quatrefoil, for sea surface. They are drawn from scratch in `apps/desktop/src/lib/symbols.ts`.
  - The coastline is Natural Earth 1:50m land, which is public domain.
  - The layout, colours, keyboard shortcuts and route-planning interaction are original design. No other product's UI was referenced.
- **Phase 1 interim:** postures don't exist yet (Phase 2), so the viewing seat's own side is drawn as *friend* and every other side as *unknown*.
- **Pacing:** wall-clock pacing lives only in `apps/desktop/src-tauri/src/host.rs`. It decides how many fixed ticks to run and caps the credit for a long stall at 0.25 s.
