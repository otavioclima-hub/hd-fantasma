<script lang="ts">
  import { S, diskById, activate, goUp, gotoResult, loadMore, toggleRowArchive, expandable, searchActive } from "$lib/store.svelte";
  import { t, locale, type Key } from "$lib/i18n";
  import { formatBytes, formatDate, formatDuration, formatNumber, formatResolution, parentPath } from "$lib/format";
  import type { Item } from "$lib/types";
  import KindIcon from "./KindIcon.svelte";

  const ROW = 26;
  const OVERSCAN = 8;
  interface Row { item: Item; depth: number }

  const searching = $derived(searchActive());

  function cmp(a: Item, b: Item): number {
    const { key, dir } = S.sort;
    let r = 0;
    switch (key) {
      case "size": r = a.sizeBytes - b.sizeBytes; break;
      case "kind": r = a.kind.localeCompare(b.kind) || (a.ext ?? "").localeCompare(b.ext ?? ""); break;
      case "modified": r = (a.modifiedAt ?? "").localeCompare(b.modifiedAt ?? ""); break;
      case "duration": r = (a.video?.durationSec ?? -1) - (b.video?.durationSec ?? -1); break;
      case "resolution": r = (a.video?.width ?? 0) * (a.video?.height ?? 0) - (b.video?.width ?? 0) * (b.video?.height ?? 0); break;
      case "codec": r = (a.video?.codec ?? "").localeCompare(b.video?.codec ?? ""); break;
      case "disk": r = (diskById(a.diskId)?.name ?? "").localeCompare(diskById(b.diskId)?.name ?? ""); break;
      case "path": r = a.path.localeCompare(b.path, undefined, { numeric: true }); break;
      default: r = a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: "base" });
    }
    return r * dir || a.name.localeCompare(b.name);
  }
  const sorted = (l: Item[]): Item[] => {
    if (searching) return [...l].sort(cmp);
    return [...l.filter((i) => i.kind === "folder").sort(cmp), ...l.filter((i) => i.kind !== "folder").sort(cmp)];
  };

  const flat = $derived.by<Row[]>(() => {
    if (searching) return sorted(S.results).map((item) => ({ item, depth: 0 }));
    const out: Row[] = [];
    const walk = (list: Item[], depth: number) => {
      for (const item of sorted(list)) {
        out.push({ item, depth });
        if (S.tableExpanded[item.id] && S.tableChildren[item.id]) walk(S.tableChildren[item.id], depth + 1);
      }
    };
    walk(S.rows, 0);
    return out;
  });

  const showVideo = $derived(searching || flat.some((r) => r.item.video));

  type Col = { key: string; label: Key; width: string; align?: "right" };
  const cols = $derived.by<Col[]>(() => {
    const c: Col[] = [{ key: "name", label: "col.name", width: searching ? "minmax(200px,1.2fr)" : "minmax(220px,1fr)" }];
    if (searching) {
      c.push({ key: "disk", label: "col.disk", width: "120px" }, { key: "path", label: "col.path", width: "minmax(160px,1.5fr)" });
    }
    c.push(
      { key: "size", label: "col.size", width: "88px", align: "right" },
      { key: "kind", label: "col.kind", width: "92px" },
      { key: "modified", label: "col.modified", width: "128px" },
    );
    if (showVideo) {
      c.push({ key: "duration", label: "col.duration", width: "68px", align: "right" }, { key: "resolution", label: "col.resolution", width: "92px" }, { key: "codec", label: "col.codec", width: "70px" });
    }
    return c;
  });
  const template = $derived(cols.map((c) => c.width).join(" "));
  const minW = $derived(cols.reduce((s, c) => s + (parseInt(c.width.match(/\d+/)?.[0] ?? "100") || 100), 0) + 24);

  // ---- virtualização ----
  let scroller: HTMLDivElement | undefined = $state();
  let scrollTop = $state(0);
  let viewH = $state(400);
  const start = $derived(Math.max(0, Math.floor(scrollTop / ROW) - OVERSCAN));
  const end = $derived(Math.min(flat.length, Math.ceil((scrollTop + viewH) / ROW) + OVERSCAN));
  const visible = $derived(flat.slice(start, end));

  $effect(() => {
    if (searching && flat.length && end >= flat.length - 20 && S.results.length < S.total) loadMore();
  });
  // volta ao topo quando a lista muda de pasta ou de busca
  $effect(() => {
    void S.folderId; void S.selDiskId;
    if (!searching && scroller) scroller.scrollTop = 0;
  });
  let lastQ = "";
  $effect(() => {
    const q = JSON.stringify([S.text, S.kinds, S.minVal, S.maxVal, S.after, S.before, S.diskFilter]);
    if (q !== lastQ && scroller) { scroller.scrollTop = 0; lastQ = q; }
  });

  function ensureVisible(i: number) {
    if (!scroller) return;
    const top = i * ROW, bottom = top + ROW;
    if (top < scroller.scrollTop) scroller.scrollTop = top;
    else if (bottom > scroller.scrollTop + viewH - ROW) scroller.scrollTop = bottom - viewH + ROW;
  }

  const selIndex = $derived(S.selItem ? flat.findIndex((r) => r.item.id === S.selItem!.id) : -1);

  function select(item: Item) {
    if (searching) gotoResult(item);
    else S.selItem = item;
  }
  function move(delta: number) {
    if (!flat.length) return;
    const i = selIndex < 0 ? (delta > 0 ? 0 : flat.length - 1) : Math.max(0, Math.min(flat.length - 1, selIndex + delta));
    select(flat[i].item);
    ensureVisible(i);
  }
  function onkey(e: KeyboardEvent) {
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    switch (e.key) {
      case "ArrowDown": e.preventDefault(); move(1); break;
      case "ArrowUp": e.preventDefault(); move(-1); break;
      case "PageDown": e.preventDefault(); move(Math.floor(viewH / ROW)); break;
      case "PageUp": e.preventDefault(); move(-Math.floor(viewH / ROW)); break;
      case "Home": e.preventDefault(); move(-1e9); break;
      case "End": e.preventDefault(); move(1e9); break;
      case "Enter": if (S.selItem) { e.preventDefault(); activate(S.selItem); } break;
      case "Backspace": if (!searching) { e.preventDefault(); goUp(); } break;
      case "ArrowRight": if (S.selItem && expandable(S.selItem) && !S.tableExpanded[S.selItem.id]) { e.preventDefault(); toggleRowArchive(S.selItem); } break;
      case "ArrowLeft": if (S.selItem && S.tableExpanded[S.selItem.id]) { e.preventDefault(); toggleRowArchive(S.selItem); } break;
    }
  }
  function sortBy(key: string) {
    S.sort = S.sort.key === key ? { key, dir: (S.sort.dir * -1) as 1 | -1 } : { key, dir: 1 };
  }
  const kindLabel = (i: Item) => (i.kind === "other" || i.kind === "archive" || i.kind === "document") && i.ext ? i.ext.toUpperCase() : $t(`kind.${i.kind}` as Key);
  const loading = $derived(searching ? S.searching && !S.results.length : S.rowsLoading);
