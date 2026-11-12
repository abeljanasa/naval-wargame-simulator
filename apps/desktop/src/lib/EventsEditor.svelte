<script lang="ts">
  // Forms for scenario events: trigger, conditions and actions (docs/models/events.md). The
  // engine checks the result on every change, as for the rest of the draft; Lua conditions and
  // actions are edited as script text. Events stay plain JSON objects in the draft, so anything
  // the forms do not cover survives untouched.
  import { editor } from "./editor.svelte";
  import { MPS_PER_KNOT, n } from "./format";

  let { onchange }: { onchange: () => void } = $props();

  /** A trigger, condition or action: a tagged JSON object. */
  type Part = { type: string; [key: string]: unknown };
  interface Ev {
    id: string;
    name?: string;
    enabled?: boolean;
    repeatable?: boolean;
    trigger: Part;
    conditions?: Part[];
    actions: Part[];
    [key: string]: unknown;
  }

  const TRIGGERS = [
    { key: "time", label: "at a time" },
    { key: "every", label: "every so often" },
    { key: "area_entered", label: "a unit enters an area" },
    { key: "unit_destroyed", label: "a unit is destroyed" },
    { key: "contact_gained", label: "a side gains a contact" },
  ];
  const CONDITIONS = [
    { key: "event_fired", label: "another event has fired" },
    { key: "event_not_fired", label: "another event has not fired" },
    { key: "unit_alive", label: "a unit is alive" },
    { key: "unit_gone", label: "a unit is gone" },
    { key: "units_alive", label: "a side has so many units" },
    { key: "time_between", label: "the time is between" },
    { key: "probability", label: "a chance" },
    { key: "lua", label: "a Lua condition" },
  ];
  const ACTIONS = [
    { key: "message", label: "send a message" },
    { key: "send_inject", label: "send an inject" },
    { key: "set_posture", label: "change a posture" },
    { key: "set_doctrine", label: "change doctrine" },
    { key: "set_environment", label: "change the weather" },
    { key: "set_speed", label: "set a unit's speed" },
    { key: "set_route", label: "set a unit's route" },
    { key: "assign_mission", label: "give a unit a patrol, or clear its mission" },
    { key: "spawn_reserve", label: "bring in a reserve" },
    { key: "enable_event", label: "turn an event on or off" },
    { key: "lua", label: "run a Lua script" },
  ];
  const POSTURES = ["friendly", "neutral", "unfriendly", "hostile"];
  const CMPS = [
    { key: "lt", label: "fewer than" },
    { key: "le", label: "at most" },
    { key: "eq", label: "exactly" },
    { key: "ge", label: "at least" },
    { key: "gt", label: "more than" },
  ];

  const d = $derived(editor.draft);
  const events = $derived((d.events ?? []) as Ev[]);
  const firstSide = $derived(d.sides[0]?.id ?? 1);
  const otherSide = $derived(d.sides[1]?.id ?? firstSide);
  const firstUnit = $derived(d.units[0]?.id ?? 1);
  const reserves = $derived(d.units.filter((u) => u.reserve));
  const injectIds = $derived(
    ((d.injects ?? []) as { id?: unknown }[]).map((i) => String(i.id ?? "")).filter((x) => x),
  );

  function num(v: string, fallback = 0): number {
    const x = Number(v);
    return Number.isFinite(x) ? x : fallback;
  }
  function get(p: Part, k: string): unknown {
    return p[k];
  }
  function getNum(p: Part, k: string, fallback = 0): number {
    const v = p[k];
    return typeof v === "number" ? v : fallback;
  }
  function getStr(p: Part, k: string): string {
    const v = p[k];
    return typeof v === "string" ? v : "";
  }
  /** An optional number field: blank (null) or a number. */
  function optNum(v: string, scale = 1): number | null {
    const t = v.trim();
    return t === "" ? null : num(t) * scale;
  }

  function changed() {
    onchange();
  }

  function setEvents(next: Ev[]) {
    editor.draft.events = next.length ? next : undefined;
    changed();
  }

  function uniqueId(base: string): string {
    const ids = new Set(events.map((e) => e.id));
    let k = 1;
    while (ids.has(`${base}-${k}`)) k += 1;
    return `${base}-${k}`;
  }

  function newTrigger(kind: string): Part {
    switch (kind) {
      case "every":
        return { type: "every", period_s: 600, start_s: 0 };
      case "area_entered":
        return {
          type: "area_entered",
          area: {
            shape: "circle",
            centre: d.units[0]?.position ?? { lat_deg: 0, lon_deg: 0 },
            radius_m: 10_000,
          },
          side: null,
          unit: null,
        };
      case "unit_destroyed":
        return { type: "unit_destroyed", side: null, unit: null };
      case "contact_gained":
        return { type: "contact_gained", side: firstSide, target_side: null };
      default:
        return { type: "time", at_s: 600 };
    }
  }

  function newCondition(kind: string): Part {
    const other = events[0]?.id ?? "";
    switch (kind) {
      case "event_fired":
      case "event_not_fired":
        return { type: kind, event: other };
      case "unit_alive":
      case "unit_gone":
        return { type: kind, unit: firstUnit };
      case "units_alive":
        return { type: kind, side: firstSide, cmp: "ge", count: 1 };
      case "time_between":
        return { type: kind, from_s: 0, to_s: 3600 };
      case "probability":
        return { type: kind, p: 0.5 };
      default:
        return { type: "lua", script: "return true" };
    }
  }

  function newAction(kind: string): Part {
    switch (kind) {
      case "send_inject":
        return { type: kind, inject: injectIds[0] ?? "" };
      case "set_posture":
        return { type: kind, side: firstSide, toward: otherSide, posture: "hostile" };
      case "set_doctrine":
        return { type: kind, scope: { level: "side", side: firstSide }, doctrine: {} };
      case "set_environment":
        return { type: kind, environment: { sea_state: 4, weather_loss_db: 0 } };
      case "set_speed":
        return { type: kind, unit: firstUnit, speed_mps: 10 * MPS_PER_KNOT };
      case "set_route":
        return { type: kind, unit: firstUnit, waypoints: [] };
      case "assign_mission":
        return { type: kind, unit: firstUnit, mission: null };
      case "spawn_reserve":
        return { type: kind, unit: reserves[0]?.id ?? firstUnit };
      case "enable_event":
        return { type: kind, event: events[0]?.id ?? "", enabled: true };
      case "lua":
        return { type: kind, script: 'sim.message("all", "Lua says hello")' };
      default:
        return { type: "message", to: "all", text: "Message text" };
    }
  }

  function addEvent() {
    setEvents([
      ...events,
      {
        id: uniqueId("event"),
        name: "Exercise control",
        trigger: newTrigger("time"),
        actions: [newAction("message")],
      },
    ]);
  }
