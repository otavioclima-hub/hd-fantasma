<script lang="ts">
  import { S, saveSettings } from "$lib/store.svelte";
  import { t } from "$lib/i18n";
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && (S.settingsOpen = false)} />
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="backdrop" onmousedown={(e) => e.target === e.currentTarget && (S.settingsOpen = false)}>
  <div class="modal" role="dialog" aria-modal="true" aria-label={$t("settings.title")} style="width:min(420px,calc(100vw - 32px))">
    <header>{$t("settings.title")}</header>
    <div class="body">
      <label class="line">
        <span>{$t("settings.language")}</span>
        <select value={S.settings.language} onchange={(e) => saveSettings({ language: e.currentTarget.value as "pt" | "en" })}>
          <option value="pt">Português</option>
          <option value="en">English</option>
        </select>
      </label>
      <label class="line chk">
        <input type="checkbox" checked={S.settings.includeHidden} onchange={(e) => saveSettings({ includeHidden: e.currentTarget.checked })} />
        <span>{$t("settings.hidden")}<br /><span class="muted">{$t("settings.hiddenHelp")}</span></span>
      </label>
    </div>
    <footer><button class="btn primary" onclick={() => (S.settingsOpen = false)}>{$t("close")}</button></footer>
  </div>
</div>

<style>
  .line { display: flex; justify-content: space-between; align-items: center; gap: 12px; padding: 8px 0; }
  .chk { justify-content: flex-start; align-items: flex-start; }
</style>