</script>

<div class="wrap">
  {#if searching}
    <div class="strip muted">
      {#if S.searching && !S.results.length}{$t("loading")}
      {:else if S.results.length < S.total}{$t("results.partial", { shown: formatNumber(S.results.length, $locale), n: formatNumber(S.total, $locale) })}
      {:else}{$t("results", { n: formatNumber(S.total, $locale) })}{/if}
    </div>
  {:else if S.trail.length}
    <div class="strip muted ellipsis" title={S.trail.map((i) => i.name).join(" / ")}>{S.trail.map((i) => i.name).join("  ›  ")}</div>
  {/if}

  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div class="scroller" bind:this={scroller} bind:clientHeight={viewH} onscroll={(e) => (scrollTop = e.currentTarget.scrollTop)}
       tabindex="0" role="grid" aria-rowcount={flat.length} onkeydown={onkey}>
    <div class="inner" style="min-width:{minW}px">
      <div class="header" style="grid-template-columns:{template}" role="row">
        {#each cols as c}
          <button class="th" class:r={c.align === "right"} onclick={() => sortBy(c.key)} role="columnheader"
                  aria-sort={S.sort.key === c.key ? (S.sort.dir === 1 ? "ascending" : "descending") : "none"}>
            {$t(c.label)}{#if S.sort.key === c.key}<span class="arrow">{S.sort.dir === 1 ? "▲" : "▼"}</span>{/if}
          </button>
        {/each}
      </div>
      <div class="body" style="height:{flat.length * ROW}px">
        {#each visible as r, k (r.item.id)}
          {@const it = r.item}
          {@const i = start + k}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <div class="row" class:sel={S.selItem?.id === it.id} class:alt={i % 2 === 1}
               style="top:{i * ROW}px;grid-template-columns:{template}" role="row" tabindex="-1" aria-selected={S.selItem?.id === it.id}
               onclick={() => { select(it); scroller?.focus(); }} ondblclick={() => activate(it)}>
            <div class="cell name" style="padding-left:{8 + r.depth * 16}px">
              {#if expandable(it)}
                <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
                <span class="tw" class:open={S.tableExpanded[it.id]} onclick={(e) => { e.stopPropagation(); toggleRowArchive(it); }}>▸</span>
              {:else}<span class="tw none"></span>{/if}
              <KindIcon kind={it.kind} />
              <span class="ellipsis" title={it.name}>{it.name}</span>
            </div>
            {#if searching}
              <div class="cell ellipsis muted" title={diskById(it.diskId)?.name}>{diskById(it.diskId)?.name ?? "—"}</div>
              <div class="cell ellipsis muted" title={it.path}>{parentPath(it) || "/"}</div>
            {/if}
            <div class="cell r num">{formatBytes(it.sizeBytes, $locale)}</div>
            <div class="cell ellipsis muted">{kindLabel(it)}</div>
            <div class="cell ellipsis muted num">{formatDate(it.modifiedAt, $locale)}</div>
            {#if showVideo}
              <div class="cell r num">{formatDuration(it.video?.durationSec)}</div>
              <div class="cell num">{formatResolution(it.video)}</div>
              <div class="cell ellipsis muted">{it.video?.codec ?? ""}</div>
            {/if}
          </div>
        {/each}
      </div>
      {#if loading}
        <p class="note muted">{$t("loading")}</p>
      {:else if !flat.length}
        <p class="note muted">{searching ? $t("noResults") : S.selDiskId == null ? $t("pickDisk") : $t("emptyFolder")}</p>
      {/if}
    </div>
  </div>
</div>

<style>
  .wrap { display: flex; flex-direction: column; height: 100%; min-width: 0; background: var(--bg); }
  .strip { padding: 6px 12px 4px; font-size: 12px; border-bottom: 1px solid var(--border); flex: none; }
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
  .scroller:focus-within .row.sel { background: var(--sel); box-shadow: inset 2px 0 0 var(--accent); }
  .cell { padding: 0 8px; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .cell.r { text-align: right; }
  .cell.name { display: flex; align-items: center; gap: 6px; }
  .tw { width: 12px; text-align: center; font-size: 10px; color: var(--fg-2); transition: transform 0.1s; flex: none; }
  .tw.open { transform: rotate(90deg); }
  .tw.none { visibility: hidden; }
  .note { padding: 14px 16px; margin: 0; position: sticky; left: 0; }
</style>
