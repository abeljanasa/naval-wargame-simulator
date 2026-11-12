<script lang="ts">
  // Edits one platform record in the database's JSON source (ADR-0004): provenance fields, the
  // record's JSON, a live check against the whole database, a diff against the original, and a
  // save that hands the edited source file over for review. The running app keeps using its
  // built-in database until it is rebuilt with the file.
  import { commands, type DbCheck } from "./bindings";
  import { lineDiff, readProvenance, setProvenance } from "./db-edit";
  import { download } from "./files";

  let { id, onclose }: { id: string; onclose: () => void } = $props();

  let file = $state("");
  let original = $state("");
  let text = $state("");
  let check = $state<DbCheck | null>(null);
  let checking = $state(false);
  let error = $state<string | null>(null);
  let saved = $state<string | null>(null);

  const provenance = $derived(readProvenance(text));
  const changed = $derived(text !== original);
  const diff = $derived(changed ? lineDiff(original, text).filter((l) => l.kind !== "same") : []);
  const errors = $derived(check?.issues.filter((i) => i.severity === "error") ?? []);
  const warnings = $derived(check?.issues.filter((i) => i.severity === "warning") ?? []);
  const canSave = $derived(
    changed && !checking && check !== null && check.error === null && errors.length === 0,
  );

  $effect(() => {
    void commands.dbRecordSource(id).then((res) => {
      if (res.status === "error") error = res.error;
      else {
        file = res.data.file;
        original = res.data.text;
        text = res.data.text;
      }
    });
  });

  // Check the edit against the whole database, 300 ms after typing stops.
  $effect(() => {
    const t = text;
    if (!original) return;
    checking = true;
    const timer = setTimeout(() => {
      void commands.dbCheckRecord(id, t).then((c) => {
        if (t === text) {
          check = c;
          checking = false;
        }
      });
    }, 300);
    return () => clearTimeout(timer);
  });

  function setField(field: string, value: string) {
    const next = setProvenance(text, field, value);
    if (next === null) error = "Fix the JSON first: the record does not parse.";
    else {
      text = next;
      error = null;
    }
  }

  async function save() {
    const res = await commands.dbApplyRecord(id, text);
    if (res.status === "error") {
      error = res.error;
      return;
    }
    download(res.data.name.split("/").pop() ?? res.data.name, res.data.text, "application/json");
    saved = res.data.name;
    original = text;
    error = null;
  }
</script>

<section class="editor" aria-label="Edit platform record">
  <h3>Edit {id} <span class="dim">· {file}</span></h3>

  <fieldset>
    <legend>Provenance (this record)</legend>
    <label for="prov-citation">Source citation</label>
    <input
      id="prov-citation"
      type="text"
      placeholder="File default"
      value={provenance?.source_citation ?? ""}
      onchange={(e) => setField("source_citation", e.currentTarget.value)}
    />
    <label for="prov-confidence">Confidence</label>
    <select
      id="prov-confidence"
      value={provenance?.confidence ?? ""}
      onchange={(e) => setField("confidence", e.currentTarget.value)}
    >
      <option value="">File default</option>
      <option value="A">A: official or manufacturer figure</option>
      <option value="B">B: several independent open sources agree</option>
      <option value="C">C: a single credible open source</option>
      <option value="D">D: generic class estimate</option>
      <option value="E">E: placeholder; replace before analysis</option>
    </select>
    <label for="prov-updated">Updated</label>
    <input
      id="prov-updated"
      type="date"
      value={provenance?.updated_at ?? ""}
      onchange={(e) => setField("updated_at", e.currentTarget.value)}
    />
    <p class="dim">Marking: UNCLASSIFIED (the open build accepts nothing else).</p>
  </fieldset>

  <label for="record-json">Record (JSON, as in the source file)</label>
  <textarea id="record-json" spellcheck="false" rows="14" bind:value={text}></textarea>

  <div class="check" role="status" aria-live="polite">
    {#if checking}
      <p class="dim">Checking against the whole database…</p>
    {:else if check?.error}
      <p class="bad">Does not load: {check.error}</p>
    {:else if check}
      {#if errors.length === 0}
        <p class="good">
          Checks pass{warnings.length
            ? ` (${warnings.length} warning${warnings.length === 1 ? "" : "s"})`
            : ""}.
        </p>
      {/if}
      {#each errors as i (i.rule + i.location)}
        <p class="bad">{i.location}: {i.message} <span class="dim">({i.rule})</span></p>
      {/each}
      {#each warnings as i (i.rule + i.location)}
        <p class="warn">{i.location}: {i.message} <span class="dim">({i.rule})</span></p>
      {/each}
    {/if}
  </div>

  {#if diff.length}
    <h4>Changes</h4>
    <div class="diff" aria-label="Changes from the original">
      {#each diff as l, i (i)}
        <div class={l.kind}>{l.kind === "added" ? "+ " : "- "}{l.text.trim()}</div>
      {/each}
    </div>
  {/if}

  {#if error}
    <p class="bad" role="alert">{error}</p>
  {/if}
  {#if saved}
    <p class="good">
      Saved {saved}. Put it in <code>data/db/open/{saved}</code> for review. This app keeps using its
      built-in database until it is rebuilt with the file.
    </p>
  {/if}

  <div class="row">
    <button type="button" disabled={!canSave} onclick={save}>Save to source file</button>
    <button type="button" disabled={!changed} onclick={() => (text = original)}
      >Discard changes</button
    >
    <button type="button" onclick={onclose}>Close editor</button>
  </div>
</section>

<style>
  .editor {
    border-top: 1px solid var(--border);
    margin-top: 12px;
    padding-top: 8px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  h3 {
    margin: 0;
    font-size: 14px;
  }
  h4 {
    margin: 4px 0 0;
    font-size: 12px;
  }
  fieldset {
    border: 1px solid var(--border);
    border-radius: 4px;
    display: grid;
    grid-template-columns: 110px 1fr;
    gap: 4px 8px;
    align-items: center;
  }
  fieldset p {
    grid-column: 1 / -1;
    margin: 2px 0 0;
  }
  textarea {
    width: 100%;
    box-sizing: border-box;
    font-family: var(--mono);
    font-size: 12px;
    resize: vertical;
  }
  .dim {
    color: var(--text-dim);
    font-size: 12px;
  }
  .check p {
    margin: 2px 0;
    font-size: 12px;
  }
  .good {
    color: var(--accent);
  }
  .bad {
    color: var(--danger, #e5484d);
  }
  .warn {
    color: var(--warn);
  }
  .diff {
    margin: 0;
    max-height: 10rem;
    overflow: auto;
    font-family: var(--mono);
    font-size: 12px;
    white-space: pre-wrap;
  }
  .diff .added {
    color: var(--accent);
  }
  .diff .removed {
    color: var(--danger, #e5484d);
  }
  .row {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }
</style>
