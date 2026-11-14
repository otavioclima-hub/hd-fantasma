<script lang="ts">
  import { t } from "$lib/i18n";
  import type { Item } from "$lib/types";
  import KindIcon from "./KindIcon.svelte";
  import Chevron from "./Chevron.svelte";

  let { items, selectedId, active, onpick }: {
    items: Item[] | undefined; selectedId: number | undefined; active: boolean; onpick: (item: Item) => void;
  } = $props();

  const ROW = 26, OVERSCAN = 8;
  let scroller: HTMLDivElement | undefined = $state();
  let scrollTop = $state(0);
  let viewH = $state(400);
  const list = $derived(items ?? []);
  const start = $derived(Math.max(0, Math.floor(scrollTop / ROW) - OVERSCAN));
  const end = $derived(Math.min(list.length, Math.ceil((scrollTop + viewH) / ROW) + OVERSCAN));
  const visible = $derived(list.slice(start, end));

  // mantém a linha selecionada à vista (teclado, clique em resultado de busca)
  $effect(() => {
    if (selectedId == null || !scroller) return;
    const i = list.findIndex((x) => x.id === selectedId);
    if (i < 0) return;
    const top = i * ROW, bottom = top + ROW;
    if (top < scroller.scrollTop) scroller.scrollTop = top;
    else if (bottom > scroller.scrollTop + viewH) scroller.scrollTop = bottom - viewH;
  });
</script>

<div class="col" role="listbox" tabindex="-1">
  <div class="scroller" bind:this={scroller} bind:clientHeight={viewH} onscroll={(e) => (scrollTop = e.currentTarget.scrollTop)}>
    <div class="body" style="height:{list.length * ROW}px">
      {#each visible as it, k (it.id)}
        {@const i = start + k}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div class="row" class:sel={it.id === selectedId} class:active role="option" tabindex="-1" aria-selected={it.id === selectedId}
             style="top:{i * ROW}px" onclick={() => onpick(it)}>
          <KindIcon kind={it.kind} size={15} />
          <span class="ellipsis" title={it.name}>{it.name}</span>
          {#if it.kind === "folder"}<span class="chev"><Chevron hidden={it.childCount === 0} /></span>{/if}
        </div>
      {/each}
    </div>
    {#if items === undefined}
      <p class="note muted">{$t("loading")}</p>
    {:else if !list.length}
      <p class="note muted">{$t("empty.col")}</p>
    {/if}
  </div>
</div>

<style>
  .col { width: 220px; flex: none; height: 100%; border-right: 1px solid var(--border); background: var(--bg); }
  .scroller { height: 100%; overflow-y: auto; overflow-x: hidden; position: relative; }
  .body { position: relative; }
  .row { position: absolute; left: 0; right: 0; height: var(--row-h); display: flex; align-items: center; gap: 6px; padding: 0 2px 0 8px; white-space: nowrap; }
  .row:hover { background: var(--bg-2); }
  .row.sel { background: var(--sel); }
  .row.sel.active { background: var(--accent); color: var(--accent-fg); }
  .row .ellipsis { flex: 1; min-width: 0; }
  .chev { margin-left: auto; display: inline-flex; opacity: 0.75; }
  .note { padding: 10px 12px; margin: 0; }
</style>
