<script lang="ts">
  import { onMount } from "svelte";
  import { S, init, runSearch, searchActive } from "$lib/store.svelte";
  import { t } from "$lib/i18n";
  import Toolbar from "$lib/components/Toolbar.svelte";
  import FilterBar from "$lib/components/FilterBar.svelte";
  import DiskList from "$lib/components/DiskList.svelte";
  import FolderTree from "$lib/components/FolderTree.svelte";
  import ItemTable from "$lib/components/ItemTable.svelte";
  import DetailsBar from "$lib/components/DetailsBar.svelte";
  import DuplicatesView from "$lib/components/DuplicatesView.svelte";
  import EmptyState from "$lib/components/EmptyState.svelte";
  import ReadSheet from "$lib/components/ReadSheet.svelte";
  import ProgressModal from "$lib/components/ProgressModal.svelte";
  import SettingsDialog from "$lib/components/SettingsDialog.svelte";
  import DialogHost from "$lib/components/DialogHost.svelte";
  import Splitter from "$lib/components/Splitter.svelte";

  let searchEl: HTMLInputElement | undefined = $state();
  let disksW = $state(240);
  let treeW = $state(240);

  onMount(() => {
    try {
      const saved = JSON.parse(localStorage.getItem("hdf-cols") ?? "null");
      if (saved) { disksW = saved.disksW ?? disksW; treeW = saved.treeW ?? treeW; }
    } catch { /* sem armazenamento */ }
    init();
  });
  const saveCols = () => { try { localStorage.setItem("hdf-cols", JSON.stringify({ disksW, treeW })); } catch { /* ignora */ } };

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
        <div class="col" class:dim={searching} style="width:{treeW}px"><FolderTree /></div>
        <Splitter bind:value={treeW} min={140} max={520} onchange={saveCols} />
        <div class="col grow"><ItemTable /></div>
      {/if}
    {/if}
  </div>

  <DetailsBar />
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
  .col.dim { opacity: 0.85; }
  .toast {
    position: fixed; bottom: 62px; left: 50%; transform: translateX(-50%); background: var(--fg); color: var(--bg);
    padding: 7px 14px; border-radius: 8px; box-shadow: var(--shadow); z-index: 70; max-width: 80vw;
  }
</style>
