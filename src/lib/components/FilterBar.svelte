<script lang="ts">
  import { S, clearSearchFields } from "$lib/store.svelte";
  import { t, type Key } from "$lib/i18n";
  import type { Kind } from "$lib/types";

  const KINDS: Kind[] = ["video", "image", "audio", "document", "archive", "other"];
  let diskMenu = $state(false);

  function toggleKind(k: Kind) {
    S.kinds = S.kinds.includes(k) ? S.kinds.filter((x) => x !== k) : [...S.kinds, k];
  }
  function toggleDisk(id: number) {
    S.diskFilter = S.diskFilter.includes(id) ? S.diskFilter.filter((x) => x !== id) : [...S.diskFilter, id];
  }
  const diskLabel = $derived(
    S.diskFilter.length === 0 ? $t("filters.alldisks")
      : S.diskFilter.length === 1 ? (S.disks.find((d) => d.id === S.diskFilter[0])?.name ?? "")
      : $t("filters.disksSelected", { n: S.diskFilter.length }),
  );
</script>

<svelte:window onclick={() => (diskMenu = false)} />
<div class="filters">
  <div class="grp">
    <span class="lbl">{$t("filters.kind")}</span>
    {#each KINDS as k}
      <button class="chip" class:on={S.kinds.includes(k)} onclick={() => toggleKind(k)}>{$t(`kind.${k}` as Key)}</button>
    {/each}
  </div>
  <div class="grp">
    <span class="lbl">{$t("filters.size")}</span>
    <input type="number" min="0" step="any" placeholder={$t("filters.min")} bind:value={S.minVal} style="width:70px" />
    <select bind:value={S.minUnit}><option>MB</option><option>GB</option></select>
    <span class="muted">–</span>
    <input type="number" min="0" step="any" placeholder={$t("filters.max")} bind:value={S.maxVal} style="width:70px" />
    <select bind:value={S.maxUnit}><option>MB</option><option>GB</option></select>
  </div>
  <div class="grp">
    <span class="lbl">{$t("filters.date")}</span>
    <input type="date" bind:value={S.after} aria-label={$t("filters.from")} title={$t("filters.from")} />
    <span class="muted">–</span>
    <input type="date" bind:value={S.before} aria-label={$t("filters.to")} title={$t("filters.to")} />
  </div>
  <div class="grp" style="position:relative">
    <span class="lbl">{$t("filters.disks")}</span>
    <button class="btn" onclick={(e) => { e.stopPropagation(); diskMenu = !diskMenu; }}>{diskLabel} ▾</button>
    {#if diskMenu}
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <div class="pop" onclick={(e) => e.stopPropagation()}>
        {#each S.disks as d}
          <label><input type="checkbox" checked={S.diskFilter.includes(d.id)} onchange={() => toggleDisk(d.id)} /> {d.name}</label>
        {/each}
      </div>
    {/if}
  </div>
  <button class="btn" onclick={clearSearchFields}>{$t("filters.clear")}</button>
</div>

<style>
  .filters { display: flex; flex-wrap: wrap; gap: 8px 18px; align-items: center; padding: 7px 10px; border-bottom: 1px solid var(--border); background: var(--bg); }
  .grp { display: flex; align-items: center; gap: 5px; }
  .lbl { color: var(--fg-2); font-size: 11px; text-transform: uppercase; letter-spacing: 0.04em; margin-right: 2px; }
  .pop {
    position: absolute; top: 30px; left: 0; z-index: 30; min-width: 200px; max-height: 260px; overflow: auto; padding: 6px;
    background: var(--bg); border: 1px solid var(--border); border-radius: 8px; box-shadow: var(--shadow);
  }
  .pop label { display: flex; gap: 7px; align-items: center; padding: 4px 6px; border-radius: 4px; }
  .pop label:hover { background: var(--bg-2); }
  input[type="date"] { width: 128px; }
</style>
