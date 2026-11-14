<script lang="ts">
  import { S, selDisk, openFolder, toggleTree } from "$lib/store.svelte";
  import { t } from "$lib/i18n";
  import type { Item } from "$lib/types";
  import KindIcon from "./KindIcon.svelte";
  import DiskIcon from "./DiskIcon.svelte";

  interface Node { item: Item; depth: number }
  const nodes = $derived.by(() => {
    const out: Node[] = [];
    const walk = (list: Item[] | null | undefined, depth: number) => {
      for (const it of list ?? []) {
        if (it.kind !== "folder") continue;
        out.push({ item: it, depth });
        if (S.expanded[it.id]) walk(S.treeChildren[it.id], depth + 1);
      }
    };
    walk(S.rootItems, 0);
    return out;
  });
  const disk = $derived(selDisk());
</script>

<div class="panel">
  <div class="head">{$t("folders")}</div>
  <div class="tree" role="tree">
    {#if !disk}
      <p class="muted pad">{$t("pickDisk")}</p>
    {:else}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <div class="row" class:sel={S.folderId === null} style="padding-left:6px" role="treeitem" aria-selected={S.folderId === null} tabindex="-1"
           onclick={() => openFolder(null)}>
        <span class="tw"></span>
        <DiskIcon fs={disk.filesystem} size={16} />
        <span class="ellipsis">{disk.name}</span>
      </div>
      {#if !S.rootItems}<p class="muted pad">{$t("loading")}</p>{/if}
      {#each nodes as n (n.item.id)}
        {@const it = n.item}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div class="row" class:sel={S.folderId === it.id} style="padding-left:{6 + (n.depth + 1) * 14}px" role="treeitem"
             aria-selected={S.folderId === it.id} aria-expanded={S.expanded[it.id] ?? false} tabindex="-1"
             onclick={() => openFolder(it)} ondblclick={() => toggleTree(it)}>
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
          <span class="tw" class:open={S.expanded[it.id]} class:hidden={it.childCount === 0} title={$t("expand")}
                onclick={(e) => { e.stopPropagation(); toggleTree(it); }}>▸</span>
          <KindIcon kind="folder" size={15} />
          <span class="ellipsis">{it.name}</span>
          {#if S.treeLoading[it.id]}<span class="muted">…</span>{/if}
        </div>
      {/each}
    {/if}
  </div>
</div>

<style>
  .panel { display: flex; flex-direction: column; height: 100%; min-width: 0; background: var(--bg); }
  .head { padding: 8px 12px 5px; font-size: 11px; text-transform: uppercase; letter-spacing: 0.05em; color: var(--fg-2); font-weight: 600; }
  .tree { flex: 1; overflow: auto; padding-bottom: 8px; }
  .pad { padding: 4px 12px; }
  .row { display: flex; align-items: center; gap: 5px; height: var(--row-h); padding-right: 8px; white-space: nowrap; }
  .row:hover { background: var(--bg-2); }
  .row.sel { background: var(--sel); }
  .tw { width: 14px; text-align: center; color: var(--fg-2); font-size: 10px; transition: transform 0.1s; flex: none; }
  .tw.open { transform: rotate(90deg); }
  .tw.hidden { visibility: hidden; }
</style>
