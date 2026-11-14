<script lang="ts">
  let { value = $bindable(), min = 140, max = 520, onchange }: { value: number; min?: number; max?: number; onchange?: () => void } = $props();
  function down(e: PointerEvent) {
    const startX = e.clientX, start = value;
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const move = (ev: PointerEvent) => { value = Math.max(min, Math.min(max, start + ev.clientX - startX)); };
    const up = () => { el.removeEventListener("pointermove", move); el.removeEventListener("pointerup", up); onchange?.(); };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
  }
  function key(e: KeyboardEvent) {
    if (e.key === "ArrowLeft") { value = Math.max(min, value - 12); onchange?.(); }
    if (e.key === "ArrowRight") { value = Math.min(max, value + 12); onchange?.(); }
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<div class="split" role="separator" aria-orientation="vertical" tabindex="0" onpointerdown={down} onkeydown={key}></div>

<style>
  .split { width: 5px; margin: 0 -2px; cursor: col-resize; flex: none; position: relative; z-index: 5; background: transparent; }
  .split::after { content: ""; position: absolute; left: 2px; top: 0; bottom: 0; width: 1px; background: var(--border); }
  .split:hover::after, .split:active::after { background: var(--accent); width: 2px; left: 1.5px; }
</style>
