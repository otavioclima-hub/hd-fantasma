<script lang="ts">
  import { tick } from "svelte";
  import { S, kidsOf, ensureKids, pickInColumn, selectNothing, isDir } from "$lib/store.svelte";
  import { t } from "$lib/i18n";
  import type { Item } from "$lib/types";
  import Column from "./Column.svelte";

  let box: HTMLDivElement | undefined = $state();

  interface Col { key: string; items: Item[] | undefined; selId: number | undefined }
  const cols = $derived.by<Col[]>(() => {
    const d = S.selDiskId;
    if (d == null) return [];
    const out: Col[] = [{ key: "root", items: kidsOf(d, null), selId: S.path[0]?.id }];
    for (let i = 0; i < S.path.length; i++) {
      const p = S.path[i];
      if (!isDir(p)) break;
      out.push({ key: String(p.id), items: kidsOf(d, p.id), selId: S.path[i + 1]?.id });
    }
    return out;
  });
  const activeCol = $derived(Math.max(0, S.path.length - 1));

  $effect(() => { if (S.selDiskId != null) void ensureKids(S.selDiskId, null); });

  // a coluna mais nova sempre fica à vista
  $effect(() => {
    void cols.length;
    void S.path.length;
    tick().then(() => box?.scrollTo({ left: box.scrollWidth, behavior: "smooth" }));
  });

  function onkey(e: KeyboardEvent) {
    if (e.metaKey || e.ctrlKey || e.altKey || !cols.length) return;
    const col = cols[activeCol];
    const items = col?.items ?? [];
    const idx = S.path.length ? items.findIndex((x) => x.id === S.path[S.path.length - 1].id) : -1;
    const pick = (i: number) => { const it = items[Math.max(0, Math.min(items.length - 1, i))]; if (it) pickInColumn(activeCol, it); };
    switch (e.key) {
      case "ArrowDown": e.preventDefault(); pick(idx < 0 ? 0 : idx + 1); break;
      case "ArrowUp": e.preventDefault(); pick(idx < 0 ? items.length - 1 : idx - 1); break;
      case "Home": e.preventDefault(); pick(0); break;
      case "End": e.preventDefault(); pick(items.length - 1); break;
      case "PageDown": e.preventDefault(); pick((idx < 0 ? 0 : idx) + 15); break;
      case "PageUp": e.preventDefault(); pick((idx < 0 ? 0 : idx) - 15); break;
      case "ArrowRight":
      case "Enter": {
        e.preventDefault();
        if (!S.path.length) { pick(0); break; }
        const cur = S.path[S.path.length - 1];
        const next = isDir(cur) ? cols[S.path.length]?.items : undefined;
        if (next?.length) pickInColumn(S.path.length, next[0]);
        break;
      }
      case "ArrowLeft":
      case "Backspace": {
        e.preventDefault();
        if (S.path.length > 1) { S.path = S.path.slice(0, -1); S.selItem = S.path[S.path.length - 1]; }
        else if (S.path.length === 1) selectNothing();
        break;
      }
    }
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<div class="cols" bind:this={box} tabindex="0" role="group" aria-label={$t("view.columns")} onkeydown={onkey}
     onpointerdown={(e) => { if (e.target === e.currentTarget) box?.focus(); }}>
  {#if S.selDiskId == null}
    <p class="muted pad">{$t("pickDisk")}</p>
  {:else}
    {#each cols as c, i (c.key)}
      <Column items={c.items} selectedId={c.selId} active={i === activeCol} onpick={(it) => { pickInColumn(i, it); box?.focus(); }} />
    {/each}
    <div class="fill"></div>
  {/if}
</div>

<style>
  .cols { display: flex; height: 100%; overflow-x: auto; overflow-y: hidden; outline: none; background: var(--bg); }
  .fill { flex: 1 0 24px; }
  .pad { padding: 14px 16px; margin: 0; }
</style>
