<script lang="ts">
  import { onMount } from "svelte";
  import { S, init, runSearch, searchActive, toggleInspector } from "$lib/store.svelte";
  import { t } from "$lib/i18n";
  import Toolbar from "$lib/components/Toolbar.svelte";
  import FilterBar from "$lib/components/FilterBar.svelte";
  import DiskList from "$lib/components/DiskList.svelte";
  import ItemTable from "$lib/components/ItemTable.svelte";
  import ColumnsView from "$lib/components/ColumnsView.svelte";
  import ListView from "$lib/components/ListView.svelte";
  import Inspector from "$lib/components/Inspector.svelte";
  import StatusBar from "$lib/components/StatusBar.svelte";
  import DuplicatesView from "$lib/components/DuplicatesView.svelte";
  import EmptyState from "$lib/components/EmptyState.svelte";
  import ReadSheet from "$lib/components/ReadSheet.svelte";
  import ProgressModal from "$lib/components/ProgressModal.svelte";
  import SettingsDialog from "$lib/components/SettingsDialog.svelte";
  import DialogHost from "$lib/components/DialogHost.svelte";
  import Splitter from "$lib/components/Splitter.svelte";

  let searchEl: HTMLInputElement | undefined = $state();
  let disksW = $state(240);
  let inspW = $state(300);

  onMount(() => {
    try {
      const saved = JSON.parse(localStorage.getItem("hdf-cols") ?? "null");
      if (saved) { disksW = saved.disksW ?? disksW; inspW = saved.inspW ?? inspW; }
    } catch { /* sem armazenamento */ }
    init();
  });
  const saveCols = () => { try { localStorage.setItem("hdf-cols", JSON.stringify({ disksW, inspW })); } catch { /* ignora */ } };

  // busca com atraso de 250 ms
  $effect(() => {
    void [S.text, S.kinds, S.minVal, S.maxVal, S.minUnit, S.maxUnit, S.after, S.before, S.diskFilter];
    const h = setTimeout(runSearch, 250);
    return () => clearTimeout(h);
  });

  function onkeydown(e: KeyboardEvent) {
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "f") {
      e.preventDefault();
      searchEl?.focus();
      searchEl?.select();
    } else if ((e.metaKey || e.ctrlKey) && !e.shiftKey && !e.altKey && e.key.toLowerCase() === "i") {
      e.preventDefault();
      toggleInspector();
    }
  }
  const empty = $derived(S.ready && S.disks.length === 0);
  const searching = $derived(searchActive());
</script>

<svelte:window {onkeydown} />

<div class="app">
  <Toolbar bind:searchEl />
  {#if S.filtersOpen}<FilterBar />{/if}

  <div class="main">
    {#if empty}
      <EmptyState />
    {:else}
      <div class="col" style="width:{disksW}px"><DiskList /></div>
      <Splitter bind:value={disksW} min={180} max={420} onchange={saveCols} />
      {#if S.view === "dup"}
        <div class="col grow"><DuplicatesView /></div>
      {:else}
        <div class="col grow">
          {#if searching}<ItemTable />{:else if S.settings.viewMode === "list"}<ListView />{:else}<ColumnsView />{/if}
        </div>
      {/if}
      {#if S.settings.showInspector}
        <Splitter bind:value={inspW} min={240} max={560} reverse onchange={saveCols} />
        <div class="col" style="width:{inspW}px"><Inspector /></div>
      {/if}
    {/if}
  </div>

  {#if !empty}<StatusBar />{/if}
  {#if S.toast}<div class="toast" role="status">{S.toast}</div>{/if}
</div>

{#if S.sheetOpen}<ReadSheet />{/if}
{#if S.reading}<ProgressModal />{/if}
{#if S.settingsOpen}<SettingsDialog />{/if}
<DialogHost />

<style>
  .app { display: flex; flex-direction: column; height: 100vh; }
  .main { flex: 1; display: flex; min-height: 0; }
  .col { flex: none; min-width: 0; height: 100%; overflow: hidden; }
  .col.grow { flex: 1; }
  .toast {
    position: fixed; bottom: 40px; left: 50%; transform: translateX(-50%); background: var(--fg); color: var(--bg);
    padding: 7px 14px; border-radius: 8px; box-shadow: var(--shadow); z-index: 70; max-width: 80vw;
  }
</style>
