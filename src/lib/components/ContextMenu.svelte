<script lang="ts">
  export interface MenuItem { label: string; onclick: () => void; disabled?: boolean; danger?: boolean; sep?: boolean }
  let { x, y, items, onclose }: { x: number; y: number; items: MenuItem[]; onclose: () => void } = $props();
  let el: HTMLDivElement | undefined = $state();
  let pos = $state({ x: 0, y: 0 });
  $effect(() => {
    const w = el?.offsetWidth ?? 200, h = el?.offsetHeight ?? 200;
    pos = { x: Math.min(x, window.innerWidth - w - 6), y: Math.min(y, window.innerHeight - h - 6) };
  });
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onclose()} onblur={onclose} />
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="shield" onmousedown={onclose} oncontextmenu={(e) => { e.preventDefault(); onclose(); }}></div>
<div class="menu" bind:this={el} style="left:{pos.x}px;top:{pos.y}px" role="menu">
  {#each items as it}
    {#if it.sep}<hr />{/if}
    <button role="menuitem" class:danger={it.danger} disabled={it.disabled} onclick={() => { onclose(); it.onclick(); }}>{it.label}</button>
  {/each}
</div>

<style>
  .shield { position: fixed; inset: 0; z-index: 90; }
  .menu {
    position: fixed; z-index: 91; min-width: 190px; padding: 4px; background: var(--bg); border: 1px solid var(--border);
    border-radius: 8px; box-shadow: var(--shadow); display: flex; flex-direction: column;
  }
  button { text-align: left; border: 0; background: none; padding: 5px 10px; border-radius: 5px; }
  button:hover:not(:disabled) { background: var(--accent); color: var(--accent-fg); }
  button:disabled { opacity: 0.4; }
  button.danger { color: var(--warn); }
  button.danger:hover:not(:disabled) { background: var(--warn); color: #fff; }
  hr { border: 0; border-top: 1px solid var(--border); margin: 4px 2px; width: calc(100% - 4px); }
</style>
