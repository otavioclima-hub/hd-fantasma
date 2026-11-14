<script lang="ts">
  import { S, searchActive, openDuplicates } from "$lib/store.svelte";
  import { t } from "$lib/i18n";
  import { isMock } from "$lib/api";

  let { searchEl = $bindable() }: { searchEl?: HTMLInputElement } = $props();
  const nFilters = $derived(
    (S.kinds.length ? 1 : 0) + (S.minVal || S.maxVal ? 1 : 0) + (S.after || S.before ? 1 : 0) + (S.diskFilter.length ? 1 : 0),
  );
</script>

<div class="bar">
  <div class="search">
    <svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true"><circle cx="7" cy="7" r="4.6" fill="none" stroke="currentColor" stroke-width="1.5" /><path d="M10.5 10.5L14 14" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" /></svg>
    <input bind:this={searchEl} type="search" placeholder={$t("search.placeholder")} bind:value={S.text}
           onkeydown={(e) => e.key === "Escape" && (S.text = "")} />
  </div>
  <button class="btn" class:active={S.filtersOpen || nFilters > 0} onclick={() => (S.filtersOpen = !S.filtersOpen)}>
    <svg width="13" height="13" viewBox="0 0 16 16" aria-hidden="true"><path d="M2 3.5h12L9.5 9v4l-3 1.5V9z" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round" /></svg>
    {$t("filters")}{nFilters ? ` (${nFilters})` : ""}
  </button>
  <div class="grow"></div>
  {#if isMock}<span class="muted demo" title={$t("mock.banner")}>{$t("mock.banner")}</span>{/if}
  <button class="btn" class:active={S.view === "dup"} onclick={() => (S.view === "dup" ? (S.view = "browse") : openDuplicates())}>{$t("duplicates")}</button>
  <button class="btn primary" onclick={() => (S.sheetOpen = true)}>{$t("readDisk")}</button>
  <button class="btn icon" title={$t("settings")} aria-label={$t("settings")} onclick={() => (S.settingsOpen = true)}>
    <svg width="15" height="15" viewBox="0 0 16 16" aria-hidden="true"><circle cx="8" cy="8" r="2.3" fill="none" stroke="currentColor" stroke-width="1.4" /><path d="M8 1.5v2M8 12.5v2M1.5 8h2M12.5 8h2M3.4 3.4l1.4 1.4M11.2 11.2l1.4 1.4M12.6 3.4l-1.4 1.4M4.8 11.2l-1.4 1.4" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" /></svg>
  </button>
</div>

<style>
  .bar { display: flex; gap: 8px; align-items: center; padding: 8px 10px; border-bottom: 1px solid var(--border); background: var(--bg-2); }
  .grow { flex: 1; }
  .search { position: relative; flex: 0 1 380px; min-width: 160px; display: flex; align-items: center; }
  .search svg { position: absolute; left: 8px; color: var(--fg-2); pointer-events: none; }
  .search input { width: 100%; padding-left: 27px; }
  .demo { font-size: 11px; padding-right: 4px; }
  @media (max-width: 1000px) { .demo { display: none; } }
</style>
