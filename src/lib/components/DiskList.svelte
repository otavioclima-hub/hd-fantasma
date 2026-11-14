<script lang="ts">
  import { S, selectDisk, startRead, renameDisk, exportDisk, removeDisk, importCatalog } from "$lib/store.svelte";
  import { t, locale } from "$lib/i18n";
  import { formatBytes, formatDate, formatNumber } from "$lib/format";
  import type { Disk } from "$lib/types";
  import DiskIcon from "./DiskIcon.svelte";
  import ContextMenu, { type MenuItem } from "./ContextMenu.svelte";

  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);

  function ctx(e: MouseEvent, d: Disk) {
    e.preventDefault();
    const can = d.present && !!d.mountPath;
    const read = (mode: "quick" | "full") => startRead(d.mountPath!, d.name, { mode, includeHidden: S.settings.includeHidden });
    menu = {
      x: e.clientX, y: e.clientY,
      items: [
        { label: $t("menu.quick"), disabled: !can, onclick: () => read("quick") },
        { label: $t("menu.full"), disabled: !can, onclick: () => read("full") },
        { label: $t("menu.rename"), sep: true, onclick: () => renameDisk(d) },
        { label: $t("menu.export"), onclick: () => exportDisk(d) },
        { label: $t("menu.remove"), sep: true, danger: true, onclick: () => removeDisk(d) },
      ],
    };
  }
  const pct = (d: Disk) => (d.capacityBytes > 0 ? Math.min(100, Math.max(0, ((d.capacityBytes - d.freeBytes) / d.capacityBytes) * 100)) : 0);
</script>

<div class="panel">
  <div class="head">{$t("disks")}</div>
  <div class="list" role="listbox" aria-label={$t("disks")}>
    {#each S.disks as d (d.id)}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <div class="disk" class:sel={d.id === S.selDiskId} class:absent={!d.present} role="option" tabindex="0" aria-selected={d.id === S.selDiskId}
           onclick={() => selectDisk(d.id)} oncontextmenu={(e) => ctx(e, d)}
           onkeydown={(e) => (e.key === "Enter" || e.key === " ") && selectDisk(d.id)}>
        <DiskIcon fs={d.filesystem} />
        <div class="info">
          <div class="name">
            <span class="dot" class:on={d.present} title={d.present ? $t("disks.present") : $t("disks.absent")}></span>
            <span class="ellipsis">{d.name}</span>
          </div>
          <div class="bar" title={$t("disks.of", { used: formatBytes(d.capacityBytes - d.freeBytes, $locale), total: formatBytes(d.capacityBytes, $locale) })}>
            <div class="fill" style="width:{pct(d)}%"></div>
          </div>
          <div class="meta muted ellipsis">
            {$t("disks.items", { n: formatNumber(d.itemCount, $locale) })} · {formatBytes(d.capacityBytes, $locale)}
          </div>
          <div class="meta muted ellipsis">{$t("disks.lastRead", { date: formatDate(d.lastReadAt, $locale, false) })}</div>
        </div>
      </div>
    {/each}
  </div>
  <div class="foot">
    <button class="btn" style="width:100%;justify-content:center" onclick={importCatalog}>{$t("disks.import")}</button>
  </div>
</div>
{#if menu}<ContextMenu {...menu} onclose={() => (menu = null)} />{/if}

<style>
  .panel { display: flex; flex-direction: column; height: 100%; background: var(--bg-2); min-width: 0; }
  .head { padding: 8px 12px 5px; font-size: 11px; text-transform: uppercase; letter-spacing: 0.05em; color: var(--fg-2); font-weight: 600; }
  .list { flex: 1; overflow: auto; padding: 0 6px 6px; }
  .disk { display: flex; gap: 9px; padding: 7px 8px; border-radius: 8px; margin-bottom: 2px; align-items: flex-start; }
  .disk:hover { background: var(--bg-3); }
  .disk.sel { background: var(--sel); }
  .disk.absent .info { opacity: 0.82; }
  .info { flex: 1; min-width: 0; }
  .name { display: flex; align-items: center; gap: 6px; font-weight: 600; }
  .dot { width: 8px; height: 8px; border-radius: 50%; background: #9a97a6; flex: none; }
  .dot.on { background: var(--ok); box-shadow: 0 0 0 2px color-mix(in srgb, var(--ok) 25%, transparent); }
  .bar { height: 4px; border-radius: 2px; background: var(--bg-3); margin: 4px 0 3px; overflow: hidden; }
  .disk.sel .bar { background: var(--bg); }
  .fill { height: 100%; background: var(--accent); border-radius: 2px; }
  .meta { font-size: 11px; }
  .foot { padding: 8px; border-top: 1px solid var(--border); }
</style>
