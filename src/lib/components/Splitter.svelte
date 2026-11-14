<script lang="ts">
  let { value = $bindable(), min = 140, max = 520, onchange, reverse = false }: { value: number; min?: number; max?: number; onchange?: () => void; reverse?: boolean } = $props();
  function down(e: PointerEvent) {
    const startX = e.clientX, start = value;
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const move = (ev: PointerEvent) => { const dx = ev.clientX - startX; value = Math.max(min, Math.min(max, start + (reverse ? -dx : dx))); };
    const up = () => { el.removeEventListener("pointermove", move); el.removeEventListener("pointerup", up); onchange?.(); };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
  }
  function key(e: KeyboardEvent) {
    const d = e.key === "ArrowLeft" ? -12 : e.key === "ArrowRight" ? 12 : 0;
    if (d) { value = Math.max(min, Math.min(max, value + (reverse ? -d : d))); onchange?.(); }
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<div class="split" role="separator" aria-orientation="vertical" tabindex="0" onpointerdown={down} onkeydown={key}></div>

<style>
  .split { width: 5px; margin: 0 -2px; cursor: col-resize; flex: none; position: relative; z-index: 5; background: transparent; }
  .split::after { content: ""; position: absolute; left: 2px; top: 0; bottom: 0; width: 1px; background: var(--border); }
  .split:hover::after, .split:active::after { background: var(--accent); width: 2px; left: 1.5px; }
</style>
