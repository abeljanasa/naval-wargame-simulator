<script lang="ts">
  // Air operations (Phase 4 slices 9–11, docs/models/air-operations.md): the seat's airbases and
  // the aircraft parked there, their readiness and loadouts, and the orders to launch, cancel and
  // set readiness. Airborne aircraft are recalled from the unit inspector.
  import { commands, type GeoPoint, type ParkedView, type Readiness } from "./bindings";
  import { n } from "./format";
  import { sim } from "./sim.svelte";
  import {
    READINESS,
    SORTIE_KINDS,
    needsPoint,
    parkedText,
    sortieMission,
    type SortieKind,
  } from "./air";

  /** The launch form of each aircraft, by unit. */
  let kind = $state<Record<number, SortieKind>>({});
  let loadout = $state<Record<number, string>>({});
  let target = $state<Record<number, number | null>>({});
  let radiusKm = $state(40);

  const failed = (r: { status: string; error?: string }) => {
    if (r.status === "error") sim.error = r.error ?? "refused";
  };

  /** The contacts a base's side holds (a side seat sees only its own). */
  function contactsOf(side: number) {
    return sim.allContacts.filter((c) => c.side === side);
  }

  function launch(a: ParkedView) {
    const k = kind[a.unit] ?? "patrol";
    const lo = loadout[a.unit] || null;
    const send = (at: GeoPoint | null) => {
      const m = sortieMission(k, at, target[a.unit] ?? null, Math.max(1, radiusKm) * 1000);
      if (!m) {
        sim.error = k === "strike" ? "Choose the contact to strike" : "Pick a point on the map";
        return;
      }
      void commands.orderSortie(a.unit, m, lo).then(failed);
    };
    if (needsPoint(k)) {
      const what = k === "ferry" ? "destination" : k === "support" ? "station" : "patrol centre";
      sim.pick(`${a.name}: click the ${what}`, (p) => send(p));
    } else {
      send(null);
    }
  }
</script>

<section class="air" aria-label="Air operations">
  {#each sim.airBases as b (b.facility)}
    <article class="base">
      <h3>{b.name}</h3>
      <p class="dim">
        {b.deck ? "Deck" : "Airfield"}: {b.runways}
        {b.deck ? "spot" : "runway"}{b.runways === 1 ? "" : "s"}{b.parking > 0
          ? `, room for ${b.parking}`
          : ""}; aviation fuel {b.aviation_fuel_t == null
          ? "unlimited"
          : `${n(b.aviation_fuel_t).toFixed(1)} t`}
      </p>
      <ul aria-label={`Aircraft at ${b.name}`}>
        {#each b.aircraft as a (a.unit)}
          <li>
            <div class="line">
              <strong>{a.name}</strong>
              <span class="dim">{a.platform ?? ""}</span>
              <span class="state" class:busy={n(a.ready_in_s) > 0 || a.damaged}
                >{parkedText(a)}</span
              >
              {#if a.fuel_fraction != null}
                <span class="dim">fuel {Math.round(100 * n(a.fuel_fraction))} %</span>
              {/if}
            </div>
            {#if a.launching}
              <div class="line">
                <span class="dim">{a.loadout ?? "no loadout"}</span>
                <button
                  type="button"
                  onclick={() => void commands.orderRecall(a.unit, null).then(failed)}
                  >Cancel launch</button
                >
              </div>
            {:else}
              <div class="line">
                <label
                  >Readiness
                  <select
                    value={a.readiness}
                    aria-label={`Readiness of ${a.name}`}
                    onchange={(e) =>
                      void commands
                        .orderReadiness(a.unit, e.currentTarget.value as Readiness, null)
                        .then(failed)}
                  >
                    {#each READINESS as r (r.key)}
                      <option value={r.key}>{r.label}</option>
                    {/each}
                  </select></label
                >
              </div>
              <div class="line">
                <select
                  aria-label={`Sortie for ${a.name}`}
                  value={kind[a.unit] ?? "patrol"}
                  onchange={(e) => (kind[a.unit] = e.currentTarget.value as SortieKind)}
                >
                  {#each SORTIE_KINDS as k (k.key)}
                    <option value={k.key}>{k.label}</option>
                  {/each}
                </select>
                {#if a.loadouts.length > 0}
                  <select
                    aria-label={`Loadout for ${a.name}`}
                    value={loadout[a.unit] ?? ""}
                    onchange={(e) => (loadout[a.unit] = e.currentTarget.value)}
                  >
                    <option value="">Loadout for the mission</option>
                    {#each a.loadouts as o (o.id)}
                      <option value={o.id}>{o.name}{o.id === a.loadout ? " (carried)" : ""}</option>
                    {/each}
                  </select>
                {/if}
                {#if (kind[a.unit] ?? "patrol") === "strike"}
                  <select
                    aria-label={`Contact for ${a.name} to strike`}
                    value={target[a.unit] ?? ""}
                    onchange={(e) =>
                      (target[a.unit] =
                        e.currentTarget.value === "" ? null : Number(e.currentTarget.value))}
                  >
                    <option value="">Contact…</option>
                    {#each contactsOf(b.side) as c (c.number)}
                      <option value={c.number}>C{c.number}{c.domain ? ` (${c.domain})` : ""}</option
                      >
                    {/each}
                  </select>
                {/if}
                <button
                  type="button"
                  disabled={a.damaged || a.readiness === "maintenance"}
                  onclick={() => launch(a)}>Launch</button
                >
              </div>
            {/if}
          </li>
        {:else}
          <li class="dim">No aircraft on the ground.</li>
        {/each}
      </ul>
    </article>
  {/each}
  <p class="dim">
    <label
      >Patrol and station radius <input
        type="number"
        min="1"
        bind:value={radiusKm}
        style="width: 4.5em"
      /> km</label
    >
  </p>
  <p class="dim">
    Launches wait for readiness (alert 15 or 60 minutes), any change of loadout, and a free runway.
    Aircraft come home when the mission is done, their weapons are spent, their target is gone or
    their fuel reaches bingo; recall one from the unit inspector.
  </p>
</section>

<style>
  .air h3 {
    margin: 8px 0 2px;
  }
  .base {
    border-top: 1px solid var(--border);
    padding: 4px 0 8px;
  }
  ul {
    list-style: none;
    margin: 4px 0;
    padding: 0;
  }
  li {
    padding: 6px 0;
    border-bottom: 1px solid var(--border);
  }
  .line {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    margin: 2px 0;
  }
  .state {
    color: var(--accent);
  }
  .state.busy {
    color: var(--text-dim);
  }
  .dim {
    color: var(--text-dim);
  }
</style>
