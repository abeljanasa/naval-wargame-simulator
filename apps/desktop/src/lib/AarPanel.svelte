<script lang="ts">
  // Exercise review: replay with a scrubber, exercise records, and the after-action review tables
  // with CSV export (docs/models/replay-aar.md). For the control station and white-cell seats.
  import type { AarTable, AarView, TimelineKind } from "./bindings";
  import { gameTime, n } from "./format";
  import { sim } from "./sim.svelte";

  let view = $state<AarView | null>(null);
  let tab = $state<AarTable>("sides");
  let kind = $state<TimelineKind | "all">("all");
  /** Scrubber position while dragging (null: follow the clock). */
  let dragging = $state<number | null>(null);

  const replay = $derived(sim.clock?.replay ?? null);
  const tick = $derived(n(sim.clock?.tick ?? 0));
  const timeline = $derived(
    (view?.report.timeline ?? []).filter((e) => kind === "all" || e.kind === kind),
  );

  const TABS: { key: AarTable; label: string }[] = [
    { key: "sides", label: "Summary" },
    { key: "losses", label: "Losses" },
    { key: "weapons", label: "Weapons" },
    { key: "detections", label: "Detections" },
    { key: "transits", label: "Transits" },
    { key: "timeline", label: "Timeline" },
  ];

  const KINDS: TimelineKind[] = [
    "order",
    "refused",
    "contact",
    "launch",
    "hit",
    "miss",
    "intercept",
    "damage",
    "loss",
    "mine",
    "override",
    "facilitation",
    "event",
    "script_error",
  ];

  async function openTables() {
    view = await sim.aar();
  }

  function seekTo(t: number) {
    if (sim.replaying) void sim.seek(t);
  }

  function rate(hits: number, fired: number): string {
    return fired > 0 ? `${Math.round((100 * hits) / fired)} %` : "–";
  }
</script>

