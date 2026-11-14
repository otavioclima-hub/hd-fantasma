<script lang="ts">
  import { S, selDisk } from "$lib/store.svelte";
  import { t, locale } from "$lib/i18n";
  import { formatBytes, formatNumber } from "$lib/format";

  const it = $derived(S.selItem);
  const disk = $derived(selDisk());
  const left = $derived.by(() => {
    if (it) {
      return it.kind === "folder"
        ? $t("status.sel", { n: formatNumber(it.descendantCount, $locale), size: formatBytes(it.sizeBytes, $locale) })
        : $t("status.one", { size: formatBytes(it.sizeBytes, $locale) });
    }
    return disk ? $t("status.sel", { n: formatNumber(disk.itemCount, $locale), size: formatBytes(disk.totalBytes, $locale) }) : "";
  });
  const total = $derived(S.disks.reduce((s, d) => s + d.itemCount, 0));
</script>

<div class="bar muted">
  <span class="ellipsis">{left}</span>
  <span class="grow"></span>
  <span>{$t("status.total", { n: formatNumber(total, $locale) })}</span>
</div>

<style>
  .bar { display: flex; gap: 12px; align-items: center; height: 24px; padding: 0 12px; font-size: 12px; border-top: 1px solid var(--border); background: var(--bg-2); flex: none; }
  .grow { flex: 1; }
</style>
