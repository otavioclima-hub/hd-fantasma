<script lang="ts">
  import { S, diskById, revealSelected } from "$lib/store.svelte";
  import { t, locale } from "$lib/i18n";
  import { formatBytes, formatDate } from "$lib/format";

  const it = $derived(S.selItem);
  const disk = $derived(it ? diskById(it.diskId) : null);
</script>

<div class="bar">
  {#if !it}
    <span class="muted">{$t("details.none")}</span>
  {:else}
    <div class="main">
      <div class="path ellipsis" title={it.path}><strong>{it.name}</strong> <span class="muted">— {it.path}</span></div>
      <div class="meta muted">
        <span>{$t("details.disk")}: {disk?.name ?? "—"}</span>
        <span>{$t("details.size")}: {formatBytes(it.sizeBytes, $locale)}</span>
        <span>{$t("details.created")}: {formatDate(it.createdAt, $locale)}</span>
        <span>{$t("details.modified")}: {formatDate(it.modifiedAt, $locale)}</span>
      </div>
    </div>
    <div class="act">
      {#if disk && !disk.present}<span class="muted">{$t("plugDisk", { name: disk.name })}</span>{/if}
      <button class="btn" disabled={!disk?.present} onclick={revealSelected}>{$t("reveal")}</button>
    </div>
  {/if}
</div>

<style>
  .bar { display: flex; align-items: center; gap: 12px; padding: 6px 12px; min-height: 48px; border-top: 1px solid var(--border); background: var(--bg-2); }
  .main { flex: 1; min-width: 0; user-select: text; }
  .meta { display: flex; flex-wrap: wrap; gap: 2px 14px; font-size: 12px; margin-top: 1px; }
  .act { display: flex; align-items: center; gap: 10px; flex: none; }
</style>
