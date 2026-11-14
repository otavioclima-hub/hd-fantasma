<script lang="ts">
  // Seta de pasta (13 px dentro de uma área de 20 px). `open` gira 90°; `hidden` só reserva o espaço.
  let { open = false, hidden = false, title, onclick }: { open?: boolean; hidden?: boolean; title?: string; onclick?: (e: MouseEvent) => void } = $props();
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<span class="tw" class:open class:hidden class:clickable={!!onclick} {title} onclick={onclick}><svg viewBox="0 0 16 16" width="13" height="13" aria-hidden="true"><path d="M6 3.5 10.5 8 6 12.5" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg></span>

<style>
  .tw { width: 20px; height: 20px; display: inline-flex; align-items: center; justify-content: center; color: inherit; border-radius: 5px; transition: transform 0.12s, background 0.12s; flex: none; }
  .tw.clickable { cursor: pointer; }
  .tw.clickable:hover { background: rgba(127,127,127,0.22); }
  .tw.open { transform: rotate(90deg); }
  .tw.hidden { visibility: hidden; }
</style>
