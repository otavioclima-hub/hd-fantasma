<script lang="ts">
  import { S, selDisk, kidsOf, ensureKids, selectItem, selectNothing, pickInColumn, toggleExpand, isDir } from "$lib/store.svelte";
  import { t, locale, type Key } from "$lib/i18n";
  import { formatBytes, formatDate } from "$lib/format";
  import type { Item } from "$lib/types";
  import KindIcon from "./KindIcon.svelte";
  import Chevron from "./Chevron.svelte";

  const ROW = 26, OVERSCAN = 8, INDENT = 18;
  interface Row { item: Item; depth: number }

  function cmp(a: Item, b: Item): number {
    const { key, dir } = S.listSort;
    let r = 0;
    switch (key) {
      case "size": r = a.sizeBytes - b.sizeBytes; break;
      case "kind": r = a.kind.localeCompare(b.kind) || (a.ext ?? "").localeCompare(b.ext ?? ""); break;
      case "created": r = (a.createdAt ?? "").localeCompare(b.createdAt ?? ""); break;
      case "modified": r = (a.modifiedAt ?? "").localeCompare(b.modifiedAt ?? ""); break;
      default: {
        // Nome mantém as pastas primeiro (e inverte só dentro de cada grupo)
        const fa = isDir(a) ? 0 : 1, fb = isDir(b) ? 0 : 1;
        if (fa !== fb) return fa - fb;
        r = a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: "base" });
      }
    }
    return r * dir || a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: "base" });
  }
  // ordenar 50 mil itens é caro: guarda o resultado por lista e por critério
  const memo = new WeakMap<Item[], { sig: string; out: Item[] }>();
  function sorted(l: Item[]): Item[] {
    const sig = S.listSort.key + S.listSort.dir;
    const m = memo.get(l);
    if (m && m.sig === sig) return m.out;
    const out = [...l].sort(cmp);
    memo.set(l, { sig, out });
    return out;
  }

  const disk = $derived(selDisk());
  const flat = $derived.by<Row[]>(() => {
    const d = S.selDiskId;
    if (d == null) return [];
    const out: Row[] = [];
    const walk = (list: Item[] | undefined, depth: number) => {
      if (!list) return;
      for (const it of sorted(list)) {
        out.push({ item: it, depth });
        if (isDir(it) && S.expanded[it.id]) walk(kidsOf(d, it.id), depth + 1);
      }
    };
    walk(kidsOf(d, null), 0);
    return out;
  });
  const rootLoaded = $derived(S.selDiskId != null && kidsOf(S.selDiskId, null) !== undefined);
  $effect(() => { if (S.selDiskId != null) void ensureKids(S.selDiskId, null); });

  type Col = { key: string; label: Key; align?: "right" };
  const cols: Col[] = [
    { key: "name", label: "col.name" },
    { key: "size", label: "col.size", align: "right" },
    { key: "kind", label: "col.kind" },
    { key: "created", label: "col.created" },
    { key: "modified", label: "col.modified" },
  ];
  const template = "minmax(240px,1fr) 96px 104px 142px 142px";
  const MINW = 240 + 96 + 104 + 142 * 2 + 24;

  // ---- virtualização ----
  let scroller: HTMLDivElement | undefined = $state();
  let scrollTop = $state(0);
  let viewH = $state(400);
  const start = $derived(Math.max(0, Math.floor(scrollTop / ROW) - OVERSCAN));
  const end = $derived(Math.min(flat.length, Math.ceil((scrollTop + viewH) / ROW) + OVERSCAN));
  const visible = $derived(flat.slice(start, end));

  $effect(() => { void S.selDiskId; if (scroller) scroller.scrollTop = 0; });

  function ensureVisible(i: number) {
    if (!scroller) return;
    const top = i * ROW, bottom = top + ROW;
    const hdr = ROW;
    if (top < scroller.scrollTop) scroller.scrollTop = top;
    else if (bottom > scroller.scrollTop + viewH - hdr) scroller.scrollTop = bottom - viewH + hdr;
  }
  // rola até o item quando a seleção muda de fora (busca, troca de vista), mas não a cada abrir/fechar pasta
  let revealed: number | null = null;
  $effect(() => {
    const id = S.selItem?.id ?? null;
    if (id === revealed) return;
    const i = id == null ? -1 : flat.findIndex((r) => r.item.id === id);
    if (i >= 0) { revealed = id; ensureVisible(i); }
    else if (id == null) revealed = null;
  });

  const selIndex = $derived(S.selItem ? flat.findIndex((r) => r.item.id === S.selItem!.id) : -1);
  function move(delta: number) {
    if (!flat.length) return;
    const i = selIndex < 0 ? (delta > 0 ? 0 : flat.length - 1) : Math.max(0, Math.min(flat.length - 1, selIndex + delta));
    revealed = flat[i].item.id;
    void selectItem(flat[i].item);
    ensureVisible(i);
  }
  function onkey(e: KeyboardEvent) {
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    const cur = selIndex >= 0 ? flat[selIndex] : null;
    switch (e.key) {
      case "ArrowDown": e.preventDefault(); move(1); break;
      case "ArrowUp": e.preventDefault(); move(-1); break;
      case "PageDown": e.preventDefault(); move(Math.floor(viewH / ROW)); break;
      case "PageUp": e.preventDefault(); move(-Math.floor(viewH / ROW)); break;
      case "Home": e.preventDefault(); move(-1e9); break;
      case "End": e.preventDefault(); move(1e9); break;
      case "ArrowRight":
        if (cur && isDir(cur.item)) { e.preventDefault(); if (!S.expanded[cur.item.id]) void toggleExpand(cur.item); else move(1); }
        break;
      case "ArrowLeft":
        if (cur) {
          e.preventDefault();
          if (isDir(cur.item) && S.expanded[cur.item.id]) S.expanded[cur.item.id] = false;
          else if (cur.item.parentId != null) {
            const p = flat.slice(0, selIndex).reverse().find((r) => r.item.id === cur.item.parentId);
            if (p) { revealed = p.item.id; void selectItem(p.item); ensureVisible(flat.indexOf(p)); }
          }
        }
        break;
      case "Enter": if (cur && isDir(cur.item)) { e.preventDefault(); void toggleExpand(cur.item); } break;
    }
  }
  function sortBy(key: string) {
    S.listSort = S.listSort.key === key ? { key, dir: (S.listSort.dir * -1) as 1 | -1 } : { key, dir: 1 };
  }
  const kindLabel = (i: Item) => (i.kind === "other" || i.kind === "archive" || i.kind === "document") && i.ext ? i.ext.toUpperCase() : $t(`kind.${i.kind}` as Key);
