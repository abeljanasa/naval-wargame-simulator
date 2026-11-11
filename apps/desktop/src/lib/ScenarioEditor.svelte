<script lang="ts">
  // Scenario editor drawer (docs/models/scenario-editor.md). Forms for the overview, sides and
  // postures, units and missions, minefields, reference points and the planning layer
  // (facilities, zones, action groups); JSON for events, injects, seats and groups. The engine checks the draft; errors block Save and Play.
  import type { Emcon, FacilityKind, Mission, Posture, Wcs } from "./bindings";
  import { editor } from "./editor.svelte";
  import { MPS_PER_KNOT, n } from "./format";
  import {
    addChokePoint,
    addFacility,
    addSide,
    addUnit,
    duplicateUnit,
    EVENT_TEMPLATES,
    parseJsonArray,
    postureOf,
    removeSide,
    removeUnit,
    setPosture,
    uniqueEventId,
    type DraftUnit,
  } from "./scenario-edit";

  type Tab = "overview" | "sides" | "units" | "minefields" | "points" | "planning" | "events";
  const TABS: { key: Tab; label: string }[] = [
    { key: "overview", label: "Overview" },
    { key: "sides", label: "Sides" },
    { key: "units", label: "Units" },
    { key: "minefields", label: "Mines" },
    { key: "points", label: "Points" },
    { key: "planning", label: "Planning" },
    { key: "events", label: "Events" },
  ];
  const POSTURES: Posture[] = ["friendly", "neutral", "unfriendly", "hostile"];
  const FACILITY_KINDS: { key: FacilityKind; label: string }[] = [
    { key: "command_post", label: "Command post" },
    { key: "sheltering_port", label: "Sheltering port" },
    { key: "anchorage", label: "Anchorage" },
    { key: "refuelling_point", label: "Refuelling point" },
  ];
  const SERVICES = ["repair", "rearm", "refuel"] as const;
  let planSide = $state(1);
  let newKind = $state<FacilityKind>("sheltering_port");
  const JSON_FIELDS = ["events", "injects", "seats", "groups"] as const;
  type JsonField = (typeof JSON_FIELDS)[number];

  let tab = $state<Tab>("overview");
  let newSide = $state("");
  let jsonText = $state<Record<JsonField, string>>({
    events: "",
    injects: "",
    seats: "",
    groups: "",
  });
  let jsonError = $state<Record<JsonField, string | null>>({
    events: null,
    injects: null,
    seats: null,
    groups: null,
  });

  const d = $derived(editor.draft);
  const unit = $derived(d.units.find((u) => u.id === editor.selectedUnit) ?? null);
  const mines = $derived(editor.weapons.filter((w) => w.kind === "mine"));

  // Refill the JSON boxes whenever a different draft is opened.
  let shownDraft: object | null = null;
  $effect(() => {
    if (shownDraft === editor.draft) return;
    shownDraft = editor.draft;
    for (const f of JSON_FIELDS) {
      const v = editor.draft[f];
      jsonText[f] = Array.isArray(v) && v.length ? JSON.stringify(v, null, 2) : "";
      jsonError[f] = null;
    }
  });

  function changed() {
    editor.changed();
  }

  function applyJson(f: JsonField) {
    const r = parseJsonArray(jsonText[f]);
    if ("error" in r) {
      jsonError[f] = r.error;
      return;
    }
    jsonError[f] = null;
    // An empty list is written as absent, as the engine writes it.
    editor.draft[f] = r.value.length ? r.value : undefined;
    changed();
  }

  function addTemplate(i: number) {
    const t = EVENT_TEMPLATES[i];
    if (!t) return;
    const parsed = parseJsonArray(jsonText.events);
    const events = "value" in parsed ? parsed.value : [];
    const event = { ...t.event, id: uniqueEventId(events, t.event.id) };
    jsonText.events = JSON.stringify([...events, event], null, 2);
    applyJson("events");
  }

  function sideName(id: number): string {
    return d.sides.find((s) => s.id === id)?.name ?? `S${id}`;
  }

  function knotsOf(u: DraftUnit): string {
    return u.speed_mps == null ? "" : (n(u.speed_mps) / MPS_PER_KNOT).toFixed(1);
  }

  function setMission(u: DraftUnit, kind: string) {
    const here = u.position;
    const m: Mission | undefined =
      kind === "patrol"
        ? { kind: "patrol", centre: here, radius_m: 30_000 }
        : kind === "ferry"
          ? { kind: "ferry", destination: here }
          : kind === "escort"
            ? {
                kind: "escort",
                escorted: d.units.find((x) => x.id !== u.id && x.side === u.side)?.id ?? 0,
                bearing_deg: 0,
                range_m: 5_000,
              }
            : kind === "strike"
              ? { kind: "strike", contact: 1 }
              : undefined;
    if (m) u.mission = m;
    else delete u.mission;
    changed();
  }

  function addUnitHere() {
    const side = unit?.side ?? d.sides[0]?.id ?? 1;
    const platform = unit?.platform ?? editor.platforms[0]?.id;
    editor.pick("Click the map to place the new unit", (p) => {
      editor.selectedUnit = addUnit(editor.draft, side, p, platform);
    });
  }

  function num(v: string, fallback = 0): number {
    const x = Number(v);
    return Number.isFinite(x) ? x : fallback;
  }

  /** `datetime-local` text (UTC) for Unix seconds, and back. */
  function isoOf(s: number | undefined): string {
    return new Date((s ?? 0) * 1000).toISOString().slice(0, 16);
  }
  function unixOf(text: string): number {
    const t = Date.parse(`${text}:00Z`);
    return Number.isFinite(t) ? Math.round(t / 1000) : 0;
  }
