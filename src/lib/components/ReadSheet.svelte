<script lang="ts">
  const isWin = typeof navigator !== "undefined" && /Win/i.test(navigator.userAgent || "");
  import { S, startRead } from "$lib/store.svelte";
  import { t, locale } from "$lib/i18n";
  import { formatBytes } from "$lib/format";
  import * as api from "$lib/api";
  import type { MountedVolume } from "$lib/types";
  import DiskIcon from "./DiskIcon.svelte";

  let volumes = $state<MountedVolume[]>([]);
  let loading = $state(true);
  let error = $state("");
  let chosen = $state<string | null>(null);
  let hidden = $state(S.settings.includeHidden);

  async function load() {
    loading = true; error = "";
    try {
      volumes = await api.listMountedVolumes();
      if (!chosen || !volumes.some((v) => v.mountPath === chosen)) chosen = volumes[0]?.mountPath ?? null;
    } catch (e) { error = e instanceof Error ? e.message : String(e); } finally { loading = false; }
  }
  $effect(() => { load(); });

  const vol = $derived(volumes.find((v) => v.mountPath === chosen) ?? null);
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && (S.sheetOpen = false)} />
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="backdrop" onmousedown={(e) => e.target === e.currentTarget && (S.sheetOpen = false)}>
  <div class="modal" role="dialog" aria-modal="true" aria-label={$t("sheet.title")}>
    <header>{$t("sheet.title")}</header>
    <div class="body">
      <div class="vols">
        {#if loading}<p class="muted">{$t("loading")}</p>
        {:else if error}<p style="color:var(--warn)">{error}</p>
        {:else if !volumes.length}
          <p class="muted">{$t("sheet.noVolumes")}</p>
          <button class="btn" onclick={load}>{$t("sheet.refresh")}</button>
        {/if}
        {#each volumes as v (v.mountPath)}
          <label class="vol" class:sel={chosen === v.mountPath}>
            <input type="radio" name="vol" value={v.mountPath} bind:group={chosen} />
            <DiskIcon fs={v.filesystem} size={28} />
            <div class="vi">
              <div class="ellipsis"><strong>{v.name}</strong> {#if v.catalogedDiskId != null}<span class="tag">{$t("sheet.cataloged")}</span>{/if}</div>
              <div class="muted small ellipsis">{v.filesystem} · {formatBytes(v.capacityBytes, $locale)} · {v.mountPath}</div>
            </div>
          </label>
        {/each}
      </div>

      <label class="opt chk"><input type="checkbox" bind:checked={hidden} /> {$t("sheet.hidden")}</label>
      {#if isWin}<p class="muted small">{$t("sheet.macDisks.win")}</p>{/if}
    </div>
    <footer>
      <button class="btn" onclick={() => (S.sheetOpen = false)}>{$t("cancel")}</button>
      <button class="btn primary" disabled={!vol} onclick={() => vol && startRead(vol.mountPath, vol.name, { mode: "full", includeHidden: hidden })}>{$t("sheet.start")}</button>
    </footer>
  </div>
</div>

<style>
  .vols { display: flex; flex-direction: column; gap: 4px; max-height: 210px; overflow: auto; margin-bottom: 8px; }
  .vol, .opt { display: flex; gap: 10px; align-items: center; padding: 7px 9px; border: 1px solid var(--border); border-radius: 8px; }
  .vol.sel { border-color: var(--accent); background: var(--accent-soft); }
  .vi { min-width: 0; flex: 1; }
  .small { font-size: 11px; }
  .tag { font-size: 10px; background: var(--bg-3); border-radius: 8px; padding: 1px 7px; color: var(--fg-2); margin-left: 4px; }
  .opt.chk { border: 0; padding-left: 2px; align-items: center; }
</style>