</script>

<div class="wrap">
  <div class="strip muted" aria-label="breadcrumb">
    {#if disk}
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <span class="crumb" onclick={selectNothing}>{disk.name}</span>
      {#each S.path as p, i (p.id)}
        <span class="sepr">›</span>
        <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
        <span class="crumb" class:last={i === S.path.length - 1} onclick={() => pickInColumn(i, p)}>{p.name}</span>
      {/each}
    {/if}
  </div>

  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div class="scroller" bind:this={scroller} bind:clientHeight={viewH} onscroll={(e) => (scrollTop = e.currentTarget.scrollTop)}
       tabindex="0" role="treegrid" aria-rowcount={flat.length} onkeydown={onkey}>
    <div class="inner" style="min-width:{MINW}px">
      <div class="header" style="grid-template-columns:{template}" role="row">
        {#each cols as c}
          <button class="th" class:r={c.align === "right"} onclick={() => sortBy(c.key)} role="columnheader"
                  aria-sort={S.listSort.key === c.key ? (S.listSort.dir === 1 ? "ascending" : "descending") : "none"}>
            {$t(c.label)}{#if S.listSort.key === c.key}<span class="arrow">{S.listSort.dir === 1 ? "▲" : "▼"}</span>{/if}
          </button>
        {/each}
      </div>
      <div class="body" style="height:{flat.length * ROW}px">
        {#each visible as r, k (r.item.id)}
          {@const it = r.item}
          {@const i = start + k}
          {@const dir = isDir(it)}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <div class="row" class:sel={S.selItem?.id === it.id} class:alt={i % 2 === 1}
               style="top:{i * ROW}px;grid-template-columns:{template}" role="row" tabindex="-1" aria-selected={S.selItem?.id === it.id}
               aria-expanded={dir ? (S.expanded[it.id] ?? false) : undefined}
               onclick={() => { void selectItem(it); scroller?.focus(); }} ondblclick={() => dir && toggleExpand(it)}>
            <div class="cell name" style="padding-left:{4 + r.depth * INDENT}px">
              <Chevron open={dir && S.expanded[it.id]} hidden={!dir || it.childCount === 0} title={dir ? $t("expand") : undefined}
                       onclick={dir ? (e) => { e.stopPropagation(); void toggleExpand(it); } : undefined} />
              <KindIcon kind={it.kind} />
              <span class="ellipsis" title={it.name}>{it.name}</span>
              {#if S.loadingKids[String(it.id)]}<span class="muted">…</span>{/if}
            </div>
            <div class="cell r num">{formatBytes(it.sizeBytes, $locale)}</div>
            <div class="cell ellipsis muted">{kindLabel(it)}</div>
            <div class="cell ellipsis muted num">{formatDate(it.createdAt, $locale)}</div>
            <div class="cell ellipsis muted num">{formatDate(it.modifiedAt, $locale)}</div>
          </div>
        {/each}
      </div>
      {#if S.selDiskId == null}
        <p class="note muted">{$t("pickDisk")}</p>
      {:else if !rootLoaded}
        <p class="note muted">{$t("loading")}</p>
      {:else if !flat.length}
        <p class="note muted">{$t("emptyFolder")}</p>
      {/if}
    </div>
  </div>
</div>

<style>
  .wrap { display: flex; flex-direction: column; height: 100%; min-width: 0; background: var(--bg); }
  .strip { padding: 6px 12px 4px; font-size: 12px; border-bottom: 1px solid var(--border); flex: none; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-height: 26px; }
  .crumb { cursor: pointer; }
  .crumb:hover { color: var(--fg); text-decoration: underline; }
  .crumb.last { color: var(--fg); }
  .sepr { margin: 0 5px; }
  .scroller { flex: 1; overflow: auto; position: relative; outline: none; }
  .header { display: grid; position: sticky; top: 0; z-index: 3; background: var(--bg-2); border-bottom: 1px solid var(--border); height: var(--row-h); }
  .th { text-align: left; border: 0; background: none; padding: 0 8px; font-size: 11px; font-weight: 600; color: var(--fg-2); text-transform: uppercase; letter-spacing: 0.04em; border-right: 1px solid var(--border); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .th:last-child { border-right: 0; }
  .th:hover { background: var(--bg-3); }
  .th.r { text-align: right; }
  .arrow { font-size: 8px; margin-left: 4px; }
  .body { position: relative; }
  .row { position: absolute; left: 0; right: 0; height: var(--row-h); display: grid; align-items: center; }
  .row.alt { background: color-mix(in srgb, var(--bg-2) 55%, transparent); }
  .row:hover { background: var(--bg-2); }
  .row.sel { background: var(--sel); }
  .scroller:focus-within .row.sel { box-shadow: inset 2px 0 0 var(--accent); }
  .cell { padding: 0 8px; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .cell.r { text-align: right; }
  .cell.name { display: flex; align-items: center; gap: 4px; }
  .cell.name .ellipsis { margin-left: 2px; }
  .note { padding: 14px 16px; margin: 0; position: sticky; left: 0; }
</style>