</script>

<p class="dim">
  Each event waits for its trigger, checks its conditions, then carries out its actions in order
  (docs/models/events.md). Times are in minutes from the start.
</p>
{#each events as ev, i (i)}
  {@const t = ev.trigger}
  <fieldset>
    <legend>{ev.name || ev.id}</legend>
    <div class="grid2">
      <label>Id <input bind:value={ev.id} onchange={changed} /></label>
      <label>Name <input bind:value={ev.name} onchange={changed} /></label>
      <label class="inline">
        <input
          type="checkbox"
          checked={ev.enabled ?? true}
          onchange={(e) => {
            ev.enabled = e.currentTarget.checked;
            changed();
          }}
        />
        On at the start
      </label>
      <label class="inline">
        <input
          type="checkbox"
          checked={ev.repeatable ?? false}
          onchange={(e) => {
            ev.repeatable = e.currentTarget.checked;
            changed();
          }}
        />
        Can fire more than once
      </label>
    </div>

    <h4>When</h4>
    <label>
      Trigger
      <select
        value={t.type}
        onchange={(e) => {
          ev.trigger = newTrigger(e.currentTarget.value);
          changed();
        }}
      >
        {#each TRIGGERS as k (k.key)}<option value={k.key}>{k.label}</option>{/each}
      </select>
    </label>
    {#if t.type === "time"}
      <label>
        At (min)
        <input
          type="number"
          min="0"
          value={getNum(t, "at_s") / 60}
          onchange={(e) => {
            t.at_s = Math.max(0, num(e.currentTarget.value)) * 60;
            changed();
          }}
        />
      </label>
    {:else if t.type === "every"}
      <div class="grid2">
        <label>
          Every (min)
          <input
            type="number"
            min="0.1"
            step="0.1"
            value={getNum(t, "period_s", 600) / 60}
            onchange={(e) => {
              t.period_s = Math.max(0.1, num(e.currentTarget.value, 10)) * 60;
              changed();
            }}
          />
        </label>
        <label>
          From (min)
          <input
            type="number"
            min="0"
            value={getNum(t, "start_s") / 60}
            onchange={(e) => {
              t.start_s = Math.max(0, num(e.currentTarget.value)) * 60;
              changed();
            }}
          />
        </label>
      </div>
    {:else if t.type === "area_entered"}
      {@const area = get(t, "area") as { shape: string; radius_m?: number; points?: unknown[] }}
      <div class="grid2">
        <label>
          By side
          <select
            value={get(t, "side") ?? ""}
            onchange={(e) => {
              const v = e.currentTarget.value;
              t.side = v === "" ? null : Number(v);
              changed();
            }}
          >
            <option value="">any side</option>
            {#each d.sides as s (s.id)}<option value={s.id}>{s.name}</option>{/each}
          </select>
        </label>
        <label>
          By unit
          <select
            value={get(t, "unit") ?? ""}
            onchange={(e) => {
              const v = e.currentTarget.value;
              t.unit = v === "" ? null : Number(v);
              changed();
            }}
          >
            <option value="">any unit</option>
            {#each d.units as u (u.id)}<option value={u.id}>{u.name}</option>{/each}
          </select>
        </label>
        {#if area.shape === "circle"}
          <label>
            Radius (km)
            <input
              type="number"
              min="0.1"
              value={n(area.radius_m ?? 10_000) / 1000}
              onchange={(e) => {
                area.radius_m = num(e.currentTarget.value, 10) * 1000;
                changed();
              }}
            />
          </label>
        {:else}
          <p class="dim">Polygon ({(area.points ?? []).length} corners)</p>
        {/if}
      </div>
      <div class="row">
        <button
          onclick={() =>
            editor.pick("Click the centre of the area", (p) => {
              t.area = { shape: "circle", centre: p, radius_m: area.radius_m ?? 10_000 };
            })}>Circle centre on the map</button
        >
        <button
          onclick={() => {
            t.area = { shape: "polygon", points: [] };
            editor.pick(
              "Click the corners of the area, then Done",
              (p) => {
                const a = t.area as { shape: string; points?: unknown[] };
                if (a.shape === "polygon") a.points = [...(a.points ?? []), p];
              },
              true,
            );
          }}>Draw a polygon</button
        >
      </div>
    {:else if t.type === "unit_destroyed"}
      <div class="grid2">
        <label>
          Of side
          <select
            value={get(t, "side") ?? ""}
            onchange={(e) => {
              const v = e.currentTarget.value;
              t.side = v === "" ? null : Number(v);
              changed();
            }}
          >
            <option value="">any side</option>
            {#each d.sides as s (s.id)}<option value={s.id}>{s.name}</option>{/each}
          </select>
        </label>
        <label>
          Unit
          <select
            value={get(t, "unit") ?? ""}
            onchange={(e) => {
              const v = e.currentTarget.value;
              t.unit = v === "" ? null : Number(v);
              changed();
            }}
          >
            <option value="">any unit</option>
            {#each d.units as u (u.id)}<option value={u.id}>{u.name}</option>{/each}
          </select>
        </label>
      </div>
    {:else if t.type === "contact_gained"}
      <div class="grid2">
        <label>
          Side
          <select
            value={getNum(t, "side", firstSide)}
            onchange={(e) => {
              t.side = Number(e.currentTarget.value);
              changed();
            }}
          >
            {#each d.sides as s (s.id)}<option value={s.id}>{s.name}</option>{/each}
          </select>
        </label>
        <label>
          On a unit of
          <select
            value={get(t, "target_side") ?? ""}
            onchange={(e) => {
              const v = e.currentTarget.value;
              t.target_side = v === "" ? null : Number(v);
              changed();
            }}
          >
            <option value="">any other side</option>
            {#each d.sides as s (s.id)}<option value={s.id}>{s.name}</option>{/each}
          </select>
        </label>
      </div>
    {/if}

    <h4>Only if</h4>
    {#each ev.conditions ?? [] as c, j (j)}
      <div class="row part">
        <span class="kind">{CONDITIONS.find((k) => k.key === c.type)?.label ?? c.type}</span>
        {#if c.type === "event_fired" || c.type === "event_not_fired"}
          <select bind:value={c.event} onchange={changed} aria-label="Event">
            {#each events.filter((x) => x.id !== ev.id) as x (x.id)}
              <option value={x.id}>{x.name || x.id}</option>
            {/each}
          </select>
        {:else if c.type === "unit_alive" || c.type === "unit_gone"}
          <select
            value={getNum(c, "unit", firstUnit)}
            onchange={(e) => {
              c.unit = Number(e.currentTarget.value);
              changed();
            }}
            aria-label="Unit"
          >
            {#each d.units as u (u.id)}<option value={u.id}>{u.name}</option>{/each}
          </select>
        {:else if c.type === "units_alive"}
          <select
            value={getNum(c, "side", firstSide)}
            onchange={(e) => {
              c.side = Number(e.currentTarget.value);
              changed();
            }}
            aria-label="Side"
          >
            {#each d.sides as s (s.id)}<option value={s.id}>{s.name}</option>{/each}
          </select>
          <select bind:value={c.cmp} onchange={changed} aria-label="Comparison">
            {#each CMPS as k (k.key)}<option value={k.key}>{k.label}</option>{/each}
          </select>
          <input
            class="short"
            type="number"
            min="0"
            value={getNum(c, "count")}
            onchange={(e) => {
              c.count = Math.max(0, Math.round(num(e.currentTarget.value)));
              changed();
            }}
            aria-label="Count"
          />
        {:else if c.type === "time_between"}
          from
          <input
            class="short"
            type="number"
            min="0"
            value={c.from_s == null ? "" : getNum(c, "from_s") / 60}
            onchange={(e) => {
              c.from_s = optNum(e.currentTarget.value, 60);
              changed();
            }}
            aria-label="From (min)"
          />
          to
          <input
            class="short"
            type="number"
            min="0"
            value={c.to_s == null ? "" : getNum(c, "to_s") / 60}
            onchange={(e) => {
              c.to_s = optNum(e.currentTarget.value, 60);
              changed();
            }}
            aria-label="To (min)"
          />
          min
        {:else if c.type === "probability"}
          <input
            class="short"
            type="number"
            min="0"
            max="1"
            step="0.05"
            value={getNum(c, "p", 0.5)}
            onchange={(e) => {
              c.p = Math.min(1, Math.max(0, num(e.currentTarget.value, 0.5)));
              changed();
            }}
            aria-label="Probability"
          />
        {:else if c.type === "lua"}
          <textarea class="json" rows="3" bind:value={c.script} onblur={changed}></textarea>
        {/if}
        <button
          class="danger"
          onclick={() => {
            ev.conditions = (ev.conditions ?? []).filter((_, k) => k !== j);
            changed();
          }}>Remove</button
        >
      </div>
    {/each}
    <select
      aria-label="Add a condition"
      onchange={(e) => {
        const v = e.currentTarget.value;
        if (v) ev.conditions = [...(ev.conditions ?? []), newCondition(v)];
        e.currentTarget.value = "";
        changed();
      }}
    >
      <option value="">add a condition…</option>
      {#each CONDITIONS as k (k.key)}<option value={k.key}>{k.label}</option>{/each}
    </select>

    <h4>Then</h4>
    {#each ev.actions as a, j (j)}
      <div class="row part">
        <span class="kind">{ACTIONS.find((k) => k.key === a.type)?.label ?? a.type}</span>
        {#if a.type === "message"}
          <select
            value={a.to === "side" ? `side:${getNum(a, "side")}` : getStr(a, "to")}
            onchange={(e) => {
              const v = e.currentTarget.value;
              if (v.startsWith("side:")) {
                a.to = "side";
                a.side = Number(v.slice(5));
              } else {
                a.to = v;
                delete a.side;
              }
              changed();
            }}
            aria-label="To"
          >
            <option value="all">everyone</option>
            {#each d.sides as s (s.id)}<option value={`side:${s.id}`}>{s.name}</option>{/each}
            <option value="white_cell">the white cell</option>
          </select>
          <input bind:value={a.text} onchange={changed} aria-label="Text" />
        {:else if a.type === "send_inject"}
          <select bind:value={a.inject} onchange={changed} aria-label="Inject">
            {#each injectIds as id (id)}<option value={id}>{id}</option>{/each}
          </select>
        {:else if a.type === "set_posture"}
          <select
            value={getNum(a, "side", firstSide)}
            onchange={(e) => {
              a.side = Number(e.currentTarget.value);
              changed();
            }}
            aria-label="Side"
          >
            {#each d.sides as s (s.id)}<option value={s.id}>{s.name}</option>{/each}
          </select>
          regards
          <select
            value={getNum(a, "toward", otherSide)}
            onchange={(e) => {
              a.toward = Number(e.currentTarget.value);
              changed();
            }}
            aria-label="Toward"
          >
            {#each d.sides as s (s.id)}<option value={s.id}>{s.name}</option>{/each}
          </select>
          as
          <select bind:value={a.posture} onchange={changed} aria-label="Posture">
            {#each POSTURES as p (p)}<option value={p}>{p}</option>{/each}
          </select>
        {:else if a.type === "set_doctrine"}
          {@const scope = get(a, "scope") as { level: string; side?: number; unit?: number }}
          {@const doc = get(a, "doctrine") as {
            wcs?: string | null;
            emcon?: string | null;
            engage_unfriendly?: boolean | null;
          }}
          <select
            value={scope.level === "unit" ? `unit:${scope.unit}` : `side:${scope.side}`}
            onchange={(e) => {
              const [level, id] = e.currentTarget.value.split(":");
              a.scope =
                level === "unit"
                  ? { level: "unit", unit: Number(id) }
                  : { level: "side", side: Number(id) };
              changed();
            }}
            aria-label="Applies to"
          >
            {#each d.sides as s (s.id)}<option value={`side:${s.id}`}>all of {s.name}</option
              >{/each}
            {#each d.units as u (u.id)}<option value={`unit:${u.id}`}>{u.name}</option>{/each}
          </select>
          <select
            value={doc.wcs ?? ""}
            onchange={(e) => {
              const v = e.currentTarget.value;
              a.doctrine = { ...doc, wcs: v === "" ? null : v };
              changed();
            }}
            aria-label="Weapons control"
          >
            <option value="">WCS unchanged</option>
            <option value="safe">safe</option>
            <option value="tight">tight</option>
            <option value="free">free</option>
          </select>
          <select
            value={doc.emcon ?? ""}
            onchange={(e) => {
              const v = e.currentTarget.value;
              a.doctrine = { ...doc, emcon: v === "" ? null : v };
              changed();
            }}
            aria-label="Emissions"
          >
            <option value="">EMCON unchanged</option>
            <option value="active">active</option>
            <option value="silent">silent</option>
          </select>
        {:else if a.type === "set_environment"}
          {@const env = get(a, "environment") as {
            sea_state?: number;
            weather_loss_db?: number;
          }}
          sea state
          <input
            class="short"
            type="number"
            min="0"
            max="9"
            value={env.sea_state ?? 0}
            onchange={(e) => {
              a.environment = {
                ...env,
                sea_state: Math.min(9, Math.max(0, Math.round(num(e.currentTarget.value)))),
              };
              changed();
            }}
            aria-label="Sea state"
          />
          weather loss (dB)
          <input
            class="short"
            type="number"
            min="0"
            step="0.5"
            value={env.weather_loss_db ?? 0}
            onchange={(e) => {
              a.environment = { ...env, weather_loss_db: Math.max(0, num(e.currentTarget.value)) };
              changed();
            }}
            aria-label="Weather loss"
          />
        {:else if a.type === "set_speed" || a.type === "set_route" || a.type === "assign_mission" || a.type === "spawn_reserve"}
          <select
            value={getNum(a, "unit", firstUnit)}
            onchange={(e) => {
              a.unit = Number(e.currentTarget.value);
              changed();
            }}
            aria-label="Unit"
          >
            {#each a.type === "spawn_reserve" ? reserves : d.units as u (u.id)}
              <option value={u.id}>{u.name}</option>
            {/each}
          </select>
          {#if a.type === "set_speed"}
            <input
              class="short"
              type="number"
              min="0"
              value={(getNum(a, "speed_mps") / MPS_PER_KNOT).toFixed(1)}
              onchange={(e) => {
                a.speed_mps = Math.max(0, num(e.currentTarget.value)) * MPS_PER_KNOT;
                changed();
              }}
              aria-label="Speed (kn)"
            />
            kn
          {:else if a.type === "set_route"}
            <span class="dim">{((get(a, "waypoints") as unknown[]) ?? []).length} waypoints</span>
            <button
              onclick={() => {
                a.waypoints = [];
                editor.pick(
                  "Click the waypoints, then Done",
                  (p) => {
                    a.waypoints = [...((a.waypoints as unknown[]) ?? []), p];
                  },
                  true,
                );
              }}>Draw route</button
            >
          {:else if a.type === "assign_mission"}
            {@const m = get(a, "mission") as { kind?: string; radius_m?: number } | null}
            <span class="dim"
              >{m ? `${m.kind} ${m.radius_m ? `${n(m.radius_m) / 1000} km` : ""}` : "clear"}</span
            >
            <button
              onclick={() =>
                editor.pick("Click the centre of the patrol area", (p) => {
                  a.mission = { kind: "patrol", centre: p, radius_m: 20_000 };
                })}>Patrol (map)</button
            >
            <button
              onclick={() => {
                a.mission = null;
                changed();
              }}>Clear</button
            >
          {/if}
        {:else if a.type === "enable_event"}
          <select bind:value={a.event} onchange={changed} aria-label="Event">
            {#each events as x (x.id)}<option value={x.id}>{x.name || x.id}</option>{/each}
          </select>
          <label class="inline">
            <input
              type="checkbox"
              checked={Boolean(get(a, "enabled"))}
              onchange={(e) => {
                a.enabled = e.currentTarget.checked;
                changed();
              }}
            />
            on
          </label>
        {:else if a.type === "lua"}
          <textarea class="json" rows="3" bind:value={a.script} onblur={changed}></textarea>
        {/if}
        <button
          class="danger"
          onclick={() => {
            ev.actions = ev.actions.filter((_, k) => k !== j);
            changed();
          }}>Remove</button
        >
      </div>
    {/each}
    <div class="row">
      <select
        aria-label="Add an action"
        onchange={(e) => {
          const v = e.currentTarget.value;
          if (v) ev.actions = [...ev.actions, newAction(v)];
          e.currentTarget.value = "";
          changed();
        }}
      >
        <option value="">add an action…</option>
        {#each ACTIONS as k (k.key)}<option value={k.key}>{k.label}</option>{/each}
      </select>
      <button class="danger" onclick={() => setEvents(events.filter((_, k) => k !== i))}
        >Remove event</button
      >
    </div>
  </fieldset>
{:else}
  <p class="dim">No events.</p>
{/each}
<button onclick={addEvent}>Add an event</button>

<style>
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
  .part {
    border-left: 2px solid var(--border);
    padding-left: 6px;
  }
  .kind {
    color: var(--text-dim);
  }
  fieldset {
    border: 1px solid var(--border);
    border-radius: 4px;
    margin: 8px 0;
  }
  h4 {
    margin: 10px 0 2px;
    font-size: 12px;
    color: var(--text-dim);
  }
  input.short {
    width: 5em;
  }
  textarea.json {
    font-family: var(--mono);
    font-size: 12px;
    width: 100%;
  }
  .dim {
    color: var(--text-dim);
  }
</style>
