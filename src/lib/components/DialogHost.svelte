<script lang="ts">
  import { S } from "$lib/store.svelte";
  import { t } from "$lib/i18n";

  let value = $state("");
  let input: HTMLInputElement | undefined = $state();
  $effect(() => {
    if (S.dialog?.kind === "prompt") {
      value = S.dialog.value ?? "";
      queueMicrotask(() => { input?.focus(); input?.select(); });
    }
  });
  function done(v: boolean | string | null) {
    const d = S.dialog;
    S.dialog = null;
    d?.resolve(v);
  }
</script>

{#if S.busy}
  <div class="backdrop" aria-busy="true">
    <div class="modal" role="status" aria-live="polite" style="width:min(380px,calc(100vw - 32px))">
      <div class="body busy"><span class="spin" aria-hidden="true"></span>{S.busy}</div>
    </div>
  </div>
{/if}

{#if S.dialog}
  {@const d = S.dialog}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="backdrop" onmousedown={(e) => e.target === e.currentTarget && done(null)}
       onkeydown={(e) => { if (e.key === "Escape") done(null); }}>
    <div class="modal" role="dialog" aria-modal="true" aria-label={d.title} style="width:min(440px,calc(100vw - 32px))">
      <header>{d.title}</header>
      <div class="body">
        {#if d.kind === "confirm"}
          <p class="muted" style="margin:0">{d.text}</p>
        {:else}
          <input bind:this={input} type="text" bind:value style="width:100%"
                 onkeydown={(e) => e.key === "Enter" && done(value)} />
        {/if}
      </div>
      <footer>
        <button class="btn" onclick={() => done(null)}>{$t("cancel")}</button>
        <button class="btn" class:danger={d.danger} class:primary={!d.danger}
                onclick={() => done(d.kind === "confirm" ? true : value)}>{d.okLabel}</button>
      </footer>
    </div>
  </div>
{/if}

<style>
  .busy { display: flex; align-items: center; gap: 10px; }
  .spin { width: 14px; height: 14px; border: 2px solid currentColor; border-right-color: transparent; border-radius: 50%; animation: spin .8s linear infinite; flex: none; opacity: .7; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (prefers-reduced-motion: reduce) { .spin { animation: none; } }
</style>
