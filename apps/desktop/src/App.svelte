<script lang="ts">
  import { onMount } from "svelte";
  import type { Map as MapLibreMap } from "maplibre-gl";
  import type { MapboxOverlay } from "@deck.gl/mapbox";
  import { createMap, buildLayers, contactAge } from "./lib/map";
  import { sim, COMPRESSIONS } from "./lib/sim.svelte";
  import { bearing, gameTime, knots, latLon, n, MPS_PER_KNOT } from "./lib/format";
  import type { Viewer } from "./lib/bindings";

  let mapEl: HTMLElement;
  let map: MapLibreMap | null = null;
  let overlay = $state<MapboxOverlay | null>(null);
  let speedInput = $state("");
  let startError = $state<string | null>(null);

  const clock = $derived(sim.frame?.clock ?? null);
  const viewerKey = $derived(viewerToKey(sim.session?.viewer ?? { kind: "white_cell" }));

  function unitName(id: number): string {
    return sim.units.find((u) => u.id === id)?.name ?? `unit ${id}`;
  }

  function viewerToKey(v: Viewer): string {
    return v.kind === "white_cell" ? "white" : `side-${v.side}`;
  }

  function keyToViewer(k: string): Viewer {
    return k === "white" ? { kind: "white_cell" } : { kind: "side", side: Number(k.slice(5)) };
  }

  onMount(() => {
    const created = createMap(mapEl, [146.0, -39.3]);
    map = created.map;
    overlay = created.overlay;
    map.on("click", (e) => {
      if (sim.draft) sim.addWaypoint({ lat_deg: e.lngLat.lat, lon_deg: e.lngLat.lng });
    });
    sim.start().catch((e: unknown) => {
      startError = `Cannot connect to the simulation engine: ${String(e)}`;
    });
    return () => map?.remove();
  });

  // Redraw the dynamic layers whenever the frame, the selection or the draft changes.
  $effect(() => {
    if (!overlay) return;
    overlay.setProps({
      layers: buildLayers({
        units: sim.units,
        contacts: sim.contacts,
        gameTimeS: n(clock?.game_time_s ?? 0),
        viewer: sim.session?.viewer ?? { kind: "white_cell" },
        selected: sim.selected,
        draft: sim.draft,
        onPickUnit: (id) => sim.select(id),
      }),
    });
  });

  // Keep the speed box in step with the selected unit's ordered speed while it is not being edited.
  $effect(() => {
    const u = sim.selectedUnit;
    if (u && document.activeElement?.id !== "ordered-speed") {
      speedInput = (n(u.ordered_speed_mps) / MPS_PER_KNOT).toFixed(1);
    }
  });

  function onKey(e: KeyboardEvent) {
    if (e.target instanceof HTMLInputElement || e.target instanceof HTMLSelectElement) return;
    if (e.key === " ") {
      e.preventDefault();
      void sim.togglePause();
    } else if (e.key >= "1" && e.key <= "7") {
      const c = COMPRESSIONS[Number(e.key) - 1];
      if (c) void sim.setCompression(c.key);
    } else if (e.key === "Escape") {
      if (sim.draft) sim.cancelPlan();
      else sim.select(null);
    } else if (e.key === "Enter" && sim.draft?.length) {
      void sim.sendPlan();
    } else if (e.key === "Backspace" && sim.draft) {
      sim.undoWaypoint();
    } else if (e.key === "." && clock?.paused) {
      void sim.step(1);
    }
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="shell">
  <header class="topbar">
    <div class="title">
      <strong>{sim.session?.scenario_name ?? "Loading…"}</strong>
    </div>

    <div class="clock">
      <span class="time">{gameTime(clock?.game_time_s ?? 0)}</span>
      <span class="ratio" title="Achieved game seconds per wall second">
        {clock && !clock.paused ? `${Math.round(n(clock.achieved_ratio))}×` : "paused"}
      </span>
    </div>

    <div class="controls" role="group" aria-label="Time controls">
      <button
        class="play"
        class:running={clock && !clock.paused}
        onclick={() => sim.togglePause()}
        title="Pause / resume (Space)"
      >
        {clock?.paused === false ? "❚❚ Pause" : "▶ Run"}
      </button>
      <button onclick={() => sim.step(1)} disabled={!clock?.paused} title="Step one tick (.)">
        Step
      </button>
      {#each COMPRESSIONS as c, i (c.key)}
        <button
          class:active={clock?.compression === c.key}
          onclick={() => sim.setCompression(c.key)}
          title={`Time compression ${c.label} (${i + 1})`}
        >
          {c.label}
        </button>
      {/each}
    </div>
  </header>

  <aside class="panel">
    {#if startError}
      <p class="error">{startError}</p>
    {/if}

    <section>
      <h2><label for="seat">Seat</label></h2>
      <select
        id="seat"
        class="seat"
        value={viewerKey}
        onchange={(e) => sim.setViewer(keyToViewer(e.currentTarget.value))}
      >
        {#each sim.session?.sides ?? [] as s (s.id)}
          <option value={`side-${s.id}`}>{s.name}</option>
        {/each}
        <option value="white">White cell (ground truth)</option>
      </select>
    </section>

    <section>
      <h2>Units</h2>
      <ul class="units">
        {#each sim.units as u (u.id)}
          <li>
            <button class:selected={u.id === sim.selected} onclick={() => sim.select(u.id)}>
              <span class="name">{u.name}</span>
              <span class="meta">{bearing(u.heading_deg)} · {knots(u.speed_mps)}</span>
            </button>
          </li>
        {:else}
          <li class="empty">No units visible from this seat.</li>
        {/each}
      </ul>
    </section>

    {#if sim.session?.viewer.kind === "side"}
      <section>
        <h2>Contacts</h2>
        <ul class="units">
          {#each sim.contacts as c (c.number)}
            <li class="contact">
              <span class="name">C{c.number} · unknown</span>
              <span class="meta">
                {latLon(c.position.lat_deg, c.position.lon_deg)} · seen {contactAge(
                  n(clock?.game_time_s ?? 0),
                  c,
                )} ago by {unitName(c.detected_by)}
              </span>
            </li>
          {:else}
            <li class="empty">No contacts held.</li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if sim.selectedUnit}
      {@const u = sim.selectedUnit}
      <section>
        <h2>{u.name}</h2>
        <dl>
          <dt>Class</dt>
          <dd>{u.platform ?? "—"}</dd>
          <dt>Position</dt>
          <dd>{latLon(u.position.lat_deg, u.position.lon_deg)}</dd>
          <dt>Heading</dt>
          <dd>{bearing(u.heading_deg)}</dd>
          <dt>Speed</dt>
          <dd>
            {knots(u.speed_mps)} (ordered {knots(u.ordered_speed_mps)}, max {knots(
              u.max_speed_mps,
            )})
          </dd>
          <dt>Route</dt>
          <dd>
            {#if u.route && u.route.next < u.route.waypoints.length}
              waypoint {u.route.next + 1} of {u.route.waypoints.length}
            {:else}
              none
            {/if}
          </dd>
        </dl>

        <form
          class="row"
          onsubmit={(e) => {
            e.preventDefault();
            void sim.orderSpeedKnots(Number(speedInput));
          }}
        >
          <label for="ordered-speed">Ordered speed (kn)</label>
          <input id="ordered-speed" type="number" min="0" step="0.5" bind:value={speedInput} />
          <button type="submit">Set</button>
        </form>

        {#if sim.draft}
          <div class="plan">
            <p>Click the map to add waypoints ({sim.draft.length} so far).</p>
            <div class="row">
              <button onclick={() => sim.sendPlan()} disabled={!sim.draft.length}
                >Send route ⏎</button
              >
              <button onclick={() => sim.undoWaypoint()} disabled={!sim.draft.length}>Undo ⌫</button
              >
              <button onclick={() => sim.cancelPlan()}>Cancel Esc</button>
            </div>
          </div>
        {:else}
          <button onclick={() => sim.beginPlan()}>Plan new route…</button>
        {/if}
      </section>
    {/if}

    {#if sim.error}
      <p class="error" role="alert">{sim.error}</p>
    {/if}

    <footer class="help">Space run/pause · 1–7 compression · . step · Esc deselect</footer>
  </aside>

  <main class="map" bind:this={mapEl} class:planning={sim.draft !== null}></main>
</div>

<style>
  .shell {
    display: grid;
    grid-template-columns: 320px 1fr;
    grid-template-rows: 48px 1fr;
    height: 100vh;
  }
  .topbar {
    grid-column: 1 / -1;
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 0 12px;
    min-width: 0;
    background: var(--surface-2);
    border-bottom: 1px solid var(--border);
  }
  .title {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .clock {
    display: flex;
    align-items: baseline;
    gap: 10px;
    font-variant-numeric: tabular-nums;
  }
  .clock .time {
    font-family: var(--mono);
    font-size: 18px;
  }
  .clock .ratio {
    color: var(--text-dim);
    min-width: 56px;
  }
  .controls {
    display: flex;
    gap: 3px;
    flex-shrink: 0;
  }
  .controls .active {
    background: var(--accent);
    color: var(--on-accent);
    border-color: var(--accent);
  }
  .play.running {
    border-color: var(--accent);
  }
  .seat {
    width: 100%;
  }
  .panel {
    overflow-y: auto;
    padding: 12px;
    background: var(--surface-1);
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  h2 {
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-dim);
    margin: 0 0 8px;
  }
  .units {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .units button {
    width: 100%;
    text-align: left;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .units button.selected {
    border-color: var(--accent);
    background: var(--surface-3);
  }
  .units .meta {
    color: var(--text-dim);
    font-family: var(--mono);
    font-size: 12px;
  }
  .empty {
    color: var(--text-dim);
  }
  .contact {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 4px 8px;
    border: 1px dashed var(--warn);
    border-radius: 4px;
  }
  .contact .meta {
    color: var(--text-dim);
    font-family: var(--mono);
    font-size: 11px;
  }
  dl {
    display: grid;
    grid-template-columns: 76px 1fr;
    gap: 4px 8px;
    margin: 0 0 12px;
  }
  dt {
    color: var(--text-dim);
  }
  dd {
    margin: 0;
    font-family: var(--mono);
    font-size: 12px;
  }
  .row {
    display: flex;
    gap: 6px;
    align-items: center;
    flex-wrap: wrap;
    margin-bottom: 10px;
  }
  .row input {
    width: 72px;
  }
  .plan p {
    margin: 0 0 8px;
    color: var(--warn);
  }
  .error {
    color: var(--danger);
    margin: 0;
  }
  .help {
    margin-top: auto;
    color: var(--text-dim);
    font-size: 11px;
  }
  .map {
    position: relative;
  }
  .map.planning :global(canvas) {
    cursor: crosshair;
  }
</style>
