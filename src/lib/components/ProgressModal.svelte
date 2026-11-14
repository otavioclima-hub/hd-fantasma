<script lang="ts">
  import { S, cancelRead } from "$lib/store.svelte";
  import { t, locale, type Key } from "$lib/i18n";
  import { formatNumber } from "$lib/format";

  const p = $derived(S.progress);
  const phase = $derived(p ? $t(`phase.${p.phase}` as Key) : $t("progress.starting"));
</script>

<div class="backdrop">
  <div class="modal" role="alertdialog" aria-modal="true" aria-label={$t("progress.title", { name: S.readName })} style="width:min(500px,calc(100vw - 32px))">
    <header>{$t("progress.title", { name: S.readName })}</header>
    <div class="body">
      <div class="phase">{phase}</div>
      <div class="track"><div class="ind"></div></div>
      <div class="num muted">{$t("progress.items", { n: formatNumber(p?.itemsSeen ?? 0, $locale) })}</div>
      <div class="cur muted ellipsis" title={p?.currentPath ?? ""}>{p?.currentPath ?? ""}</div>
      {#if p?.message}<div class="cur">{p.message}</div>{/if}
    </div>
    <footer><button class="btn" onclick={cancelRead}>{$t("cancel")}</button></footer>
  </div>
</div>

<style>
  .phase { font-weight: 600; margin-bottom: 8px; }
  .track { height: 5px; border-radius: 3px; background: var(--bg-3); overflow: hidden; margin-bottom: 8px; }
  .ind { width: 35%; height: 100%; border-radius: 3px; background: var(--accent); animation: slide 1.2s ease-in-out infinite; }
  @keyframes slide { 0% { margin-left: -35%; } 100% { margin-left: 100%; } }
  .cur { font-size: 12px; height: 18px; }
</style>
