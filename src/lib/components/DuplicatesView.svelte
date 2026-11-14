<script lang="ts">
  import { S, diskById, loadDuplicates, gotoResult } from "$lib/store.svelte";
  import { t, locale } from "$lib/i18n";
  import { formatBytes, formatNumber } from "$lib/format";
  import KindIcon from "./KindIcon.svelte";

  let shown = $state(100);
  $effect(() => { void S.dupGroups; shown = 100; });
  const mins = [0, 1e6, 100e6, 1e9];
  const minLabel = (n: number) => (n === 0 ? "—" : formatBytes(n, $locale));
</script>

<div class="wrap">
  <div class="top">
    <div>
      <strong>{$t("dup.title")}</strong>
      <span class="muted"> · {$t("dup.help")}</span>
    </div>
    <div class="grow"></div>
    <label class="muted">{$t("dup.minSize")}
      <select bind:value={S.dupMin} onchange={loadDuplicates}>
        {#each mins as m}<option value={m}>{minLabel(m)}</option>{/each}
      </select>
    </label>
    <button class="btn" onclick={() => (S.view = "browse")}>{$t("dup.back")}</button>
  </div>
  <div class="list">
    {#if S.dupLoading}
      <p class="muted pad">{$t("loading")}</p>
    {:else if !S.dupGroups.length}
      <p class="muted pad">{$t("dup.none")}</p>
    {:else}
      <div class="muted pad small">{$t("dup.groups", { n: formatNumber(S.dupGroups.length, $locale) })}</div>
      {#each S.dupGroups.slice(0, shown) as g (g.name + g.sizeBytes)}
        <section class="grp">
          <header>
            <KindIcon kind={g.items[0]?.kind ?? "other"} />
            <span class="ellipsis gname" title={g.name}>{g.name}</span>
            <span class="muted num">{formatBytes(g.sizeBytes, $locale)}</span>
            <span class="badge">{$t("dup.copies", { n: g.items.length })}</span>
          </header>
          {#each g.items as it (it.id)}
            <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
            <div class="loc" ondblclick={() => { S.view = "browse"; gotoResult(it, true); }} onclick={() => (S.selItem = it)} class:sel={S.selItem?.id === it.id}>
              <span class="dn ellipsis"><span class="dot" class:on={diskById(it.diskId)?.present}></span>{diskById(it.diskId)?.name ?? "—"}</span>
              <span class="ellipsis muted" title={it.path}>{it.path}</span>
            </div>
          {/each}
        </section>
      {/each}
      {#if S.dupGroups.length > shown}
        <div class="pad"><button class="btn" onclick={() => (shown += 100)}>{$t("dup.more")}</button></div>
      {/if}
    {/if}
  </div>
</div>

<style>
  .wrap { display: flex; flex-direction: column; height: 100%; min-width: 0; background: var(--bg); }
  .top { display: flex; align-items: center; gap: 10px; padding: 8px 12px; border-bottom: 1px solid var(--border); background: var(--bg-2); flex: none; }
  .grow { flex: 1; }
  .list { flex: 1; overflow: auto; padding-bottom: 12px; }
  .pad { padding: 8px 12px; margin: 0; }
  .small { font-size: 12px; }
  .grp { margin: 4px 12px 8px; border: 1px solid var(--border); border-radius: 8px; overflow: hidden; content-visibility: auto; contain-intrinsic-size: auto 110px; }
  .grp header { display: flex; align-items: center; gap: 8px; padding: 5px 10px; background: var(--bg-2); }
  .gname { font-weight: 600; flex: 1; min-width: 0; }
  .badge { background: var(--accent-soft); color: var(--accent); border-radius: 9px; padding: 0 8px; font-size: 11px; font-weight: 600; }
  .loc { display: grid; grid-template-columns: 170px 1fr; gap: 10px; height: var(--row-h); align-items: center; padding: 0 10px; border-top: 1px solid var(--border); }
  .loc:hover { background: var(--bg-2); }
  .loc.sel { background: var(--sel); }
  .dn { display: flex; align-items: center; gap: 6px; }
  .dot { width: 7px; height: 7px; border-radius: 50%; background: #9a97a6; flex: none; }
  .dot.on { background: var(--ok); }
</style>
