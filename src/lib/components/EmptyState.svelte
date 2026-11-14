<script lang="ts">
  import { S, importCatalog } from "$lib/store.svelte";
  import { t } from "$lib/i18n";
  import Ghost from "./Ghost.svelte";
</script>

<div class="empty">
  <Ghost size={130} />
  <h2>{$t("empty.title")}</h2>
  <p class="muted">{$t("empty.text")}</p>
  <div class="row">
    <button class="btn primary" style="height:32px;padding:0 18px" onclick={() => (S.sheetOpen = true)}>{$t("empty.cta")}</button>
    <button class="btn" style="height:32px" onclick={importCatalog}>{$t("disks.import")}</button>
  </div>
</div>

<style>
  .empty { height: 100%; display: flex; flex-direction: column; align-items: center; justify-content: center; text-align: center; padding: 24px; gap: 4px; }
  h2 { margin: 12px 0 0; font-size: 20px; font-weight: 600; }
  p { max-width: 380px; margin: 4px 0 14px; }
  .row { display: flex; gap: 8px; }
</style>
