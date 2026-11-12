<script lang="ts">
  import { onMount } from "svelte";
  import type { Map as MapLibreMap } from "maplibre-gl";
  import type { MapboxOverlay } from "@deck.gl/mapbox";
  import {
    createMap,
    declutter,
    labelCandidates,
    outcomeLayers,
    outcomeText,
    buildLayers,
    coverageLayers,
    nightLayers,
    editorLayers,
    planningLayers,
    referenceLayers,
    contactAge,
    contactKind,
    setMapTheme,
    CONTAINMENT_95,
    type MapTheme,
    type SymbolStyle,
  } from "./lib/map";
  import { sim, COMPRESSIONS } from "./lib/sim.svelte";
  import { bearing, distance, gameTime, knots, latLon, n, MPS_PER_KNOT } from "./lib/format";
  import type {
    Audience,
    CommsView,
    Emcon,
    Facility,
    Mission,
    Seat,
    Wcs,
    Zone,
  } from "./lib/bindings";
  import AarPanel from "./lib/AarPanel.svelte";
  import ScenarioEditor from "./lib/ScenarioEditor.svelte";
  import { editor } from "./lib/editor.svelte";
  import DbBrowser from "./lib/DbBrowser.svelte";
  import Menubar, { type Menu } from "./lib/Menubar.svelte";

  let mapEl: HTMLElement;
  let map: MapLibreMap | null = null;
  let overlay = $state<MapboxOverlay | null>(null);
  let speedInput = $state("");
  let startError = $state<string | null>(null);
  let engageContact = $state("");
  let engageWeapon = $state("");
  let engageRounds = $state("1");
  let umpireReason = $state("");
  let messageTo = $state("all");
  let newSeatName = $state("");
  let newSeatRole = $state("white");
  let messageText = $state("");

  function audienceFromKey(k: string): Audience {
    if (k === "all") return { to: "all" };
    if (k === "white") return { to: "white_cell" };
    return { to: "side", side: Number(k.slice(5)) };
  }

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
  /** Shade the night side of the map. */
  let showNight = $state(true);

  /** White cell: the kind and radius of a zone to add on the map. */
  let newZoneKind = $state<"sense" | "engage">("sense");
  let newZoneKm = $state(20);

  function editPlanning(change: (f: Facility[], z: Zone[]) => void) {
    const facilities = structuredClone($state.snapshot(sim.session?.facilities ?? []));
    const zones = structuredClone($state.snapshot(sim.session?.zones ?? []));
    change(facilities, zones);
    void sim.setPlanning(facilities, zones);
  }

  function addZoneOnMap() {
    sim.pick("Click the centre of the new zone", (p) => {
      editPlanning((_, zones) => {
        const id = Math.max(0, ...zones.map((z) => z.id)) + 1;
        zones.push({
          id,
          name: `${newZoneKind === "sense" ? "Sense" : "Engage"} ${id}`,
          side: sim.session?.sides[0]?.id ?? 1,
          kind: newZoneKind,
          area: { shape: "circle", centre: p, radius_m: newZoneKm * 1000 },
          doctrine: { wcs: null, emcon: null, engage_unfriendly: null },
          choke_point: null,
          shooters: [],
          layer: null,
          sensors: [],
        });
      });
    });
  }

  /** Sense zones the seat may see coverage for: its side's, or every one for the white cell. */
  const senseZones = $derived(
    (sim.session?.zones ?? []).filter(
      (z) =>
        z.kind === "sense" &&
        (sim.session?.viewer.kind !== "side" || z.side === sim.session.viewer.side),
    ),
  );

  // Coverage moves with the sensors: refresh the shown zones every 3 s.
  $effect(() => {
    if (sim.coverageShown.length === 0) return;
    const timer = setInterval(() => void sim.refreshCoverage(), 3000);
    return () => clearInterval(timer);
  });

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
      case "support":
        return `support station at ${latLon(m.station.lat_deg, m.station.lon_deg)}`;
      case "mining":
        return `lay ${m.mines} mines at ${latLon(m.centre.lat_deg, m.centre.lon_deg)}`;
      case "mine_clearing":
        return `clear mines ${(n(m.radius_m) / 1000).toFixed(0)} km around ${latLon(m.centre.lat_deg, m.centre.lon_deg)}`;
      case "cargo":
        return `carry ${n(m.tonnes).toFixed(0)} t to ${latLon(m.destination.lat_deg, m.destination.lon_deg)}`;
      case "barrier_patrol":
        return `barrier patrol ${latLon(m.start.lat_deg, m.start.lon_deg)} – ${latLon(m.end.lat_deg, m.end.lon_deg)}`;
      case "denial":
        return `deny choke point ${m.choke_point}`;
    }
  }

  function audienceText(a: Audience): string {
    if (a.to === "all") return "all";
    if (a.to === "white_cell") return "white cell";
    return sim.session?.sides.find((s) => s.id === a.side)?.name ?? `side ${a.side}`;
  }

  function facilityName(id: number): string {
    return (sim.session?.facilities ?? []).find((f) => f.id === id)?.name ?? `facility ${id}`;
  }

  function commsText(c: CommsView): string {
    const post = c.post == null ? "" : ` (${facilityName(c.post)})`;
    if (c.status === "up") return `link up${post}${c.via == null ? "" : ` via ${unitName(c.via)}`}`;
    return `${c.status.replaceAll("_", " ")}${post}: orders will not get through`;
  }

  function unitName(id: number): string {
    return sim.units.find((u) => u.id === id)?.name ?? `unit ${id}`;
  }

  function seatRoleText(s: Seat): string {
    return s.role === "white_cell"
      ? "white cell"
      : (sim.session?.sides.find((x) => x.id === s.side)?.name ?? `side ${s.side}`);
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

  // Unit figures by default; APP-6 frames as the alternative (remembered like the theme).
  let symbolStyle = $state<SymbolStyle>(readSymbolStyle());

  function readSymbolStyle(): SymbolStyle {
    try {
      return localStorage.getItem("nws-symbols") === "frames" ? "frames" : "figures";
    } catch {
      return "figures";
    }
  }

  function toggleSymbols() {
    symbolStyle = symbolStyle === "figures" ? "frames" : "figures";
    try {
      localStorage.setItem("nws-symbols", symbolStyle);
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
      const p = { lat_deg: e.lngLat.lat, lon_deg: e.lngLat.lng };
      if (editor.open && editor.deliver(p)) return;
      if (sim.deliverPick(p)) return;
      if (sim.draft) sim.addWaypoint(p);
    });
    // Re-place labels as the map moves, at most once a frame.
    let queued = false;
    map.on("move", () => {
      if (queued) return;
      queued = true;
      requestAnimationFrame(() => {
        queued = false;
        relayout();
      });
    });
    sim.start().catch((e: unknown) => {
      startError = `Cannot connect to the simulation engine: ${String(e)}`;
    });
    return () => map?.remove();
  });

  // Frame the map on the units a seat sees when its first picture of a scenario arrives, so the
  // action is not a cluster of overlapping labels at a fixed zoom. Once per scenario and seat;
  // the user's own view is kept after that.
  let framed = "";
  $effect(() => {
    const name = sim.session?.scenario_name ?? "";
    const key = `${name}|${sim.session?.seat.id ?? ""}`;
    if (!map || !name || key === framed || sim.units.length === 0) return;
    framed = key;
    const lats = sim.units.map((u) => n(u.position.lat_deg));
    const lons = sim.units.map((u) => n(u.position.lon_deg));
    map.fitBounds(
      [
        [Math.min(...lons), Math.min(...lats)],
        [Math.max(...lons), Math.max(...lats)],
      ],
      { padding: 80, maxZoom: 9, duration: 0 },
    );
  });

  // The newest weapon outcome of the last 15 s, as a notice in the status bar.
  const latestOutcome = $derived.by(() => {
    const now = n(clock?.game_time_s ?? 0);
    const recent = sim.outcomes.filter((o) => now - n(o.tick) <= 15);
    return recent.length ? recent[recent.length - 1] : null;
  });
  const latestOutcomeText = $derived(
    latestOutcome
      ? outcomeText(
          latestOutcome,
          sim.session?.viewer.kind === "side" ? sim.session.viewer.side : null,
        )
      : "",
  );

  // Label declutter (map.ts): where labels would overlap on screen, the lower priority hides until
  // the user zooms in. Recomputed when the picture changes and as the map moves.
  let hiddenLabels = $state(new Set<string>());
  function relayout() {
    const m = map;
    if (!m || editor.open) return;
    hiddenLabels = declutter(
      labelCandidates({
        units: sim.units,
        contacts: sim.contacts,
        minefields: sim.minefields ?? [],
        facilities: sim.session?.facilities ?? [],
        zones: sim.session?.zones ?? [],
        points: sim.session?.reference_points ?? [],
        selected: sim.selected,
        gameTimeS: n(clock?.game_time_s ?? 0),
      }),
      (p) => {
        const q = m.project(p);
        return [q.x, q.y];
      },
    );
  }
  $effect(() => {
    relayout();
  });

  // Redraw the dynamic layers whenever the frame, the selection or the draft changes.
  $effect(() => {
    if (!overlay) return;
    if (editor.open) {
      // The editor draws the draft instead of the live picture.
      const d = editor.draft;
      overlay.setProps({
        layers: [
          ...planningLayers(d.facilities ?? [], d.zones ?? [], "editor-planning"),
          ...editorLayers({
            units: d.units,
            minefields: d.minefields ?? [],
            points: d.reference_points ?? [],
            sides: d.sides.map((x) => x.id),
            selected: editor.selectedUnit,
            onPickUnit: (id) => (editor.selectedUnit = id),
          }),
        ],
      });
      return;
    }
    overlay.setProps({
      layers: [
        ...(showNight
          ? nightLayers(n(sim.environment?.start_unix_s ?? 0) + n(clock?.game_time_s ?? 0))
          : []),
        ...coverageLayers(sim.coverage),
        ...planningLayers(
          sim.session?.facilities ?? [],
          sim.session?.zones ?? [],
          "planning",
          hiddenLabels,
        ),
        ...buildLayers({
          units: sim.units,
          contacts: sim.contacts,
          bearings: sim.bearings,
          weapons: sim.weapons,
          weaponTracks: sim.session?.viewer.kind === "side" ? sim.weaponTracks : [],
          minefields: sim.minefields,
          gameTimeS: n(clock?.game_time_s ?? 0),
          viewer: sim.session?.viewer ?? { kind: "white_cell" },
          selected: sim.selected,
          draft: sim.draft,
          onPickUnit: (id) => sim.select(id),
          hiddenLabels,
          symbolStyle,
        }),
        ...referenceLayers(sim.session?.reference_points ?? [], undefined, hiddenLabels),
        ...outcomeLayers(sim.outcomes, n(clock?.game_time_s ?? 0)),
      ],
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
    // Keys typed into any text entry are text, not shortcuts (a space in a Lua script or a record's
    // JSON must not pause the clock, nor a digit change the compression).
    const t = e.target;
    if (t instanceof HTMLElement && (t.isContentEditable || t.closest("input, select, textarea")))
      return;
    if (!sim.controlsTime && (e.key === " " || e.key === "." || (e.key >= "1" && e.key <= "7")))
      return;
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

  // ---- Layout: menus, panel tabs, dialogs ----

  type LeftTab = "picture" | "messages" | "coverage";
  type RightTab = "unit" | "injects" | "events" | "planning" | "seats" | "log" | "review";
  let leftTab = $state<LeftTab>("picture");
  let rightTab = $state<RightTab>("unit");
  let showDb = $state(false);
  let showKeys = $state(false);
  let scenarioInput: HTMLInputElement;
  let editInput: HTMLInputElement;

  const isWhiteCell = $derived(sim.session?.viewer.kind === "white_cell");

  /** The right-hand tabs this seat has. */
  const rightTabs = $derived<{ key: RightTab; label: string }[]>([
    { key: "unit", label: "Unit" },
    ...(isWhiteCell
      ? ([
          { key: "injects", label: "Injects" },
          { key: "events", label: "Events" },
          { key: "planning", label: "Planning" },
          { key: "seats", label: "Seats" },
          { key: "log", label: "Log" },
        ] as const)
      : []),
    ...(sim.controlsTime ? ([{ key: "review", label: "Review" }] as const) : []),
  ]);
  /** A side seat with nothing selected and nothing to review keeps the map wide. */
  const showRight = $derived(rightTabs.length > 1 || sim.selectedUnit !== null);

  // Selecting a unit brings its inspector forward.
  $effect(() => {
    if (sim.selected !== null) rightTab = "unit";
  });
  // A tab the seat no longer has falls back to the unit inspector.
  $effect(() => {
    if (!rightTabs.some((t) => t.key === rightTab)) rightTab = "unit";
  });
  $effect(() => {
    if (leftTab === "coverage" && senseZones.length === 0) leftTab = "picture";
  });

  // Unread messages: those newer than the last ones seen on the Messages tab.
  let seenTick = $state(-1);
  const newestTick = $derived(Math.max(-1, ...sim.messages.map((m) => n(m.tick))));
  const unread = $derived(sim.messages.filter((m) => n(m.tick) > seenTick).length);
  $effect(() => {
    if (leftTab === "messages") seenTick = newestTick;
  });

  function showTab(t: RightTab) {
    rightTab = t;
  }

  const menus = $derived<Menu[]>([
    {
      label: "File",
      items: [
        { label: "Open scenario…", onselect: () => scenarioInput.click() },
        {
          label: "New scenario",
          disabled: !sim.controlsTime,
          separatorBefore: true,
          onselect: () => void editor.start("blank"),
        },
        {
          label: "Edit this scenario",
          disabled: !sim.controlsTime,
          onselect: () => void editor.start("current"),
        },
        {
          label: "Edit a scenario file…",
          disabled: !sim.controlsTime,
          onselect: () => editInput.click(),
        },
      ],
    },
    {
      label: "View",
      items: [
        {
          label: "Night shading",
          checked: showNight,
          onselect: () => (showNight = !showNight),
        },
        { label: "Dark theme", checked: theme === "dark", onselect: toggleTheme },
        {
          label: "Unit figures (off: APP-6 frames)",
          checked: symbolStyle === "figures",
          onselect: toggleSymbols,
        },
        { label: "Picture", separatorBefore: true, onselect: () => (leftTab = "picture") },
        { label: "Messages", onselect: () => (leftTab = "messages") },
        {
          label: "Sense coverage",
          disabled: senseZones.length === 0,
          onselect: () => (leftTab = "coverage"),
        },
        {
          label: "Platform database…",
          separatorBefore: true,
          onselect: () => (showDb = true),
        },
      ],
    },
    {
      label: "Exercise",
      items: [
        {
          label: clock?.paused === false ? "Pause" : "Run",
          shortcut: "Space",
          disabled: !sim.controlsTime,
          onselect: () => void sim.togglePause(),
        },
        {
          label: "Step one second",
          shortcut: ".",
          disabled: !sim.controlsTime || !clock?.paused,
          onselect: () => void sim.step(1),
        },
        {
          label: "Review and AAR",
          separatorBefore: true,
          disabled: !sim.controlsTime,
          onselect: () => showTab("review"),
        },
        ...(isWhiteCell
          ? [
              { label: "Injects", separatorBefore: true, onselect: () => showTab("injects") },
              { label: "Events", onselect: () => showTab("events") },
              { label: "Planning layer", onselect: () => showTab("planning") },
              { label: "Seats", onselect: () => showTab("seats") },
              { label: "Override log", onselect: () => showTab("log") },
            ]
          : []),
      ],
    },
    {
      label: "Help",
      items: [{ label: "Keyboard shortcuts", onselect: () => (showKeys = true) }],
    },
  ]);
</script>

<svelte:window onkeydown={onKey} />

<div class="shell" class:with-right={showRight}>
  <header class="menurow">
    <Menubar {menus} />
    <span class="scenario-name">{sim.session?.scenario_name ?? "Loading…"}</span>
  </header>

  <div class="toolbar">
    <div
      class="controls"
      role="group"
      aria-label="Time controls"
      title={sim.controlsTime ? undefined : "The white cell controls time"}
    >
      <button
        class="play"
        class:running={clock && !clock.paused}
        onclick={() => sim.togglePause()}
        disabled={!sim.controlsTime}
        title="Pause / resume (Space)"
      >
        {clock?.paused === false ? "❚❚ Pause" : "▶ Run"}
      </button>
      <button
        onclick={() => sim.step(1)}
        disabled={!clock?.paused || !sim.controlsTime}
        title="Step one second (.)"
      >
        Step
      </button>
      <div class="speeds" role="group" aria-label="Time compression">
        {#each COMPRESSIONS as c, i (c.key)}
          <button
            class:active={clock?.compression === c.key}
            onclick={() => sim.setCompression(c.key)}
            disabled={!sim.controlsTime}
            title={`Time compression ${c.label} (${i + 1})`}
          >
            {c.label}
          </button>
        {/each}
      </div>
    </div>

    <div class="clock">
      {#if clock?.replay}
        <span class="replay-badge" title="Reviewing a replay: orders are off">REPLAY</span>
      {/if}
      <span class="time">{gameTime(clock?.game_time_s ?? 0)}</span>
      <span class="ratio" title="Achieved game seconds per wall second">
        {clock && !clock.paused ? `${Math.round(n(clock.achieved_ratio))}×` : "paused"}
      </span>
      {#if sim.environment}
        <span class="env" title="Sea state (clutter against low targets) and weather loss on radar">
          Sea state {sim.environment.sea_state}{n(sim.environment.weather_loss_db) > 0
            ? `, weather −${n(sim.environment.weather_loss_db).toFixed(1)} dB`
            : ""}
        </span>
      {/if}
    </div>

    <div class="seat-picker">
      {#if sim.session?.locked}
        <span class="dim">Seat</span>
        <strong title="This window is locked to its seat">{sim.session.seat.name}</strong>
      {:else}
        <label for="seat">Seat</label>
        <select
          id="seat"
          value={String(sim.session?.seat.id ?? "")}
          onchange={(e) => sim.setSeat(Number(e.currentTarget.value))}
        >
          {#each sim.session?.seats ?? [] as s (s.id)}
            <option value={String(s.id)}
              >{s.name}{s.role === "white_cell" ? " (white cell: ground truth)" : ""}</option
            >
          {/each}
        </select>
      {/if}
    </div>
  </div>

  <!-- Hidden pickers behind File › Open and File › Edit a scenario file. -->
  <input
    bind:this={scenarioInput}
    id="scenario-file"
    class="visually-hidden"
    type="file"
    accept=".json,application/json"
    aria-label="Scenario"
    onchange={(e) => {
      const file = e.currentTarget.files?.[0];
      if (file) void sim.loadScenario(file);
      e.currentTarget.value = "";
    }}
  />
  <input
    bind:this={editInput}
    class="visually-hidden"
    type="file"
    accept=".json,application/json"
    aria-label="Edit a scenario file"
    onchange={(e) => {
      const file = e.currentTarget.files?.[0];
      if (file) void editor.start(file);
      e.currentTarget.value = "";
    }}
  />

  <aside class="panel left" aria-label="Picture">
    <div class="tabs" role="tablist" aria-label="Picture panels">
      <button role="tab" aria-selected={leftTab === "picture"} onclick={() => (leftTab = "picture")}
        >Picture</button
      >
      <button
        role="tab"
        aria-selected={leftTab === "messages"}
        onclick={() => (leftTab = "messages")}
        >Messages{#if unread > 0 && leftTab !== "messages"}<span class="badge">{unread}</span
          >{/if}</button
      >
      {#if senseZones.length > 0}
        <button
          role="tab"
          aria-selected={leftTab === "coverage"}
          onclick={() => (leftTab = "coverage")}>Coverage</button
        >
      {/if}
    </div>

    <div class="tabpanel" role="tabpanel">
      {#if startError}
        <p class="error">{startError}</p>
      {/if}

      {#if leftTab === "picture"}
        <section>
          <h2>Units <span class="count">{sim.units.length}</span></h2>
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
            <h2>Contacts <span class="count">{sim.contacts.length}</span></h2>
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
      {:else if leftTab === "messages"}
        <section class="messages" aria-label="Messages">
          <ul class="units">
            {#each [...sim.messages].reverse() as m, i (i)}
              <li>
                <span class="meta"
                  >{gameTime(n(m.tick))} · {m.from}{isWhiteCell
                    ? ` → ${audienceText(m.to)}`
                    : ""}</span
                >
                {#if m.title}<strong>{m.title}</strong>{/if}
                <span>{m.text}</span>
              </li>
            {:else}
              <li class="empty">
                No messages yet. Injects, event messages and zone alerts arrive here.
              </li>
            {/each}
          </ul>
        </section>
      {:else}
        <section>
          <p class="dim">
            Tick a sense zone to draw where your sensors can watch it now (green) and the gaps
            (red).
          </p>
          <ul class="zones" aria-label="Sense coverage">
            {#each senseZones as z (z.id)}
              {@const cov = sim.coverage.find((c) => c.zone === z.id)}
              <li>
                <label class="inline">
                  <input
                    type="checkbox"
                    checked={sim.coverageShown.includes(z.id)}
                    onchange={() => void sim.toggleCoverage(z.id)}
                  />
                  {z.name}{z.layer ? ` (${z.layer})` : ""}
                </label>
                {#if cov}<span>{Math.round(n(cov.fraction) * 100)} % covered</span>{/if}
              </li>
            {/each}
          </ul>
        </section>
      {/if}
    </div>
  </aside>

  <main
    class="map"
    bind:this={mapEl}
    class:planning={sim.draft !== null || editor.picking !== null || sim.picking !== null}
  ></main>

  {#if showRight}
    <aside class="panel right" aria-label={isWhiteCell ? "White-cell console" : "Unit"}>
      {#if rightTabs.length > 1}
        <div class="tabs" role="tablist" aria-label="Console panels">
          {#each rightTabs as t (t.key)}
            <button role="tab" aria-selected={rightTab === t.key} onclick={() => (rightTab = t.key)}
              >{t.label}</button
            >
          {/each}
        </div>
      {/if}
      <div class="tabpanel" role="tabpanel">
        {#if isWhiteCell && rightTab !== "review"}
          <div class="reason">
            <label for="umpire-reason">Reason (logged)</label>
            <input
              id="umpire-reason"
              type="text"
              bind:value={umpireReason}
              placeholder="Why this override"
            />
          </div>
        {/if}

        {#if rightTab === "unit"}
          {#if sim.selectedUnit}
            {@const u = sim.selectedUnit}
            <section>
              <h2 class="unit-name">{u.name}</h2>
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
                <dt>Condition</dt>
                <dd class:hurt={u.status !== "operational"}>
                  {u.status.replaceAll("_", " ")} · {Math.round(n(u.damage_fraction) * 100)} % damage{u.fires >
                  0
                    ? ` · ${u.fires} fire${u.fires > 1 ? "s" : ""}`
                    : ""}{u.flooding ? " · flooding" : ""}{u.criticals.length
                    ? ` · lost: ${u.criticals.join(", ").replaceAll("_", " ")}`
                    : ""}
                </dd>
                {#each sim.comms.filter((c) => c.unit === u.id) as c (c.unit)}
                  <dt>Comms</dt>
                  <dd class:hurt={c.status !== "up"}>{commsText(c)}</dd>
                {/each}
                {#if u.fuel_fraction != null}
                  <dt>Fuel</dt>
                  <dd class:hurt={u.fuel_fraction < 0.3}>
                    {Math.round(u.fuel_fraction * 100)} %{u.fuel_fraction <= 0
                      ? " · no propulsion"
                      : ""}
                  </dd>
                {/if}
                <dt>Stores</dt>
                <dd>
                  {#each u.stores as st (st.weapon)}
                    <div>{st.weapon} × {st.count}</div>
                  {:else}
                    none
                  {/each}
                </dd>
              </dl>
            </section>

            <section>
              <h3>Orders</h3>
              <dl>
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
                <dt>Mission</dt>
                <dd>
                  {u.mission ? missionText(u.mission) : "none"}{u.mission_complete
                    ? " (complete)"
                    : ""}
                  <div class="row">
                    <button type="button" onclick={() => sim.orderPatrolHere()}>Patrol here</button>
                    {#if u.mission}
                      <button type="button" onclick={() => sim.orderPatrolHere(true)}>Clear</button>
                    {/if}
                  </div>
                </dd>
                {#each sim.groups.filter((x) => x.leader === u.id || x.members.some((m) => m.unit === u.id)) as g (g.name)}
                  <dt>Group</dt>
                  <dd>
                    {g.name} ({g.leader === u.id
                      ? "leader: its mission is the group's"
                      : "keeping station"})
                    <button type="button" onclick={() => void sim.detach(u.id)}>Detach</button>
                  </dd>
                {:else}
                  {#if sim.groups.some((g) => g.side === u.side)}
                    <dt><label for="join-group">Group</label></dt>
                    <dd>
                      <select
                        id="join-group"
                        onchange={(e) => {
                          const name = e.currentTarget.value;
                          if (name) void sim.join(u.id, name);
                          e.currentTarget.value = "";
                        }}
                      >
                        <option value="">join a group…</option>
                        {#each sim.groups.filter((g) => g.side === u.side) as g (g.name)}
                          <option value={g.name}>{g.name}</option>
                        {/each}
                      </select>
                    </dd>
                  {/if}
                {/each}
              </dl>

              <form
                class="row"
                onsubmit={(e) => {
                  e.preventDefault();
                  void sim.orderSpeedKnots(Number(speedInput));
                }}
              >
                <label for="ordered-speed">Ordered speed (kn)</label>
                <input
                  id="ordered-speed"
                  type="number"
                  min="0"
                  step="0.5"
                  bind:value={speedInput}
                />
                <button type="submit">Set</button>
              </form>

              {#if sim.draft}
                <div class="plan">
                  <p>Click the map to add waypoints ({sim.draft.length} so far).</p>
                  <div class="row">
                    <button onclick={() => sim.sendPlan()} disabled={!sim.draft.length}
                      >Send route ⏎</button
                    >
                    <button onclick={() => sim.undoWaypoint()} disabled={!sim.draft.length}
                      >Undo ⌫</button
                    >
                    <button onclick={() => sim.cancelPlan()}>Cancel Esc</button>
                  </div>
                </div>
              {:else}
                <button onclick={() => sim.beginPlan()}>Plan new route…</button>
              {/if}
            </section>

            {#if u.stores.length > 0 && sim.contacts.length > 0}
              <section>
                <h3>Engage</h3>
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
                  <input
                    id="engage-rounds"
                    type="number"
                    min="1"
                    step="1"
                    bind:value={engageRounds}
                  />
                  <button type="submit">Fire</button>
                </form>
              </section>
            {/if}

            {#if isWhiteCell}
              <section class="override">
                <h3>Override</h3>
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
                    onclick={() =>
                      sim.adjudicate(umpireReason, { kind: "destroy_unit", unit: u.id })}
                    >Destroy</button
                  >
                  <button
                    type="button"
                    onclick={() =>
                      sim.adjudicate(umpireReason, { kind: "remove_unit", unit: u.id })}
                    >Remove</button
                  >
                </div>
              </section>
            {/if}
          {:else}
            <p class="empty">Select a unit on the map or in the Picture list to see it here.</p>
          {/if}

          {#if isWhiteCell && sim.weapons.length > 0}
            <section class="override">
              <h3>Weapons in flight</h3>
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
            </section>
          {/if}
        {:else if rightTab === "injects"}
          <section>
            <h3>Inject library</h3>
            <ul class="units injects" aria-label="Injects">
              {#each sim.injects as s (s.inject.id)}
                <li>
                  <div class="row">
                    <strong>{s.inject.title}</strong>
                    <span class="dim"
                      >→ {audienceText(s.inject)}{s.sent ? ` · sent ×${s.sent}` : ""}</span
                    >
                    <button
                      type="button"
                      aria-label={`Send ${s.inject.title}`}
                      onclick={() => sim.facilitate({ kind: "send_inject", inject: s.inject.id })}
                      >Send</button
                    >
                  </div>
                  {#if s.inject.note}<span class="dim">{s.inject.note}</span>{/if}
                </li>
              {:else}
                <li class="empty">No injects in this scenario.</li>
              {/each}
            </ul>
          </section>
          <section>
            <h3>Free message</h3>
            <form
              class="send-message"
              aria-label="Send message"
              onsubmit={(e) => {
                e.preventDefault();
                void sim
                  .facilitate({
                    kind: "send_message",
                    ...audienceFromKey(messageTo),
                    text: messageText,
                  })
                  .then(() => {
                    if (!sim.error) messageText = "";
                  });
              }}
            >
              <label for="message-to">Message to</label>
              <select id="message-to" bind:value={messageTo}>
                <option value="all">All sides</option>
                {#each sim.session?.sides ?? [] as s (s.id)}
                  <option value={`side-${s.id}`}>{s.name}</option>
                {/each}
                <option value="white">White cell</option>
              </select>
              <input
                id="message-text"
                type="text"
                aria-label="Message text"
                bind:value={messageText}
                placeholder="From exercise control"
              />
              <button type="submit">Send</button>
            </form>
          </section>
        {:else if rightTab === "events"}
          <section>
            <h3>Scenario events</h3>
            <ul class="units" aria-label="Scenario events">
              {#each sim.scenarioEvents as e (e.id)}
                <li class="row">
                  <label>
                    <input
                      type="checkbox"
                      checked={e.enabled}
                      onchange={(ev) =>
                        sim.facilitate({
                          kind: "set_event_enabled",
                          event: e.id,
                          enabled: ev.currentTarget.checked,
                        })}
                    />
                    {e.name || e.id}
                  </label>
                  <span class="dim">{e.fired ? `fired ×${e.fired}` : "not fired"}</span>
                  <button
                    type="button"
                    aria-label={`Fire ${e.id}`}
                    onclick={() => sim.facilitate({ kind: "fire_event", event: e.id })}>Fire</button
                  >
                </li>
              {:else}
                <li class="empty">No events in this scenario.</li>
              {/each}
            </ul>
          </section>
        {:else if rightTab === "planning"}
          {#if sim.posts.length > 0}
            <section>
              <h3>Command posts</h3>
              <ul class="zones" aria-label="Command posts">
                {#each sim.posts as p (p.facility)}
                  <li>
                    <span>{facilityName(p.facility)}</span>
                    <span class:hurt={!p.up}
                      >{p.up ? "up" : `down: ${p.status.replaceAll("_", " ")}`}</span
                    >
                  </li>
                {/each}
              </ul>
            </section>
          {/if}
          <section>
            <h3>Facilities</h3>
            <ul class="zones" aria-label="Facilities in play">
              {#each sim.session?.facilities ?? [] as f, i (f.id)}
                <li class="stacked">
                  <span>{f.name} <span class="dim">({f.kind.replaceAll("_", " ")})</span></span>
                  <span class="row tight">
                    {#if f.kind !== "command_post"}
                      {#each ["repair", "rearm", "refuel"] as const as sv (sv)}
                        <label class="inline">
                          <input
                            type="checkbox"
                            checked={f.services?.[sv] ?? false}
                            onchange={(e) => {
                              const on = e.currentTarget.checked;
                              editPlanning((fs) => {
                                const x = fs[i];
                                if (x) x.services = { ...x.services, [sv]: on };
                              });
                            }}
                          />{sv}
                        </label>
                      {/each}
                    {/if}
                    <button
                      type="button"
                      onclick={() => editPlanning((fs) => void fs.splice(i, 1))}
                      aria-label={`Remove ${f.name}`}>Remove</button
                    >
                  </span>
                </li>
              {:else}
                <li class="empty">No facilities.</li>
              {/each}
            </ul>
          </section>
          <section>
            <h3>Zones</h3>
            <ul class="zones" aria-label="Zone rules">
              {#each sim.session?.zones ?? [] as z (z.id)}
                <li class="stacked">
                  <span>{z.name} <span class="dim">({z.kind})</span></span>
                  <span class="row tight">
                    <select
                      aria-label={`Weapons control in ${z.name}`}
                      value={z.doctrine?.wcs ?? ""}
                      onchange={(e) => {
                        const v = e.currentTarget.value;
                        void sim.setZoneDoctrine(z.id, {
                          ...z.doctrine,
                          wcs: v === "" ? null : (v as Wcs),
                        });
                      }}
                    >
                      <option value="">WCS not set</option>
                      <option value="safe">safe</option>
                      <option value="tight">tight</option>
                      <option value="free">free</option>
                    </select>
                    <select
                      aria-label={`Emissions in ${z.name}`}
                      value={z.doctrine?.emcon ?? ""}
                      onchange={(e) => {
                        const v = e.currentTarget.value;
                        void sim.setZoneDoctrine(z.id, {
                          ...z.doctrine,
                          emcon: v === "" ? null : (v as Emcon),
                        });
                      }}
                    >
                      <option value="">EMCON not set</option>
                      <option value="active">active</option>
                      <option value="silent">silent</option>
                    </select>
                    <button
                      type="button"
                      aria-label={`Remove ${z.name}`}
                      onclick={() =>
                        editPlanning((_, zs) => {
                          const k = zs.findIndex((x) => x.id === z.id);
                          if (k >= 0) zs.splice(k, 1);
                        })}>Remove</button
                    >
                  </span>
                </li>
              {:else}
                <li class="empty">No zones.</li>
              {/each}
            </ul>
            <div class="row">
              <select bind:value={newZoneKind} aria-label="Kind of zone to add">
                <option value="sense">sense zone</option>
                <option value="engage">engage zone</option>
              </select>
              <input
                type="number"
                min="1"
                bind:value={newZoneKm}
                aria-label="Radius of the zone to add, km"
                style="width: 4.5em"
              />
              km
              <button type="button" onclick={addZoneOnMap}>Add on the map</button>
            </div>
          </section>
        {:else if rightTab === "seats"}
          <section>
            <h3>Seats</h3>
            <ul class="units" aria-label="Seats">
              {#each sim.session?.seats ?? [] as s (s.id)}
                <li class="row">
                  <span>{s.name} <span class="dim">· {seatRoleText(s)}</span></span>
                  <button
                    type="button"
                    aria-label={`Open a window for ${s.name}`}
                    onclick={() => sim.openSeatWindow(s.id)}>Open window</button
                  >
                  {#if s.id !== sim.session?.seat.id}
                    <button
                      type="button"
                      aria-label={`Remove ${s.name}`}
                      onclick={() => sim.removeSeat(s.id)}>Remove</button
                    >
                  {/if}
                </li>
              {/each}
            </ul>
            <form
              class="send-message"
              aria-label="Add seat"
              onsubmit={(e) => {
                e.preventDefault();
                void sim
                  .addSeat(
                    newSeatName,
                    newSeatRole === "white" ? null : Number(newSeatRole.slice(5)),
                  )
                  .then((ok) => {
                    if (ok) newSeatName = "";
                  });
              }}
            >
              <label for="seat-name">New seat</label>
              <input
                id="seat-name"
                type="text"
                bind:value={newSeatName}
                placeholder="e.g. Blue XO"
              />
              <select aria-label="New seat plays" bind:value={newSeatRole}>
                <option value="white">White cell</option>
                {#each sim.session?.sides ?? [] as s (s.id)}
                  <option value={`side-${s.id}`}>{s.name}</option>
                {/each}
              </select>
              <button type="submit">Add</button>
            </form>
          </section>
        {:else if rightTab === "log"}
          <section>
            <h3>Override log</h3>
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
        {:else if rightTab === "review"}
          <AarPanel />
        {/if}
      </div>
    </aside>
  {/if}

  <footer class="statusbar" aria-live="polite">
    {#if sim.error}
      <span class="error" role="alert">{sim.error}</span>
    {:else if sim.picking}
      <span class="prompt">{sim.picking.prompt}</span>
    {:else if sim.draft}
      <span class="prompt"
        >Click the map to add waypoints · Enter sends · Backspace undoes · Esc cancels</span
      >
    {:else if editor.error && !editor.open}
      <span class="error">{editor.error}</span>
    {:else if latestOutcome}
      <span class="outcome {latestOutcome.result}">{latestOutcomeText}</span>
    {:else}
      <span class="dim">Space run/pause · 1–7 compression · . step · Esc deselect</span>
    {/if}
  </footer>

  {#if showDb}
    <div class="dialog-backdrop" role="presentation" onclick={() => (showDb = false)}>
      <div
        class="dialog"
        role="dialog"
        aria-label="Platform database"
        aria-modal="true"
        tabindex="-1"
        onclick={(e) => e.stopPropagation()}
        onkeydown={(e) => {
          if (e.key === "Escape") showDb = false;
        }}
      >
        <button class="close" onclick={() => (showDb = false)} aria-label="Close the database"
          >Close</button
        >
        <DbBrowser />
      </div>
    </div>
  {/if}

  {#if showKeys}
    <div class="dialog-backdrop" role="presentation" onclick={() => (showKeys = false)}>
      <div
        class="dialog small"
        role="dialog"
        aria-label="Keyboard shortcuts"
        aria-modal="true"
        tabindex="-1"
        onclick={(e) => e.stopPropagation()}
        onkeydown={(e) => {
          if (e.key === "Escape") showKeys = false;
        }}
      >
        <h2>Keyboard shortcuts</h2>
        <dl class="keys">
          <dt><kbd>Space</kbd></dt>
          <dd>Run or pause</dd>
          <dt><kbd>1</kbd>–<kbd>7</kbd></dt>
          <dd>Time compression, 1× to Max</dd>
          <dt><kbd>.</kbd></dt>
          <dd>Step one second while paused</dd>
          <dt><kbd>Enter</kbd></dt>
          <dd>Send the route being planned</dd>
          <dt><kbd>Backspace</kbd></dt>
          <dd>Remove the last waypoint</dd>
          <dt><kbd>Esc</kbd></dt>
          <dd>Cancel the route, or deselect the unit</dd>
        </dl>
        <button onclick={() => (showKeys = false)}>Close</button>
      </div>
    </div>
  {/if}

  {#if editor.open}
    <ScenarioEditor />
  {/if}
</div>

<style>
  .shell {
    display: grid;
    grid-template-columns: 300px 1fr;
    grid-template-rows: 30px 44px 1fr 24px;
    grid-template-areas:
      "menu menu"
      "tool tool"
      "left map"
      "status status";
    height: 100vh;
  }
  .shell.with-right {
    grid-template-columns: 300px 1fr 340px;
    grid-template-areas:
      "menu menu menu"
      "tool tool tool"
      "left map right"
      "status status status";
  }

  /* Menu bar: the application's verbs. */
  .menurow {
    grid-area: menu;
    display: flex;
    align-items: stretch;
    gap: 12px;
    padding: 0 4px;
    background: var(--surface-2);
    border-bottom: 1px solid var(--border);
    min-width: 0;
  }
  .scenario-name {
    margin-left: auto;
    align-self: center;
    padding-right: 8px;
    color: var(--text-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* Toolbar: the clock and who you are. */
  .toolbar {
    grid-area: tool;
    display: flex;
    align-items: center;
    gap: 18px;
    padding: 0 10px;
    min-width: 0;
    background: var(--surface-1);
    border-bottom: 1px solid var(--border);
  }
  .controls {
    display: flex;
    gap: 6px;
    flex-shrink: 0;
  }
  .speeds {
    display: flex;
  }
  .speeds button {
    border-radius: 0;
    margin-left: -1px;
  }
  .speeds button:first-child {
    border-radius: 4px 0 0 4px;
  }
  .speeds button:last-child {
    border-radius: 0 4px 4px 0;
  }
  .speeds .active {
    background: var(--accent);
    color: var(--on-accent);
    border-color: var(--accent);
    position: relative;
  }
  .play {
    min-width: 84px;
  }
  .play.running {
    border-color: var(--accent);
  }
  .clock {
    display: flex;
    align-items: baseline;
    gap: 12px;
    font-variant-numeric: tabular-nums;
    min-width: 0;
  }
  .clock .time {
    font-family: var(--mono);
    font-size: 20px;
    letter-spacing: 0.02em;
  }
  .clock .ratio {
    color: var(--text-dim);
    min-width: 52px;
  }
  .clock .env {
    color: var(--text-dim);
    font-size: 12px;
    white-space: nowrap;
  }
  .replay-badge {
    align-self: center;
    padding: 1px 6px;
    border: 1px solid var(--warn);
    border-radius: 3px;
    color: var(--warn);
    font-weight: 700;
  }
  .seat-picker {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 8px;
    white-space: nowrap;
  }
  .seat-picker label,
  .seat-picker .dim {
    color: var(--text-dim);
    margin: 0;
  }
  .seat-picker select {
    max-width: 260px;
  }

  /* Side panels: tabs over one scrolling panel each. */
  .panel {
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--surface-1);
  }
  .panel.left {
    grid-area: left;
    border-right: 1px solid var(--border);
  }
  .panel.right {
    grid-area: right;
    border-left: 1px solid var(--border);
  }
  .tabs {
    display: flex;
    flex-wrap: wrap;
    border-bottom: 1px solid var(--border);
    background: var(--surface-2);
    flex-shrink: 0;
  }
  .tabs button {
    border: none;
    border-radius: 0;
    background: transparent;
    color: var(--text-dim);
    padding: 7px 7px 6px;
    font-size: 12px;
    border-bottom: 2px solid transparent;
  }
  .tabs button:hover {
    color: var(--text);
  }
  .tabs button[aria-selected="true"] {
    color: var(--text);
    border-bottom-color: var(--accent);
  }
  .badge {
    display: inline-block;
    margin-left: 6px;
    min-width: 16px;
    padding: 0 5px;
    border-radius: 8px;
    background: var(--warn);
    color: var(--on-accent);
    font-size: 11px;
    font-weight: 700;
    text-align: center;
  }
  .tabpanel {
    flex: 1;
    overflow-y: auto;
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .reason {
    position: sticky;
    top: -12px;
    margin: -12px -12px 0;
    padding: 10px 12px;
    background: var(--surface-1);
    border-bottom: 1px solid var(--border);
    z-index: 1;
  }
  .reason label {
    display: block;
    color: var(--text-dim);
    font-size: 12px;
    margin-bottom: 4px;
  }
  .reason input {
    width: 100%;
    box-sizing: border-box;
  }
  h2 {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-dim);
    margin: 0 0 8px;
  }
  h2.unit-name {
    font-size: 16px;
    color: var(--text);
  }
  h3 {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-dim);
    margin: 0 0 8px;
  }
  .count {
    font-weight: 400;
    font-family: var(--mono);
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
  .units .row button {
    width: auto;
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
    font-size: 11px;
  }
  dl {
    display: grid;
    grid-template-columns: 76px 1fr;
    gap: 4px 8px;
    margin: 0;
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
  }
  .engage button {
    grid-column: 2;
  }
  .override {
    border-top: 1px solid var(--border);
    padding-top: 12px;
  }
  .zones {
    list-style: none;
    margin: 4px 0;
    padding: 0;
  }
  .zones li {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 6px;
    margin: 4px 0;
  }
  .zones li.stacked {
    flex-direction: column;
    align-items: stretch;
    padding-bottom: 6px;
    border-bottom: 1px solid var(--border);
  }
  .dim {
    color: var(--text-dim);
    font-size: 12px;
  }
  p.dim {
    margin: 0 0 8px;
  }
  .injects li,
  .messages li,
  .adjudications li {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .send-message {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 6px;
  }
  .send-message label {
    grid-column: 1 / -1;
    color: var(--text-dim);
    font-size: 12px;
  }
  .hurt {
    color: var(--danger);
  }
  .row {
    display: flex;
    gap: 6px;
    align-items: center;
    flex-wrap: wrap;
    margin: 6px 0;
  }
  .row.tight {
    margin: 2px 0 0;
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

  .map {
    grid-area: map;
    position: relative;
    min-width: 0;
  }
  .map.planning :global(canvas) {
    cursor: crosshair;
  }

  /* Status bar: what needs your attention, or what the keys do. */
  .statusbar {
    grid-area: status;
    display: flex;
    align-items: center;
    padding: 0 10px;
    font-size: 12px;
    background: var(--surface-2);
    border-top: 1px solid var(--border);
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .statusbar .prompt {
    color: var(--warn);
  }
  .statusbar .outcome {
    font-weight: 600;
  }
  .statusbar .outcome.hit {
    color: #ff7a4d;
  }
  .statusbar .outcome.miss {
    color: var(--text);
  }
  .statusbar .outcome.shot_down {
    color: var(--warn);
  }

  .visually-hidden {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
    white-space: nowrap;
  }

  .dialog-backdrop {
    position: fixed;
    inset: 0;
    z-index: 40;
    display: grid;
    place-items: center;
    background: rgb(0 0 0 / 45%);
  }
  .dialog {
    position: relative;
    width: min(720px, 92vw);
    max-height: 84vh;
    overflow: auto;
    padding: 16px;
    background: var(--surface-1);
    border: 1px solid var(--border);
    border-radius: 8px;
  }
  .dialog.small {
    width: min(420px, 92vw);
  }
  .dialog .close {
    position: absolute;
    top: 12px;
    right: 12px;
  }
  .keys {
    grid-template-columns: 110px 1fr;
    margin-bottom: 12px;
  }
  .keys dd {
    font-family: inherit;
    font-size: inherit;
  }
  kbd {
    font-family: var(--mono);
    font-size: 12px;
    padding: 0 4px;
    border: 1px solid var(--border);
    border-radius: 3px;
  }
</style>