<section class="review" aria-label="Exercise review">
  <h2>Exercise review</h2>
  {#if replay}
    <p class="badge" class:bad={!replay.verified}>
      {replay.verified
        ? "Replay: matches the recorded run"
        : "Replay does NOT match the recorded run (altered record or determinism fault)"}
    </p>
    <label for="scrubber" class="dim">
      {gameTime(dragging ?? tick)} of {gameTime(n(replay.end_s))}
    </label>
    <input
      id="scrubber"
      aria-label="Replay position"
      type="range"
      min="0"
      max={n(replay.end_s)}
      step="1"
      value={dragging ?? tick}
      oninput={(e) => (dragging = Number(e.currentTarget.value))}
      onchange={(e) => {
        void sim.seek(Number(e.currentTarget.value));
        dragging = null;
      }}
    />
    <div class="row">
      <button onclick={() => sim.closeReview()}>Back to live</button>
      <button onclick={() => sim.saveRecord()}>Save record</button>
      <button onclick={openTables}>AAR tables</button>
    </div>
  {:else}
    <div class="row">
      <button onclick={() => sim.review()} title="Replay the exercise so far from the start">
        Replay
      </button>
      <button onclick={() => sim.saveRecord()} title="Save the exercise so far as a record">
        Save record
      </button>
      <button onclick={openTables} title="After-action review of the exercise so far">
        AAR tables
      </button>
    </div>
    <label for="record-file" class="dim">Open a record (.nwsrec) for review</label>
    <input
      id="record-file"
      type="file"
      accept=".nwsrec"
      onchange={(e) => {
        const file = e.currentTarget.files?.[0];
        if (file) void sim.openRecord(file);
        e.currentTarget.value = "";
      }}
    />
  {/if}
</section>

{#if view}
  <div class="aar" role="dialog" aria-label="After-action review">
    <header>
      <strong>After-action review</strong>
      <span class="dim">
        {view.replaying ? "replay" : "live run"} to {gameTime(n(view.report.end_tick))}
      </span>
      <span class="spacer"></span>
      <button onclick={openTables} title="Rebuild from the latest state">Refresh</button>
      <button onclick={() => sim.saveCsv(tab)}>Save CSV</button>
      <button onclick={() => (view = null)} aria-label="Close the review">Close</button>
    </header>
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
      {#if tab === "sides"}
        <table>
          <thead>
            <tr>
              <th>Side</th><th>Start</th><th>Reinf.</th><th>Destroyed</th><th>Removed</th>
              <th>Remaining</th><th>Fired</th><th>Hits</th><th>Kills</th>
            </tr>
          </thead>
          <tbody>
            {#each view.report.sides as s (s.side)}
              <tr>
                <td>{s.name}</td><td>{s.units_start}</td><td>{s.reinforcements}</td>
                <td>{s.destroyed}</td><td>{s.removed}</td><td>{s.remaining}</td>
                <td>{s.weapons_fired}</td><td>{s.hits}</td><td>{s.kills}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {:else if tab === "losses"}
        <table>
          <thead>
            <tr><th>Time</th><th>Unit</th><th>Side</th><th>Platform</th><th>Cause</th></tr>
          </thead>
          <tbody>
            {#each view.report.losses as l (`${l.unit}`)}
              <tr class:link={sim.replaying} onclick={() => seekTo(n(l.tick))}>
                <td class="mono">{gameTime(n(l.tick))}</td>
                <td>{l.name}{l.removed ? " (removed)" : ""}</td>
                <td>{l.side_name}</td>
                <td>{l.platform ?? ""}</td>
                <td>{l.cause}</td>
              </tr>
            {:else}
              <tr><td colspan="5" class="dim">No losses.</td></tr>
            {/each}
          </tbody>
        </table>
      {:else if tab === "weapons"}
        <table>
          <thead>
            <tr>
              <th>Side</th><th>Weapon</th><th>Use</th><th>Fired</th><th>Hits</th><th>Missed</th>
              <th>Shot down</th><th>Kills</th><th>Hit rate</th>
            </tr>
          </thead>
          <tbody>
            {#each view.report.weapons as w (`${w.side}-${w.weapon}-${w.defensive}`)}
              <tr>
                <td>{w.side_name}</td><td class="mono">{w.weapon}</td>
                <td>{w.defensive ? "defence" : "strike"}</td>
                <td>{w.fired}</td><td>{w.hits}</td><td>{w.missed}</td>
                <td>{w.intercepted}</td><td>{w.kills}</td><td>{rate(w.hits, w.fired)}</td>
              </tr>
            {:else}
              <tr><td colspan="9" class="dim">No weapons fired.</td></tr>
            {/each}
          </tbody>
        </table>
      {:else if tab === "transits"}
        <table>
          <thead>
            <tr>
              <th>Time</th><th>Choke point</th><th>Unit</th><th>Lane</th><th>Traffic/day</th><th
                >Water</th
              ><th>Draft</th><th>Risk</th>
            </tr>
          </thead>
          <tbody>
            {#each view.report.transits ?? [] as r, i (i)}
              <tr class:link={sim.replaying} onclick={() => seekTo(n(r.tick))}>
                <td class="mono">{gameTime(n(r.tick))}</td><td>{r.choke_point}</td><td
                  >{r.unit_name}</td
                ><td>{r.lane ?? "—"}</td><td class="mono">{Math.round(n(r.traffic_per_day))}</td>
                <td class="mono">{r.depth_m == null ? "—" : `${Math.round(n(r.depth_m))} m`}</td>
                <td class="mono">{r.draft_m == null ? "—" : `${n(r.draft_m).toFixed(1)} m`}</td>
                <td class="risk {r.risk}">{r.risk}</td>
              </tr>
            {:else}
              <tr
                ><td colspan="8" class="dim"
                  >No assessed transits. Choke points are assessed when they have shipping lanes or
                  terrain packs are installed.</td
                ></tr
              >
            {/each}
          </tbody>
        </table>
      {:else if tab === "detections"}
        <table>
          <thead>
            <tr>
              <th>Side</th><th>Contact</th><th>Target</th><th>Gained</th><th>Lost</th><th>Held</th>
            </tr>
          </thead>
          <tbody>
            {#each view.report.detections as d, i (i)}
              <tr class:link={sim.replaying} onclick={() => seekTo(n(d.gained))}>
                <td>{d.side_name}</td><td class="mono">C{d.contact}</td><td>{d.target_name}</td>
                <td class="mono">{gameTime(n(d.gained))}</td>
                <td class="mono">{d.lost === null ? "held" : gameTime(n(d.lost))}</td>
                <td class="mono">{gameTime(n(d.held_s))}</td>
              </tr>
            {:else}
              <tr><td colspan="6" class="dim">No contacts.</td></tr>
            {/each}
          </tbody>
        </table>
      {:else}
        <label class="filter">
          Show
          <select bind:value={kind}>
            <option value="all">everything</option>
            {#each KINDS as k (k)}
              <option value={k}>{k.replace("_", " ")}</option>
            {/each}
          </select>
          <span class="dim">
            {timeline.length} entries{sim.replaying ? " · click one to jump there" : ""}
          </span>
        </label>
        <table>
          <tbody>
            {#each timeline as e, i (i)}
              <tr class:link={sim.replaying} onclick={() => seekTo(n(e.tick))}>
                <td class="mono">{gameTime(n(e.tick))}</td>
                <td class="kind">{e.kind.replace("_", " ")}</td>
                <td>{e.text}</td>
              </tr>
            {:else}
              <tr><td colspan="3" class="dim">Nothing yet.</td></tr>
            {/each}
          </tbody>
        </table>
      {/if}
    </div>
  </div>
{/if}

<style>
  .review {
    border-top: 1px solid var(--border);
    padding-top: 8px;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin: 6px 0;
  }
  #scrubber {
    /* Fill the panel without the global input padding pushing it past the edge. */
    width: 100%;
    box-sizing: border-box;
    padding: 0;
    margin: 4px 0;
  }
  .badge {
    margin: 4px 0;
    padding: 4px 6px;
    border: 1px solid var(--accent);
    border-radius: 4px;
  }
  .badge.bad {
    border-color: var(--danger);
    color: var(--danger);
  }
  .dim {
    color: var(--text-dim);
  }
  .aar {
    position: fixed;
    left: 320px;
    top: 48px;
    right: 0;
    bottom: 0;
    z-index: 20;
    display: flex;
    flex-direction: column;
    background: var(--surface-1);
    border-left: 1px solid var(--border);
  }
  .aar header {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--border);
  }
  .spacer {
    flex: 1;
  }
  .tabs {
    display: flex;
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
  table {
    border-collapse: collapse;
    width: 100%;
  }
  th,
  td {
    text-align: left;
    padding: 3px 8px;
    border-bottom: 1px solid var(--border);
    vertical-align: top;
  }
  th {
    position: sticky;
    top: 0;
    background: var(--surface-2);
    color: var(--text-dim);
    font-weight: 600;
  }
  .mono {
    font-family: var(--mono);
    white-space: nowrap;
  }
  .kind {
    color: var(--text-dim);
    white-space: nowrap;
  }
  tr.link {
    cursor: pointer;
  }
  tr.link:hover {
    background: var(--surface-3);
  }
  .filter {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 4px 0 8px;
  }
  .risk.high {
    color: var(--danger, #e05a4f);
    font-weight: 600;
  }
  .risk.moderate {
    color: var(--warn);
  }
</style>
