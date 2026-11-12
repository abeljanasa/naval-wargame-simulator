<script lang="ts" module>
  /** One entry in a menu. */
  export interface MenuItem {
    label: string;
    /** What happens when it is chosen. */
    onselect?: () => void;
    /** A checkable item: shown ticked when true. */
    checked?: boolean;
    disabled?: boolean;
    /** Key hint shown on the right. */
    shortcut?: string;
    /** A divider line before this item. */
    separatorBefore?: boolean;
  }

  export interface Menu {
    label: string;
    items: MenuItem[];
  }
</script>

<script lang="ts">
  // An application menu bar (WAI-ARIA menubar pattern): click or Enter opens a menu; arrows
  // move between menus and items; Escape or a click elsewhere closes it.
  let { menus }: { menus: Menu[] } = $props();

  let open = $state<number | null>(null);
  let bar: HTMLElement;

  function itemsOf(i: number): HTMLElement[] {
    return [...bar.querySelectorAll<HTMLElement>(`[data-menu="${i}"] [role^="menuitem"]`)].filter(
      (el) => el.getAttribute("aria-disabled") !== "true",
    );
  }

  function topButtons(): HTMLElement[] {
    return [...bar.querySelectorAll<HTMLElement>(":scope > li > button")];
  }

  function openMenu(i: number, focusFirst = false) {
    open = i;
    if (focusFirst) queueMicrotask(() => itemsOf(i)[0]?.focus());
  }

  function close(refocus = true) {
    const was = open;
    open = null;
    if (refocus && was !== null) topButtons()[was]?.focus();
  }

  function choose(item: MenuItem) {
    if (item.disabled) return;
    close();
    item.onselect?.();
  }

  function onTopKey(e: KeyboardEvent, i: number) {
    const n = menus.length;
    if (e.key === "ArrowRight" || e.key === "ArrowLeft") {
      e.preventDefault();
      const next = (i + (e.key === "ArrowRight" ? 1 : n - 1)) % n;
      topButtons()[next]?.focus();
      if (open !== null) openMenu(next);
    } else if (e.key === "ArrowDown" || e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      openMenu(i, true);
    } else if (e.key === "Escape") {
      close();
    }
  }

  function onItemKey(e: KeyboardEvent, i: number) {
    const items = itemsOf(i);
    const k = items.indexOf(e.currentTarget as HTMLElement);
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const step = e.key === "ArrowDown" ? 1 : items.length - 1;
      items[(k + step) % items.length]?.focus();
    } else if (e.key === "ArrowRight" || e.key === "ArrowLeft") {
      e.preventDefault();
      const n = menus.length;
      const next = (i + (e.key === "ArrowRight" ? 1 : n - 1)) % n;
      topButtons()[next]?.focus();
      openMenu(next, true);
    } else if (e.key === "Escape" || e.key === "Tab") {
      close(e.key === "Escape");
    }
  }
</script>

<svelte:window
  onpointerdown={(e) => {
    if (open !== null && !bar.contains(e.target as Node)) close(false);
  }}
/>

<ul class="menubar" role="menubar" aria-label="Application menu" bind:this={bar}>
  {#each menus as m, i (m.label)}
    <li>
      <button
        type="button"
        role="menuitem"
        aria-haspopup="menu"
        aria-expanded={open === i}
        class:open={open === i}
        onclick={() => (open === i ? close() : openMenu(i))}
        onpointerenter={() => {
          if (open !== null && open !== i) openMenu(i);
        }}
        onkeydown={(e) => onTopKey(e, i)}>{m.label}</button
      >
      {#if open === i}
        <ul class="menu" role="menu" aria-label={m.label} data-menu={i}>
          {#each m.items as item (item.label)}
            {#if item.separatorBefore}<li role="separator" class="sep"></li>{/if}
            <li role="none">
              <button
                type="button"
                role={item.checked === undefined ? "menuitem" : "menuitemcheckbox"}
                aria-checked={item.checked === undefined ? undefined : item.checked}
                aria-disabled={item.disabled ? "true" : undefined}
                class:disabled={item.disabled}
                onclick={() => choose(item)}
                onkeydown={(e) => onItemKey(e, i)}
              >
                <span class="tick" aria-hidden="true">{item.checked ? "✓" : ""}</span>
                <span class="label">{item.label}</span>
                {#if item.shortcut}<kbd>{item.shortcut}</kbd>{/if}
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </li>
  {/each}
</ul>

<style>
  .menubar {
    display: flex;
    gap: 2px;
    list-style: none;
    margin: 0;
    padding: 0;
    height: 100%;
    align-items: stretch;
  }
  .menubar > li {
    position: relative;
    display: flex;
  }
  .menubar > li > button {
    border: none;
    border-radius: 0;
    background: transparent;
    padding: 0 10px;
  }
  .menubar > li > button:hover,
  .menubar > li > button.open {
    background: var(--surface-3);
  }
  .menu {
    position: absolute;
    top: 100%;
    left: 0;
    z-index: 50;
    min-width: 240px;
    list-style: none;
    margin: 0;
    padding: 4px 0;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: 0 0 6px 6px;
    box-shadow: 0 8px 24px rgb(0 0 0 / 35%);
  }
  .menu button {
    display: grid;
    grid-template-columns: 18px 1fr auto;
    align-items: center;
    width: 100%;
    gap: 4px;
    text-align: left;
    border: none;
    border-radius: 0;
    background: transparent;
    padding: 5px 12px 5px 6px;
  }
  .menu button:hover:not(.disabled),
  .menu button:focus-visible {
    background: var(--accent);
    color: var(--on-accent);
    outline: none;
  }
  .menu button.disabled {
    opacity: 0.45;
    cursor: default;
  }
  .tick {
    text-align: center;
  }
  kbd {
    font-family: var(--mono);
    font-size: 11px;
    opacity: 0.75;
    padding-left: 16px;
  }
  .sep {
    height: 1px;
    margin: 4px 0;
    background: var(--border);
  }
</style>
