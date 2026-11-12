<script lang="ts">
  // Platform database browser: search, record detail with provenance, validation feedback, and
  // an editor for a record's JSON source (DbRecordEditor; ADR-0004).
  import {
    commands,
    type DbIssue,
    type DbPlatformDetail,
    type DbPlatformRow,
    type DbSourceRow,
  } from "./bindings";
  import DbRecordEditor from "./DbRecordEditor.svelte";
  import { knots } from "./format";

  let query = $state("");
  let rows = $state<DbPlatformRow[]>([]);
  let detail = $state<DbPlatformDetail | null>(null);
  let issues = $state<DbIssue[]>([]);
  let error = $state<string | null>(null);
  let editing = $state<string | null>(null);
  // Platforms have a detail view; sensors and weapons open straight in the record editor.
  type Kind = "platform" | "sensor" | "weapon";
  let kind = $state<Kind>("platform");
  let sourceRows = $state<DbSourceRow[]>([]);
  const KINDS: { key: Kind; label: string }[] = [
    { key: "platform", label: "Platforms" },
    { key: "sensor", label: "Sensors" },
    { key: "weapon", label: "Weapons" },
  ];
  const shownSources = $derived(
    sourceRows.filter((r) => {
      const q = query.trim().toLowerCase();
      return !q || r.id.toLowerCase().includes(q) || r.name.toLowerCase().includes(q);
    }),
  );

  $effect(() => {
    const k = kind;
    editing = null;
    if (k === "platform") return;
    void commands.dbSourceRecords(k).then((res) => {
      if (res.status === "error") error = res.error;
      else sourceRows = res.data;
    });
  });

  const recordIssues = $derived(
    detail ? issues.filter((i) => i.location.endsWith(`/${detail?.row.id}`)) : [],
  );

  async function search(q: string) {
    const res = await commands.dbSearch(q);
    if (res.status === "error") error = res.error;
    else {
      rows = res.data;
      error = null;
    }
  }

  async function open(id: string) {
    const res = await commands.dbPlatform(id);
    if (res.status === "error") error = res.error;
    else {
      detail = res.data;
      editing = null;
    }
  }

  $effect(() => {
    void search(query);
  });

  $effect(() => {
    void commands.dbIssues().then((res) => {
      if (res.status === "ok") issues = res.data;
    });
  });
</script>

<section class="db">
  <h2><label for="db-search">Platform database</label></h2>
  <p class="dim">
    {issues.length === 0
      ? "Validation: all rules pass."
      : `Validation: ${issues.length} warning${issues.length === 1 ? "" : "s"}.`}
  </p>
  <div class="kinds" role="radiogroup" aria-label="Record kind">
    {#each KINDS as k (k.key)}
      <button
        type="button"
        role="radio"
        aria-checked={kind === k.key}
        class:active={kind === k.key}
        onclick={() => (kind = k.key)}>{k.label}</button
      >
    {/each}
  </div>
  <input
    id="db-search"
    type="search"
    placeholder={kind === "platform" ? "Search id, name or category" : "Search id or name"}
    bind:value={query}
  />
  {#if error}
    <p class="error" role="alert">{error}</p>
  {/if}
  {#if kind === "platform"}
    <ul class="results" aria-label="Platforms">
      {#each rows as r (r.id)}
        <li>
          <button class:selected={detail?.row.id === r.id} onclick={() => open(r.id)}>
            <span class="name">{r.name}</span>
            <span class="meta">{r.id} · {r.category}</span>
          </button>
        </li>
      {:else}
        <li class="empty">No platforms match.</li>
      {/each}
    </ul>

    {#if detail}
      {@const d = detail}
      <h3>{d.row.name}</h3>
      <dl>
        <dt>Id</dt>
        <dd>{d.row.id}</dd>
        <dt>Category</dt>
        <dd>{d.row.category} ({d.row.domain})</dd>
        <dt>Speed</dt>
        <dd>max {knots(d.row.max_speed_mps)}, cruise {knots(d.cruise_speed_mps)}</dd>
        <dt>Length</dt>
        <dd>{d.length_m === null ? "—" : `${d.length_m} m`}</dd>
        <dt>RCS</dt>
        <dd>{d.rcs_m2 === null ? "—" : `${d.rcs_m2} m²`}</dd>
        <dt>Sensors</dt>
        <dd>
          {#each d.sensors as s (s.sensor_id)}
            <div>
              {s.sensor_id} · {s.kind} · {s.max_range_m === null
                ? "—"
                : `${(s.max_range_m / 1000).toFixed(0)} km`}
            </div>
          {:else}
            none
          {/each}
        </dd>
        <dt>Source</dt>
        <dd>{d.source_citation}</dd>
        <dt>Confidence</dt>
        <dd>{d.confidence}</dd>
        <dt>Marking</dt>
        <dd>{d.classification}</dd>
      </dl>
      {#each recordIssues as i (i.rule + i.location)}
        <p class="warning">{i.rule}: {i.message}</p>
      {/each}
      {#if editing === d.row.id}
        <DbRecordEditor id={d.row.id} onclose={() => (editing = null)} />
      {:else}
        <button type="button" class="edit" onclick={() => (editing = d.row.id)}>Edit record</button>
      {/if}
    {/if}
  {:else}
    <ul class="results" aria-label={kind === "sensor" ? "Sensors" : "Weapons"}>
      {#each shownSources as r (r.id)}
        <li>
          <button class:selected={editing === r.id} onclick={() => (editing = r.id)}>
            <span class="name">{r.name}</span>
            <span class="meta">{r.id}</span>
          </button>
        </li>
      {:else}
        <li class="empty">No records match.</li>
      {/each}
    </ul>
    {#if editing}
      {#key editing}
        <DbRecordEditor {kind} id={editing} onclose={() => (editing = null)} />
      {/key}
    {/if}
  {/if}
</section>

<style>
  h2 {
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-dim);
    margin: 8px 0;
  }
  h3 {
    margin: 12px 0 4px;
    font-size: 14px;
  }
  .dim,
  .meta {
    color: var(--text-dim);
    font-family: var(--mono);
    font-size: 12px;
  }
  .dim {
    margin: 0 0 8px;
  }
  input[type="search"] {
    width: 100%;
    box-sizing: border-box;
  }
  .results {
    list-style: none;
    margin: 8px 0 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
    max-height: 14rem;
    overflow-y: auto;
  }
  .results button {
    width: 100%;
    text-align: left;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .results button.selected {
    border-color: var(--accent);
    background: var(--surface-3);
  }
  .empty {
    color: var(--text-dim);
  }
  dl {
    display: grid;
    grid-template-columns: 76px 1fr;
    gap: 4px 8px;
    margin: 0;
    font-size: 13px;
  }
  dt {
    color: var(--text-dim);
  }
  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
  .error {
    color: var(--danger, #e5484d);
  }
  .warning {
    color: var(--warn);
    font-size: 12px;
  }
  .edit {
    margin-top: 8px;
  }
  .kinds {
    display: flex;
    gap: 4px;
    margin: 0 0 8px;
  }
  .kinds button.active {
    border-color: var(--accent);
    background: var(--surface-3);
  }
</style>
