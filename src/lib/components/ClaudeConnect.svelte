<script lang="ts">
  import { S, refreshClaude, claudeConnect, claudeDisconnect, toast } from "$lib/store.svelte";
  import { t } from "$lib/i18n";

  let open = $state(false);
  let root: HTMLDivElement | undefined = $state();
  const c = $derived(S.claude);
  const anyOn = $derived(!!c && (c.desktop.connected || c.code.connected));
  const canConnectAll = $derived(!!c && ((c.desktop.installed && !c.desktop.connected) || (c.code.cliFound && !c.code.connected)));

  function toggle() {
    open = !open;
    if (open) { S.claudeJustDesktop = false; void refreshClaude(); }
  }
  function onDoc(e: MouseEvent) { if (open && root && !root.contains(e.target as Node)) open = false; }
  async function copy() {
    try { await navigator.clipboard.writeText(c?.code.command ?? ""); toast($t("claude.copied")); } catch { /* sem permissão */ }
  }
</script>

<svelte:window onmousedown={onDoc} onkeydown={(e) => e.key === "Escape" && open && (open = false)} />

<div class="wrap" bind:this={root}>
  <button class="btn" class:ok={anyOn} class:active={open} aria-haspopup="dialog" aria-expanded={open} onclick={toggle}>
    {#if anyOn}
      <span class="dot" aria-hidden="true"></span>
    {:else}
      <svg width="13" height="13" viewBox="0 0 16 16" aria-hidden="true"><path d="M8 1.5v3M8 11.5v3M1.5 8h3M11.5 8h3M3.4 3.4l2.1 2.1M10.5 10.5l2.1 2.1M12.6 3.4l-2.1 2.1M5.5 10.5l-2.1 2.1" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" fill="none" /></svg>
    {/if}
    {anyOn ? $t("claude.connected") : $t("claude.connect")}
  </button>

  {#if open}
    <div class="pop" role="dialog" aria-label={$t("claude.title")}>
      <div class="intro muted">{$t("claude.intro")}</div>

      {#if !c}
        <div class="muted pad">{$t("claude.checking")}</div>
      {:else}
        <div class="row">
          <div class="name">{$t("claude.desktop")}</div>
          <div class="st" class:on={c.desktop.connected} class:none={!c.desktop.installed}>
            {c.desktop.connected ? $t("claude.s.on") : c.desktop.installed ? $t("claude.s.off") : $t("claude.s.none")}
          </div>
          {#if c.desktop.installed}
            {#if c.desktop.connected}
              <button class="btn" disabled={S.claudeBusy} onclick={() => claudeDisconnect("desktop")}>{$t("claude.row.disconnect")}</button>
            {:else}
              <button class="btn" disabled={S.claudeBusy} onclick={() => claudeConnect("desktop")}>{$t("claude.row.connect")}</button>
            {/if}
          {/if}
        </div>
        {#if S.claudeJustDesktop && c.desktop.connected}<div class="note">{$t("claude.restart")}</div>{/if}

        <div class="row">
          <div class="name">{$t("claude.code")}</div>
          <div class="st" class:on={c.code.connected} class:none={!c.code.cliFound}>
            {c.code.connected ? $t("claude.s.on") : c.code.cliFound ? $t("claude.s.off") : $t("claude.s.none")}
          </div>
          {#if c.code.cliFound}
            {#if c.code.connected}
              <button class="btn" disabled={S.claudeBusy} onclick={() => claudeDisconnect("code")}>{$t("claude.row.disconnect")}</button>
            {:else}
              <button class="btn" disabled={S.claudeBusy} onclick={() => claudeConnect("code")}>{$t("claude.row.connect")}</button>
            {/if}
          {/if}
        </div>
        {#if !c.code.cliFound}
          <div class="cmd">
            <div class="muted">{$t("claude.noCli")}</div>
            <div class="line"><code>{c.code.command}</code><button class="btn" onclick={copy}>{$t("claude.copy")}</button></div>
          </div>
        {/if}
      {/if}

      {#if S.claudeError}<div class="err" role="alert">{S.claudeError}</div>{/if}

      <div class="foot">
        <button class="btn primary" disabled={S.claudeBusy || !canConnectAll} onclick={() => claudeConnect()}>
          {S.claudeBusy ? $t("claude.working") : $t("claude.connect")}
        </button>
      </div>
    </div>
  {/if}
</div>

<style>
  .wrap { position: relative; }
  .btn.ok { border-color: color-mix(in srgb, var(--ok) 45%, var(--border)); }
  .dot { width: 8px; height: 8px; border-radius: 50%; background: var(--ok); box-shadow: 0 0 0 2px color-mix(in srgb, var(--ok) 25%, transparent); }
  .pop {
    position: absolute; top: calc(100% + 6px); right: 0; width: 340px; z-index: 60; background: var(--bg);
    border: 1px solid var(--border); border-radius: 10px; box-shadow: var(--shadow); padding: 12px; display: flex; flex-direction: column; gap: 8px;
  }
  .intro { font-size: 12px; padding-bottom: 2px; }
  .pad { padding: 6px 0; }
  .row { display: flex; align-items: center; gap: 8px; padding: 6px 0; border-top: 1px solid var(--border); }
  .name { font-weight: 600; flex: 1; }
  .st { font-size: 12px; color: var(--fg-2); }
  .st.on { color: var(--ok); }
  .st.none { opacity: 0.8; }
  .note { font-size: 12px; background: var(--accent-soft); color: var(--fg); border-radius: 6px; padding: 6px 8px; }
  .cmd { font-size: 12px; display: flex; flex-direction: column; gap: 6px; }
  .line { display: flex; gap: 6px; align-items: stretch; }
  code {
    flex: 1; min-width: 0; font: 11px ui-monospace, SFMono-Regular, Menlo, monospace; background: var(--bg-2); border: 1px solid var(--border);
    border-radius: 6px; padding: 5px 7px; user-select: text; word-break: break-all; max-height: 72px; overflow: auto;
  }
  .line .btn { height: auto; min-height: 26px; }
  .err { color: var(--warn); font-size: 12px; }
  .foot { display: flex; justify-content: flex-end; padding-top: 4px; border-top: 1px solid var(--border); }
</style>
