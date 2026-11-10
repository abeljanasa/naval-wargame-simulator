<script lang="ts">
  import { onMount } from "svelte";
  import type { Map as MapLibreMap } from "maplibre-gl";
  import type { MapboxOverlay } from "@deck.gl/mapbox";
  import {
    createMap,
    buildLayers,
    contactAge,
    contactKind,
    setMapTheme,
    CONTAINMENT_95,
    type MapTheme,
  } from "./lib/map";
  import { sim, COMPRESSIONS } from "./lib/sim.svelte";
  import { bearing, distance, gameTime, knots, latLon, n, MPS_PER_KNOT } from "./lib/format";
  import type { Emcon, Mission, Viewer, Wcs } from "./lib/bindings";
  import DbBrowser from "./lib/DbBrowser.svelte";

  let mapEl: HTMLElement;
  let map: MapLibreMap | null = null;
  let overlay = $state<MapboxOverlay | null>(null);
  let speedInput = $state("");
  let startError = $state<string | null>(null);
  let engageContact = $state("");
  let engageWeapon = $state("");
  let engageRounds = $state("1");
  let umpireReason = $state("");

  // Keep the engage form pointing at something that exists.
  $effect(() => {
    const u = sim.selectedUnit;
    if (u && !u.stores.some((st) => st.weapon === engageWeapon)) {
      engageWeapon = u.stores[0]?.weapon ?? "";
    }
    if (!sim.contacts.some((c) => String(c.number) === engageContact)) {
      engageContact = sim.contacts[0] ? String(sim.contacts[0].number) : "";
    }
  });

  const clock = $derived(sim.clock);
  const viewerKey = $derived(viewerToKey(sim.session?.viewer ?? { kind: "white_cell" }));

  function missionText(m: Mission): string {
    switch (m.kind) {
      case "patrol":
        return `patrol ${(n(m.radius_m) / 1000).toFixed(0)} km around ${latLon(m.centre.lat_deg, m.centre.lon_deg)}`;
      case "ferry":
        return `ferry to ${latLon(m.destination.lat_deg, m.destination.lon_deg)}`;
      case "escort":
        return `escort ${unitName(m.escorted)}`;
      case "strike":
        return `strike C${m.contact}`;
    }
  }

  function unitName(id: number): string {
    return sim.units.find((u) => u.id === id)?.name ?? `unit ${id}`;
  }

  function viewerToKey(v: Viewer): string {
    return v.kind === "white_cell" ? "white" : `side-${v.side}`;
  }

  function keyToViewer(k: string): Viewer {
    return k === "white" ? { kind: "white_cell" } : { kind: "side", side: Number(k.slice(5)) };
  }

  // Dark by default (spec §10); the viewer's choice is remembered on this machine only.
  let theme = $state<MapTheme>(readTheme());

  function readTheme(): MapTheme {
    try {
      return localStorage.getItem("nws-theme") === "light" ? "light" : "dark";
    } catch {
      return "dark";
    }
  }

  function toggleTheme() {
    theme = theme === "dark" ? "light" : "dark";
    try {
      localStorage.setItem("nws-theme", theme);
    } catch {
      // Storage unavailable: the choice lasts for this session only.
    }
  }

  $effect(() => {
    document.documentElement.dataset.theme = theme;
    if (map) setMapTheme(map, theme);
  });

  onMount(() => {
    const created = createMap(mapEl, [146.0, -39.3]);
    map = created.map;
    overlay = created.overlay;
    setMapTheme(map, theme);
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
        bearings: sim.bearings,
        weapons: sim.weapons,
        minefields: sim.minefields,
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
      {#if sim.environment}
        <span class="env" title="Sea state (clutter against low targets) and weather loss on radar">
          Sea state {sim.environment.sea_state}{n(sim.environment.weather_loss_db) > 0
            ? ` · weather −${n(sim.environment.weather_loss_db).toFixed(1)} dB`
            : ""}
        </span>
      {/if}
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
    <button
      class="theme"
      onclick={toggleTheme}
      aria-label={`Switch to ${theme === "dark" ? "light" : "dark"} theme`}
      title="Switch light / dark theme"
    >
      {theme === "dark" ? "Light" : "Dark"}
    </button>
  </header>

  <aside class="panel">
    {#if startError}
      <p class="error">{startError}</p>
    {/if}

    <section>
      <h2><label for="scenario-file">Scenario</label></h2>
      <input
        id="scenario-file"
        type="file"
        accept=".json,application/json"
        onchange={(e) => {
          const file = e.currentTarget.files?.[0];
          if (file) void sim.loadScenario(file);
          e.currentTarget.value = "";
        }}
      />
    </section>

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
              <span class="name">C{c.number} · {contactKind(c)}</span>
              <span class="meta">
                {latLon(c.position.lat_deg, c.position.lon_deg)} ± {distance(
                  n(c.error.semi_major_m) * CONTAINMENT_95,
                )} (95 %) · seen {contactAge(n(clock?.game_time_s ?? 0), c)} ago by {unitName(
                  c.detected_by,
                )}
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
          <dt><label for="wcs">Weapons</label></dt>
          <dd>
            <select
              id="wcs"
              value={u.doctrine_set.wcs ?? "inherit"}
              onchange={(e) => {
                const v = e.currentTarget.value;
                void sim.orderDoctrine({ wcs: v === "inherit" ? null : (v as Wcs) });
              }}
            >
              <option value="inherit">Inherit ({u.doctrine.wcs})</option>
              <option value="safe">Safe</option>
              <option value="tight">Tight</option>
              <option value="free">Free</option>
            </select>
          </dd>
          <dt><label for="emcon">Emissions</label></dt>
          <dd>
            <select
              id="emcon"
              value={u.doctrine_set.emcon ?? "inherit"}
              onchange={(e) => {
                const v = e.currentTarget.value;
                void sim.orderDoctrine({ emcon: v === "inherit" ? null : (v as Emcon) });
              }}
            >
              <option value="inherit">Inherit ({u.doctrine.emcon})</option>
              <option value="active">Active</option>
              <option value="silent">Silent (radar off)</option>
            </select>
          </dd>
          <dt>Condition</dt>
          <dd class:hurt={u.status !== "operational"}>
            {u.status.replaceAll("_", " ")} · {Math.round(n(u.damage_fraction) * 100)} % damage{u.fires >
            0
              ? ` · ${u.fires} fire${u.fires > 1 ? "s" : ""}`
              : ""}{u.flooding ? " · flooding" : ""}{u.criticals.length
              ? ` · lost: ${u.criticals.join(", ").replaceAll("_", " ")}`
              : ""}
          </dd>
          <dt>Mission</dt>
          <dd>
            {u.mission ? missionText(u.mission) : "none"}{u.mission_complete ? " (complete)" : ""}
            <div class="row">
              <button type="button" onclick={() => sim.orderPatrolHere()}>Patrol here</button>
              {#if u.mission}
                <button type="button" onclick={() => sim.orderPatrolHere(true)}>Clear</button>
              {/if}
            </div>
          </dd>
          <dt>Stores</dt>
          <dd>
            {#each u.stores as st (st.weapon)}
              <div>{st.weapon} × {st.count}</div>
            {:else}
              none
            {/each}
          </dd>
        </dl>

        {#if u.stores.length > 0 && sim.contacts.length > 0}
          <form
            class="engage"
            aria-label="Engage"
            onsubmit={(e) => {
              e.preventDefault();
              void sim.orderEngage(Number(engageContact), engageWeapon, Number(engageRounds));
            }}
          >
            <label for="engage-contact">Target</label>
            <select id="engage-contact" bind:value={engageContact}>
              {#each sim.contacts as c (c.number)}
                <option value={String(c.number)}>C{c.number} · {contactKind(c)}</option>
              {/each}
            </select>
            <label for="engage-weapon">Weapon</label>
            <select id="engage-weapon" bind:value={engageWeapon}>
              {#each u.stores as st (st.weapon)}
                <option value={st.weapon}>{st.weapon} ({st.count})</option>
              {/each}
            </select>
            <label for="engage-rounds">Rounds</label>
            <input id="engage-rounds" type="number" min="1" step="1" bind:value={engageRounds} />
            <button type="submit">Fire</button>
          </form>
        {/if}

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

    <details class="db-panel">
      <summary>Platform database</summary>
      <DbBrowser />
    </details>

    {#if sim.session?.viewer.kind === "white_cell"}
      <section class="umpire" aria-label="White-cell console">
        <h2>White-cell console</h2>
        <label for="umpire-reason">Reason (logged)</label>
        <input
          id="umpire-reason"
          type="text"
          bind:value={umpireReason}
          placeholder="Why this override"
        />
        {#if sim.selectedUnit}
          {@const u = sim.selectedUnit}
          <p class="dim">Override {u.name}:</p>
          <div class="row">
            <button
              type="button"
              onclick={() =>
                sim.adjudicate(umpireReason, {
                  kind: "set_damage",
                  unit: u.id,
                  damage_dp: 0,
                  clear_effects: true,
                })}>Repair</button
            >
            <button
              type="button"
              onclick={() => sim.adjudicate(umpireReason, { kind: "destroy_unit", unit: u.id })}
              >Destroy</button
            >
            <button
              type="button"
              onclick={() => sim.adjudicate(umpireReason, { kind: "remove_unit", unit: u.id })}
              >Remove</button
            >
          </div>
        {/if}
        {#if sim.weapons.length > 0}
          <p class="dim">Weapons in flight:</p>
          <ul class="units">
            {#each sim.weapons as w (w.id)}
              <li class="row">
                <span>#{w.id} {w.weapon}{w.homing ? " (homing)" : ""}</span>
                <button
                  type="button"
                  onclick={() =>
                    sim.adjudicate(umpireReason, {
                      kind: "force_weapon_outcome",
                      weapon_id: w.id,
                      hit: true,
                    })}>Hit</button
                >
                <button
                  type="button"
                  onclick={() =>
                    sim.adjudicate(umpireReason, {
                      kind: "force_weapon_outcome",
                      weapon_id: w.id,
                      hit: false,
                    })}>Miss</button
                >
              </li>
            {/each}
          </ul>
        {/if}
        <p class="dim">Override log:</p>
        <ul class="units adjudications">
          {#each [...sim.adjudications].reverse() as a, i (i)}
            <li>
              <span class="meta"
                >{gameTime(n(a.tick))} · {a.umpire} · {a.action.kind.replaceAll("_", " ")}</span
              >
              <span>{a.reason}</span>
            </li>
          {:else}
            <li class="empty">No overrides yet.</li>
          {/each}
        </ul>
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
  .clock .env {
    color: var(--text-dim);
    font-size: 12px;
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
  .engage {
    display: grid;
    grid-template-columns: 76px 1fr;
    gap: 4px 8px;
    align-items: center;
    margin-bottom: 10px;
  }
  .engage button {
    grid-column: 2;
  }
  .umpire {
    border-top: 1px solid var(--border);
    padding-top: 12px;
  }
  .umpire input[type="text"] {
    width: 100%;
    box-sizing: border-box;
    margin-bottom: 8px;
  }
  .dim {
    color: var(--text-dim);
    font-size: 12px;
    margin: 8px 0 4px;
  }
  .adjudications li {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .hurt {
    color: var(--danger);
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
