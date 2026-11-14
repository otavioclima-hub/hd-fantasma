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
        {#if d.altLabel}<button class="btn" onclick={() => done("alt")}>{d.altLabel}</button>{/if}
        <button class="btn" class:danger={d.danger} class:primary={!d.danger}
                onclick={() => done(d.kind === "confirm" ? true : value)}>{d.okLabel}</button>
      </footer>
    </div>
  </div>
{/if}
