# ADR-0002: UI framework — Svelte 5 + TypeScript

- **Status:** Accepted
- **Date:** 2026-11-10

## Context

The desktop UI is data-dense: order-of-battle trees, unit and contact panels, editors, logs, and a white-cell console. It receives snapshot diffs at up to 30 Hz and must stay responsive next to a GPU map. ADR-0001 left the choice between Svelte and React open; mixing them is not allowed.

## Decision

Use **Svelte 5 (runes) + TypeScript (`strict: true`)**, built with Vite. React is not used anywhere in the app.

## Reasons

- Svelte compiles to direct DOM updates with no virtual DOM. High-frequency snapshot updates cost less and do not need memoisation discipline.
- Runes (`$state`, `$derived`) give fine-grained reactivity that maps well onto a stream of state diffs.
- Smaller bundles and fewer dependencies, which helps the air-gapped vendoring and licence audit.
- MapLibre and deck.gl are framework-agnostic, so we lose nothing on the map side.

## Trade-offs

- A smaller hiring pool and component ecosystem than React. We accept this; most of our components are custom (symbology, editors) anyway.
- If a React-only library becomes essential, it must be wrapped behind a framework-neutral boundary. A new ADR is required before any React code enters the repo.
