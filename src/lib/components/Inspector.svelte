<script lang="ts">
  import { S, diskById, selDisk, revealSelected, toast } from "$lib/store.svelte";
  import { t, locale, type Key } from "$lib/i18n";
  import { formatBytes, formatDate, formatNumber } from "$lib/format";
  import type { Item } from "$lib/types";
  import KindIcon from "./KindIcon.svelte";
  import DiskIcon from "./DiskIcon.svelte";

  const isWin = typeof navigator !== "undefined" && /Win/i.test(navigator.userAgent || "");
  const it = $derived(S.selItem);
  const disk = $derived(it ? diskById(it.diskId) : selDisk());

  const kindLabel = (i: Item) => {
    const k = $t(`kind.${i.kind}` as Key);
    return i.kind !== "folder" && i.ext ? `${k} · ${i.ext.toUpperCase()}` : k;
  };
  const sizeFull = (n: number) => $t("insp.sizeValue", { bytes: formatNumber(n, $locale), human: formatBytes(n, $locale, true) });
  const fullPath = (i: Item) => {
    const d = diskById(i.diskId);
    return d?.present && d.mountPath ? `${d.mountPath.replace(/\/$/, "")}/${i.path}` : `${d?.name ?? ""}/${i.path}`;
  };

  async function copyPath() {
    if (!it) return;
    try { await navigator.clipboard.writeText(fullPath(it)); toast($t("toast.pathCopied")); } catch { /* sem permissão de área de transferência */ }
  }
</script>

<aside class="insp" aria-label={$t("insp.title")}>
  {#if it && disk}
    <header>
      <div class="big"><KindIcon kind={it.kind} size={48} /></div>
      <div class="nm">{it.name}</div>
    </header>
    <div class="body">
      <h4>{$t("insp.general")}</h4>
      <dl>
        <dt>{$t("insp.name")}</dt><dd>{it.name}</dd>
        <dt>{$t("insp.kind")}</dt><dd>{kindLabel(it)}</dd>
        <dt>{$t("insp.size")}</dt><dd>{sizeFull(it.sizeBytes)}</dd>
        {#if it.kind === "folder"}<dt>{$t("insp.inside")}</dt><dd>{formatNumber(it.descendantCount, $locale)}</dd>{/if}
        <dt>{$t("insp.path")}</dt><dd class="wrapany">{disk.name}{it.path ? " / " : ""}{it.path.split("/").join(" / ")}</dd>
        <dt>{$t("insp.disk")}</dt><dd><span class="dot" class:on={disk.present}></span>{disk.name} · {disk.present ? $t("insp.present") : $t("insp.absent")}</dd>
        <dt>{$t("insp.created")}</dt><dd>{formatDate(it.createdAt, $locale)}</dd>
        <dt>{$t("insp.modified")}</dt><dd>{formatDate(it.modifiedAt, $locale)}</dd>
        <dt>{$t("insp.readAt")}</dt><dd>{formatDate(disk.lastReadAt, $locale)}</dd>
      </dl>
    </div>
    <footer>
      <button class="btn" disabled={!disk.present} title={disk.present ? "" : $t("plugDisk", { name: disk.name })} onclick={revealSelected}>{isWin ? $t("reveal.win") : $t("reveal")}</button>
      <button class="btn" onclick={copyPath}>{$t("copyPath")}</button>
    </footer>
  {:else if disk}
    <header>
      <div class="big"><DiskIcon fs={disk.filesystem} size={48} /></div>
      <div class="nm">{disk.name}</div>
    </header>
    <div class="body">
      <h4>{$t("insp.general")}</h4>
      <dl>
        <dt>{$t("insp.name")}</dt><dd>{disk.name}</dd>
        <dt>{$t("insp.kind")}</dt><dd>{$t("insp.diskType")}</dd>
        <dt>{$t("insp.capacity")}</dt><dd>{sizeFull(disk.capacityBytes)}</dd>
        <dt>{$t("insp.used")}</dt><dd>{formatBytes(Math.max(0, disk.capacityBytes - disk.freeBytes), $locale, true)}</dd>
        <dt>{$t("insp.free")}</dt><dd>{formatBytes(disk.freeBytes, $locale, true)}</dd>
        <dt>{$t("insp.fs")}</dt><dd>{disk.filesystem || "—"}</dd>
        <dt>{$t("insp.items")}</dt><dd>{formatNumber(disk.itemCount, $locale)}</dd>
        <dt>{$t("insp.status")}</dt><dd><span class="dot" class:on={disk.present}></span>{disk.present ? $t("insp.present") : $t("insp.absent")}</dd>
        {#if disk.present && disk.mountPath}<dt>{$t("insp.mount")}</dt><dd class="wrapany">{disk.mountPath}</dd>{/if}
        <dt>{$t("insp.readAt")}</dt><dd>{formatDate(disk.lastReadAt, $locale)}</dd>
      </dl>
    </div>
  {:else}
    <p class="muted pad">{$t("details.none")}</p>
  {/if}
</aside>

<style>
  .insp { display: flex; flex-direction: column; height: 100%; background: var(--bg-2); min-width: 0; }
  header { padding: 18px 14px 10px; text-align: center; border-bottom: 1px solid var(--border); }
  .big { display: flex; justify-content: center; margin-bottom: 8px; }
  .nm { font-weight: 600; font-size: 14px; overflow-wrap: anywhere; user-select: text; }
  .body { flex: 1; overflow: auto; padding: 6px 14px 12px; }
  h4 { margin: 10px 0 6px; font-size: 11px; text-transform: uppercase; letter-spacing: 0.05em; color: var(--fg-2); font-weight: 600; }
  dl { margin: 0; display: grid; grid-template-columns: 92px 1fr; gap: 6px 10px; }
  dt { color: var(--fg-2); text-align: right; }
  dd { margin: 0; min-width: 0; overflow-wrap: anywhere; user-select: text; }
  .dot { display: inline-block; width: 8px; height: 8px; border-radius: 50%; background: #9a97a6; margin-right: 6px; }
  .dot.on { background: var(--ok); box-shadow: 0 0 0 2px color-mix(in srgb, var(--ok) 25%, transparent); }
  footer { padding: 10px 14px; border-top: 1px solid var(--border); display: flex; flex-wrap: wrap; gap: 8px; }
  footer .btn { flex: 1; justify-content: center; }
  .pad { padding: 14px; margin: 0; }
</style>