</script>

<div class="editor" role="dialog" aria-label="Scenario editor">
  <header>
    <strong>Scenario editor</strong>
    <span class="spacer"></span>
    <button onclick={() => editor.check()} disabled={editor.busy}>Check</button>
    <button onclick={() => editor.save()} disabled={editor.busy}>Save file</button>
    <button class="primary" onclick={() => editor.play()} disabled={editor.busy}>Play</button>
    <button onclick={() => editor.close()} aria-label="Close the editor">Close</button>
  </header>

  {#if editor.picking}
    <p class="picking" role="status">
      {editor.picking.prompt}
      {#if editor.picking.many}
        <button onclick={() => editor.finishPick()}>Done</button>
      {:else}
        <button onclick={() => editor.finishPick()}>Cancel</button>
      {/if}
    </p>
  {/if}

  <div class="tabs" role="tablist">
    {#each TABS as t (t.key)}
      <button
        role="tab"
        aria-selected={tab === t.key}
        class:active={tab === t.key}
        onclick={() => (tab = t.key)}>{t.label}</button
      >
    {/each}
  </div>

  <div class="body">
    {#if tab === "overview"}
      <label>Name <input bind:value={editor.draft.name} oninput={changed} /></label>
      <label>
        Description
        <textarea rows="3" bind:value={editor.draft.description} oninput={changed}></textarea>
      </label>
      <div class="grid2">
        <label>
          Seed
          <input
            type="number"
            value={d.seed}
            onchange={(e) => {
              editor.draft.seed = Math.max(0, Math.round(num(e.currentTarget.value, 1)));
              changed();
            }}
          />
        </label>
        <label>
          Duration (hours)
          <input
            type="number"
            step="0.25"
            min="0"
            value={d.duration_s / 3600}
            onchange={(e) => {
              editor.draft.duration_s = Math.round(num(e.currentTarget.value) * 3600);
              changed();
            }}
          />
        </label>
        <label>
          Start (UTC)
          <input
            type="datetime-local"
            value={isoOf(d.environment?.start_unix_s)}
            onchange={(e) => {
              editor.draft.environment = {
                ...editor.draft.environment,
                start_unix_s: unixOf(e.currentTarget.value),
              };
              changed();
            }}
          />
        </label>
        <label>
          Sea state (0–9)
          <input
            type="number"
            min="0"
            max="9"
            value={d.environment?.sea_state ?? 0}
            onchange={(e) => {
              editor.draft.environment = {
                ...editor.draft.environment,
                sea_state: Math.round(num(e.currentTarget.value)),
              };
              changed();
            }}
          />
        </label>
        <label>
          Weather loss (dB)
          <input
            type="number"
            min="0"
            step="0.5"
            value={d.environment?.weather_loss_db ?? 0}
            onchange={(e) => {
              editor.draft.environment = {
                ...editor.draft.environment,
                weather_loss_db: num(e.currentTarget.value),
              };
              changed();
            }}
          />
        </label>
        <label>
          Visibility (km)
          <input
            type="number"
            min="0"
            value={d.environment?.visibility_km ?? 40}
            onchange={(e) => {
              editor.draft.environment = {
                ...editor.draft.environment,
                visibility_km: num(e.currentTarget.value, 40),
              };
              changed();
            }}
          />
        </label>
      </div>
    {:else if tab === "sides"}
      {#each editor.draft.sides as s (s.id)}
        <fieldset>
          <legend>Side {s.id}</legend>
          <label>Name <input bind:value={s.name} oninput={changed} /></label>
          {#each d.sides.filter((o) => o.id !== s.id) as o (o.id)}
            <label class="inline">
              Regards {o.name} as
              <select
                value={postureOf(d, s.id, o.id)}
                onchange={(e) => {
                  setPosture(editor.draft, s.id, o.id, e.currentTarget.value as Posture);
                  changed();
                }}
              >
                {#each POSTURES as p (p)}<option value={p}>{p}</option>{/each}
              </select>
            </label>
          {/each}
          <button
            class="danger"
            onclick={() => {
              const units = d.units.filter((u) => u.side === s.id).length;
              if (units === 0 || confirm(`Remove ${s.name} and its ${units} units?`)) {
                removeSide(editor.draft, s.id);
                changed();
              }
            }}>Remove side</button
          >
        </fieldset>
      {/each}
      <div class="row">
        <input placeholder="New side name" aria-label="New side name" bind:value={newSide} />
        <button
          onclick={() => {
            addSide(editor.draft, newSide);
            newSide = "";
            changed();
          }}>Add side</button
        >
      </div>
    {:else if tab === "units"}
      <div class="row">
        <button onclick={addUnitHere}>Add unit on the map</button>
        <span class="dim">{d.units.length} units · click one on the map to select it</span>
      </div>
      <ul class="list" aria-label="Scenario units">
        {#each d.units as u (u.id)}
          <li>
            <button
              class:selected={u.id === editor.selectedUnit}
              onclick={() => (editor.selectedUnit = u.id)}
            >
              <span>{u.name}</span>
              <span class="dim">{sideName(u.side)} · {u.platform ?? u.domain ?? "?"}</span>
            </button>
          </li>
        {/each}
      </ul>
      {#if unit}
        {@const u = unit}
        <fieldset>
          <legend>Unit {u.id}</legend>
          <label>Name <input bind:value={u.name} oninput={changed} /></label>
          <div class="grid2">
            <label>
              Side
              <select bind:value={u.side} onchange={changed}>
                {#each d.sides as s (s.id)}<option value={s.id}>{s.name}</option>{/each}
              </select>
            </label>
            <label>
              Platform
              <select
                value={u.platform ?? ""}
                onchange={(e) => {
                  const v = e.currentTarget.value;
                  if (v) u.platform = v;
                  else delete u.platform;
                  changed();
                }}
              >
                <option value="">(none: give domain and speeds as JSON)</option>
                {#each editor.platforms as p (p.id)}
                  <option value={p.id}>{p.id} · {p.name}</option>
                {/each}
              </select>
            </label>
            <label>
              Latitude
              <input
                type="number"
                step="0.01"
                value={n(u.position.lat_deg)}
                onchange={(e) => {
                  u.position = { ...u.position, lat_deg: num(e.currentTarget.value) };
                  changed();
                }}
              />
            </label>
            <label>
              Longitude
              <input
                type="number"
                step="0.01"
                value={n(u.position.lon_deg)}
                onchange={(e) => {
                  u.position = { ...u.position, lon_deg: num(e.currentTarget.value) };
                  changed();
                }}
              />
            </label>
            <label>
              Heading (°)
              <input
                type="number"
                min="0"
                max="359"
                value={u.heading_deg ?? 0}
                onchange={(e) => {
                  u.heading_deg = num(e.currentTarget.value);
                  changed();
                }}
              />
            </label>
            <label>
              Speed (kn, blank: cruise)
              <input
                type="number"
                min="0"
                step="0.5"
                value={knotsOf(u)}
                onchange={(e) => {
                  const v = e.currentTarget.value.trim();
                  if (v === "") delete u.speed_mps;
                  else u.speed_mps = num(v) * MPS_PER_KNOT;
                  changed();
                }}
              />
            </label>
          </div>
          <div class="row">
            <button
              onclick={() =>
                editor.pick(`Click the map to move ${u.name}`, (p) => (u.position = p))}
            >
              Move on the map
            </button>
            <button
              onclick={() => {
                u.route = [];
                editor.pick(
                  `Click waypoints for ${u.name}, then Done`,
                  (p) => (u.route = [...(u.route ?? []), p]),
                  true,
                );
              }}>Draw route</button
            >
            <span class="dim">{(u.route ?? []).length} waypoints</span>
            {#if (u.route ?? []).length}
              <button
                onclick={() => {
                  u.route = [];
                  changed();
                }}>Clear route</button
              >
            {/if}
          </div>
          <label>
            Mission
            <select
              value={u.mission?.kind ?? ""}
              onchange={(e) => setMission(u, e.currentTarget.value)}
            >
              <option value="">none</option>
              <option value="patrol">patrol</option>
              <option value="ferry">ferry</option>
              <option value="escort">escort</option>
              <option value="strike">strike</option>
            </select>
          </label>
          {#if u.mission?.kind === "patrol"}
            {@const m = u.mission}
            <div class="row">
              <label class="inline">
                Radius (km)
                <input
                  type="number"
                  min="1"
                  value={n(m.radius_m) / 1000}
                  onchange={(e) => {
                    m.radius_m = num(e.currentTarget.value, 30) * 1000;
                    changed();
                  }}
                />
              </label>
              <button
                onclick={() =>
                  editor.pick("Click the centre of the patrol area", (p) => (m.centre = p))}
              >
                Centre on the map
              </button>
            </div>
          {:else if u.mission?.kind === "ferry"}
            {@const m = u.mission}
            <button
              onclick={() => editor.pick("Click the destination", (p) => (m.destination = p))}
            >
              Destination on the map
            </button>
          {:else if u.mission?.kind === "escort"}
            {@const m = u.mission}
            <div class="grid2">
              <label>
                Escort
                <select bind:value={m.escorted} onchange={changed}>
                  {#each d.units.filter((x) => x.id !== u.id) as x (x.id)}
                    <option value={x.id}>{x.name}</option>
                  {/each}
                </select>
              </label>
              <label>
                Bearing (° from its heading)
                <input type="number" bind:value={m.bearing_deg} onchange={changed} />
              </label>
              <label>
                Range (km)
                <input
                  type="number"
                  min="0"
                  value={n(m.range_m) / 1000}
                  onchange={(e) => {
                    m.range_m = num(e.currentTarget.value, 5) * 1000;
                    changed();
                  }}
                />
              </label>
            </div>
          {:else if u.mission?.kind === "strike"}
            {@const m = u.mission}
            <label class="inline">
              Contact number
              <input type="number" min="1" bind:value={m.contact} onchange={changed} />
            </label>
          {/if}
          <label class="inline">
            <input
              type="checkbox"
              checked={u.reserve ?? false}
              onchange={(e) => {
                if (e.currentTarget.checked) u.reserve = true;
                else delete u.reserve;
                changed();
              }}
            />
            Reserve (off the board until an event brings it in)
          </label>
          <div class="row">
            <button
              onclick={() => {
                editor.selectedUnit = duplicateUnit(editor.draft, u.id);
                changed();
              }}>Duplicate</button
            >
            <button
              class="danger"
              onclick={() => {
                removeUnit(editor.draft, u.id);
                editor.selectedUnit = editor.draft.units[0]?.id ?? null;
                changed();
              }}>Delete</button
            >
          </div>
        </fieldset>
      {/if}
    {:else if tab === "minefields"}
      {#each editor.draft.minefields ?? [] as m, i (i)}
        <fieldset>
          <legend>Minefield {i + 1}</legend>
          <div class="grid2">
            <label>
              Side
              <select bind:value={m.side} onchange={changed}>
                {#each d.sides as s (s.id)}<option value={s.id}>{s.name}</option>{/each}
              </select>
            </label>
            <label>
              Mine type
              <select bind:value={m.weapon} onchange={changed}>
                {#each mines as w (w.id)}<option value={w.id}>{w.id} · {w.name}</option>{/each}
              </select>
            </label>
            <label>
              Mines
              <input type="number" min="1" bind:value={m.mines} onchange={changed} />
            </label>
            <label>
              Radius (km)
              <input
                type="number"
                min="0.1"
                step="0.1"
                value={n(m.radius_m) / 1000}
                onchange={(e) => {
                  m.radius_m = num(e.currentTarget.value, 1) * 1000;
                  changed();
                }}
              />
            </label>
          </div>
          <div class="row">
            <button
              onclick={() =>
                editor.pick("Click the centre of the minefield", (p) => (m.centre = p))}
            >
              Centre on the map
            </button>
            <button
              class="danger"
              onclick={() => {
                editor.draft.minefields = (editor.draft.minefields ?? []).filter((_, j) => j !== i);
                changed();
              }}>Remove</button
            >
          </div>
        </fieldset>
      {/each}
      <button
        onclick={() =>
          editor.pick("Click the centre of the new minefield", (p) => {
            editor.draft.minefields = [
              ...(editor.draft.minefields ?? []),
              {
                side: d.sides[0]?.id ?? 1,
                centre: p,
                radius_m: 2_000,
                mines: 20,
                weapon: mines[0]?.id ?? "",
              },
            ];
          })}>Add minefield on the map</button
      >
    {:else if tab === "points"}
      <p class="dim">Named marks for briefing and planning. They do not change the simulation.</p>
      {#each editor.draft.reference_points ?? [] as r, i (i)}
        <fieldset>
          <div class="grid2">
            <label>Name <input bind:value={r.name} oninput={changed} /></label>
            <label>
              Shown to
              <select
                value={r.side ?? ""}
                onchange={(e) => {
                  const v = e.currentTarget.value;
                  if (v === "") delete r.side;
                  else r.side = Number(v);
                  changed();
                }}
              >
                <option value="">every side</option>
                {#each d.sides as s (s.id)}<option value={s.id}>{s.name} only</option>{/each}
              </select>
            </label>
          </div>
          <div class="row">
            <button
              onclick={() => editor.pick(`Click the map for ${r.name}`, (p) => (r.position = p))}
            >
              Move on the map
            </button>
            <button
              class="danger"
              onclick={() => {
                editor.draft.reference_points = (editor.draft.reference_points ?? []).filter(
                  (_, j) => j !== i,
                );
                changed();
              }}>Remove</button
            >
          </div>
        </fieldset>
      {/each}
      <button
        onclick={() =>
          editor.pick("Click the map for the new reference point", (p) => {
            const k = (editor.draft.reference_points ?? []).length + 1;
            editor.draft.reference_points = [
              ...(editor.draft.reference_points ?? []),
              { name: `Point ${k}`, position: p },
            ];
          })}>Add point on the map</button
      >
    {:else if tab === "planning"}
      <p class="dim">
        The operational planning layer (docs/models/planning-layer.md): facilities, sense and engage
        zones, and surface action groups.
      </p>
      <div class="row">
        <label class="inline">
          Side
          <select bind:value={planSide}>
            {#each d.sides as s (s.id)}<option value={s.id}>{s.name}</option>{/each}
          </select>
        </label>
        <label class="inline">
          New facility
          <select bind:value={newKind}>
            {#each FACILITY_KINDS as k (k.key)}<option value={k.key}>{k.label}</option>{/each}
          </select>
        </label>
        <button
          onclick={() =>
            editor.pick("Click the map to place the facility", (p) =>
              addFacility(editor.draft, planSide, newKind, p),
            )}>Add facility on the map</button
        >
        <button
          onclick={() =>
            editor.pick("Click the centre of the choke point", (p) => {
              const k = new Set((editor.draft.zones ?? []).map((z) => z.choke_point)).size + 1;
              addChokePoint(editor.draft, planSide, p, `Choke point ${k}`);
            })}>Add choke point on the map</button
        >
      </div>

      <h3>Facilities</h3>
      {#each editor.draft.facilities ?? [] as f, i (f.id)}
        <fieldset>
          <legend>{f.name}</legend>
          <div class="grid2">
            <label>Name <input bind:value={f.name} oninput={changed} /></label>
            <label>
              Kind
              <select bind:value={f.kind} onchange={changed}>
                {#each FACILITY_KINDS as k (k.key)}<option value={k.key}>{k.label}</option>{/each}
              </select>
            </label>
            <label>
              Side
              <select bind:value={f.side} onchange={changed}>
                {#each d.sides as s (s.id)}<option value={s.id}>{s.name}</option>{/each}
              </select>
            </label>
            <label>
              Radius (km)
              <input
                type="number"
                min="0.1"
                step="0.1"
                value={n(f.radius_m) / 1000}
                onchange={(e) => {
                  f.radius_m = num(e.currentTarget.value, 1) * 1000;
                  changed();
                }}
              />
            </label>
            {#if f.kind === "command_post"}
              <label>
                Command range (km)
                <input
                  type="number"
                  min="1"
                  value={n(f.range_m ?? f.radius_m) / 1000}
                  onchange={(e) => {
                    f.range_m = num(e.currentTarget.value, 100) * 1000;
                    changed();
                  }}
                />
              </label>
              <label>
                Carried by
                <select
                  value={f.unit ?? ""}
                  onchange={(e) => {
                    const v = e.currentTarget.value;
                    f.unit = v === "" ? null : Number(v);
                    changed();
                  }}
                >
                  <option value="">nothing (fixed, ashore)</option>
                  {#each d.units.filter((u) => u.side === f.side) as u (u.id)}
                    <option value={u.id}>{u.name}</option>
                  {/each}
                </select>
              </label>
              <label>
                Alternate
                <select
                  value={f.alternate ?? ""}
                  onchange={(e) => {
                    const v = e.currentTarget.value;
                    f.alternate = v === "" ? null : Number(v);
                    changed();
                  }}
                >
                  <option value="">none</option>
                  {#each (d.facilities ?? []).filter((g) => g.id !== f.id && g.side === f.side && g.kind === "command_post") as g (g.id)}
                    <option value={g.id}>{g.name}</option>
                  {/each}
                </select>
              </label>
              <label>
                Handover (min)
                <input
                  type="number"
                  min="0"
                  value={(f.handover_s ?? 300) / 60}
                  onchange={(e) => {
                    f.handover_s = Math.max(0, num(e.currentTarget.value, 5)) * 60;
                    changed();
                  }}
                />
              </label>
              <label>
                Orders in the gap
                <select bind:value={f.orders_in_gap} onchange={changed}>
                  <option value="lose">lost</option>
                  <option value="queue">queued for the alternate</option>
                </select>
              </label>
            {:else}
              <label>
                Berths (0: no limit)
                <input
                  type="number"
                  min="0"
                  value={f.berths ?? 0}
                  onchange={(e) => {
                    f.berths = Math.max(0, Math.round(num(e.currentTarget.value)));
                    changed();
                  }}
                />
              </label>
            {/if}
          </div>
          {#if f.kind !== "command_post"}
            <div class="row">
              {#each SERVICES as sv (sv)}
                <label class="inline">
                  <input
                    type="checkbox"
                    checked={f.services?.[sv] ?? false}
                    onchange={(e) => {
                      f.services = { ...f.services, [sv]: e.currentTarget.checked };
                      changed();
                    }}
                  />
                  {sv}
                </label>
              {/each}
            </div>
          {/if}
          <div class="row">
            <button
              onclick={() => editor.pick(`Click the map for ${f.name}`, (p) => (f.position = p))}
            >
              Move on the map
            </button>
            <button
              class="danger"
              onclick={() => {
                editor.draft.facilities = (editor.draft.facilities ?? []).filter((_, j) => j !== i);
                changed();
              }}>Remove</button
            >
          </div>
        </fieldset>
      {:else}
        <p class="dim">No facilities.</p>
      {/each}

      <h3>Zones</h3>
      {#each editor.draft.zones ?? [] as z, i (z.id)}
        <fieldset>
          <legend>{z.name}{z.choke_point ? ` · ${z.choke_point}` : ""}</legend>
          <div class="grid2">
            <label>Name <input bind:value={z.name} oninput={changed} /></label>
            <label>
              Kind
              <select bind:value={z.kind} onchange={changed}>
                <option value="sense">sense (own units, alerts)</option>
                <option value="engage">engage (targets inside)</option>
              </select>
            </label>
            <label>
              Side
              <select bind:value={z.side} onchange={changed}>
                {#each d.sides as s (s.id)}<option value={s.id}>{s.name}</option>{/each}
              </select>
            </label>
            {#if z.area.shape === "circle"}
              {@const a = z.area}
              <label>
                Radius (km)
                <input
                  type="number"
                  min="0.1"
                  value={n(a.radius_m) / 1000}
                  onchange={(e) => {
                    a.radius_m = num(e.currentTarget.value, 10) * 1000;
                    changed();
                  }}
                />
              </label>
            {:else}
              <p class="dim">
                Polygon ({z.area.points.length} points): edit the file to change it.
              </p>
            {/if}
            <label>
              Weapons control
              <select
                value={z.doctrine?.wcs ?? ""}
                onchange={(e) => {
                  const v = e.currentTarget.value;
                  z.doctrine = { ...z.doctrine, wcs: v === "" ? null : (v as Wcs) };
                  changed();
                }}
              >
                <option value="">not set</option>
                <option value="safe">safe</option>
                <option value="tight">tight</option>
                <option value="free">free</option>
              </select>
            </label>
            <label>
              Emissions
              <select
                value={z.doctrine?.emcon ?? ""}
                onchange={(e) => {
                  const v = e.currentTarget.value;
                  z.doctrine = { ...z.doctrine, emcon: v === "" ? null : (v as Emcon) };
                  changed();
                }}
              >
                <option value="">not set</option>
                <option value="active">active</option>
                <option value="silent">silent</option>
              </select>
            </label>
            <label>
              Engage unfriendly
              <select
                value={z.doctrine?.engage_unfriendly == null
                  ? ""
                  : String(z.doctrine.engage_unfriendly)}
                onchange={(e) => {
                  const v = e.currentTarget.value;
                  z.doctrine = {
                    ...z.doctrine,
                    engage_unfriendly: v === "" ? null : v === "true",
                  };
                  changed();
                }}
              >
                <option value="">not set</option>
                <option value="true">yes</option>
                <option value="false">no</option>
              </select>
            </label>
          </div>
          {#if z.kind === "engage"}
            <div class="row" role="group" aria-label={`Shooters for ${z.name}`}>
              <span class="dim">Shooters:</span>
              {#each d.units.filter((u) => u.side === z.side) as u (u.id)}
                <label class="inline">
                  <input
                    type="checkbox"
                    checked={(z.shooters ?? []).includes(u.id)}
                    onchange={(e) => {
                      const rest = (z.shooters ?? []).filter((x) => x !== u.id);
                      z.shooters = e.currentTarget.checked
                        ? [...rest, u.id].sort((a, b) => a - b)
                        : rest;
                      changed();
                    }}
                  />
                  {u.name}
                </label>
              {/each}
            </div>
          {/if}
          <div class="row">
            {#if z.area.shape === "circle"}
              {@const a = z.area}
              <button
                onclick={() => editor.pick(`Click the centre of ${z.name}`, (p) => (a.centre = p))}
              >
                Centre on the map
              </button>
            {/if}
            <button
              class="danger"
              onclick={() => {
                editor.draft.zones = (editor.draft.zones ?? []).filter((_, j) => j !== i);
                changed();
              }}>Remove</button
            >
          </div>
        </fieldset>
      {:else}
        <p class="dim">No zones.</p>
      {/each}

      <h3>Surface action groups</h3>
      {#each editor.draft.action_groups ?? [] as g, i (i)}
        {@const sideUnits = d.units.filter((u) => u.side === g.side)}
        <fieldset>
          <legend>{g.name}</legend>
          <div class="grid2">
            <label>Name <input bind:value={g.name} oninput={changed} /></label>
            <label>
              Leader
              <select bind:value={g.leader} onchange={changed}>
                {#each sideUnits as u (u.id)}<option value={u.id}>{u.name}</option>{/each}
              </select>
            </label>
          </div>
          {#each g.members as m, k (k)}
            <div class="row">
              <select aria-label="Member" bind:value={m.unit} onchange={changed}>
                {#each sideUnits.filter((u) => u.id !== g.leader) as u (u.id)}
                  <option value={u.id}>{u.name}</option>
                {/each}
              </select>
              <label class="inline">
                Bearing
                <input class="short" type="number" bind:value={m.bearing_deg} onchange={changed} />
              </label>
              <label class="inline">
                km
                <input
                  class="short"
                  type="number"
                  min="0.1"
                  value={n(m.range_m) / 1000}
                  onchange={(e) => {
                    m.range_m = num(e.currentTarget.value, 3) * 1000;
                    changed();
                  }}
                />
              </label>
              <button
                aria-label="Remove member"
                onclick={() => {
                  g.members = g.members.filter((_, j) => j !== k);
                  changed();
                }}>×</button
              >
            </div>
          {/each}
          <div class="row">
            <button
              onclick={() => {
                const free = sideUnits.find(
                  (u) => u.id !== g.leader && !g.members.some((m) => m.unit === u.id),
                );
                if (free) {
                  g.members = [...g.members, { unit: free.id, bearing_deg: 90, range_m: 3_000 }];
                  changed();
                }
              }}>Add member</button
            >
            <button
              class="danger"
              onclick={() => {
                editor.draft.action_groups = (editor.draft.action_groups ?? []).filter(
                  (_, j) => j !== i,
                );
                changed();
              }}>Remove group</button
            >
          </div>
        </fieldset>
      {/each}
      <button
        onclick={() => {
          const lead = d.units.find((u) => u.side === planSide);
          if (!lead) {
            editor.error = "That side has no units to lead a group.";
            return;
          }
          const k = (editor.draft.action_groups ?? []).length + 1;
          editor.draft.action_groups = [
            ...(editor.draft.action_groups ?? []),
            { name: `SAG ${k}`, side: planSide, leader: lead.id, members: [] },
          ];
          changed();
        }}>Add action group</button
      >
    {:else}
      <p class="dim">
        Events, injects, seats and groups are edited as JSON (docs/models/events.md). Changes apply
        when you leave a box.
      </p>
      <label class="inline">
        Add an event
        <select
          aria-label="Add an event from a template"
          onchange={(e) => {
            addTemplate(Number(e.currentTarget.value));
            e.currentTarget.value = "";
          }}
        >
          <option value="">choose a template…</option>
          {#each EVENT_TEMPLATES as t, i (t.label)}<option value={i}>{t.label}</option>{/each}
        </select>
      </label>
      {#each JSON_FIELDS as f (f)}
        <label>
          {f}
          <textarea
            class="json"
            rows={f === "events" ? 14 : 6}
            spellcheck="false"
            bind:value={jsonText[f]}
            onblur={() => applyJson(f)}></textarea>
        </label>
        {#if jsonError[f]}<p class="error">{f}: {jsonError[f]}</p>{/if}
      {/each}
    {/if}
  </div>

  <footer aria-label="Check results">
    {#if editor.error}<p class="error" role="alert">{editor.error}</p>{/if}
    <p class:dim={editor.checked}>
      {#if !editor.checked}
        Changed since the last check.
      {:else if editor.issues.length === 0}
        No problems found.
      {:else}
        {editor.errors.length} error{editor.errors.length === 1 ? "" : "s"}, {editor.warnings
          .length}
        warning{editor.warnings.length === 1 ? "" : "s"}.
      {/if}
    </p>
    <ul class="issues">
      {#each editor.issues as i, k (k)}
        <li class={i.severity}>{i.severity === "error" ? "Error" : "Warning"}: {i.message}</li>
      {/each}
    </ul>
  </footer>
</div>

<style>
  .editor {
    position: fixed;
    top: 48px;
    right: 0;
    bottom: 0;
    width: min(480px, 60vw);
    z-index: 25;
    display: flex;
    flex-direction: column;
    background: var(--surface-1);
    border-left: 1px solid var(--border);
  }
  header,
  footer {
    padding: 8px 12px;
    border-bottom: 1px solid var(--border);
  }
  header {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  footer {
    border-top: 1px solid var(--border);
    border-bottom: none;
    max-height: 30%;
    overflow: auto;
  }
  .spacer {
    flex: 1;
  }
  .primary {
    background: var(--accent);
    color: var(--on-accent);
    border-color: var(--accent);
  }
  .danger {
    color: var(--danger);
  }
  .picking {
    margin: 0;
    padding: 6px 12px;
    background: var(--surface-3);
    border-bottom: 1px solid var(--warn);
    color: var(--warn);
  }
  .tabs {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    padding: 6px 12px;
  }
  .tabs .active {
    background: var(--accent);
    color: var(--on-accent);
    border-color: var(--accent);
  }
  .body {
    flex: 1;
    overflow: auto;
    padding: 0 12px 12px;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 6px 0;
    color: var(--text-dim);
  }
  label.inline {
    flex-direction: row;
    align-items: center;
    gap: 8px;
  }
  .grid2 {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0 10px;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    margin: 6px 0;
  }
  fieldset {
    border: 1px solid var(--border);
    border-radius: 4px;
    margin: 8px 0;
  }
  .list {
    list-style: none;
    margin: 6px 0;
    padding: 0;
    max-height: 180px;
    overflow: auto;
  }
  .list button {
    display: flex;
    justify-content: space-between;
    width: 100%;
    text-align: left;
  }
  .list button.selected {
    border-color: var(--accent);
    background: var(--surface-3);
  }
  h3 {
    margin: 12px 0 4px;
    font-size: 13px;
    color: var(--text-dim);
  }
  input.short {
    width: 5em;
  }
  textarea.json {
    font-family: var(--mono);
    font-size: 12px;
  }
  .dim {
    color: var(--text-dim);
  }
  .error,
  li.error {
    color: var(--danger);
  }
  li.warning {
    color: var(--warn);
  }
  .issues {
    margin: 4px 0 0;
    padding-left: 18px;
  }
</style>
