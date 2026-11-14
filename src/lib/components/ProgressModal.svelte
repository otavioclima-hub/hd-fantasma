<script lang="ts">
  import { S, cancelRead } from "$lib/store.svelte";
  import { t, locale, type Key } from "$lib/i18n";
  import { formatBytes, formatNumber } from "$lib/format";
  import { Eta, formatRemaining, truncateMiddle, type EtaResult } from "$lib/eta";

  const p = $derived(S.progress);
  const phase = $derived(p?.phase ?? "listing");
  const step = $derived(phase === "saving" || phase === "done" ? 2 : 1);
  const steps: Key[] = ["progress.step1", "progress.step2"];

  const hasBytes = $derived((p?.bytesExpected ?? 0) > 0);
  const listPct = $derived(p && hasBytes ? Math.min(99, (p.bytesSeen / p.bytesExpected) * 100) : 0);
  // Barra determinada só na listagem (com bytes esperados). O resto é animado.
  const determinate = $derived(phase === "listing" && hasBytes);
  const pct = $derived(listPct);

  // Tempo restante: janela de ~20 s, reiniciada a cada fase.
  const eta = new Eta();
  let etaPhase = "";
  let etaRes = $state<EtaResult>({ state: "calculating" });
  $effect(() => {
    const q = S.progress;
    if (!q) return;
    if (q.phase !== etaPhase) { etaPhase = q.phase; eta.reset(); etaRes = { state: "calculating" }; }
    const now = performance.now();
    if (q.phase === "listing" && q.bytesExpected > 0) {
      eta.add(now, q.bytesSeen);
      etaRes = eta.estimate(now, q.bytesExpected - q.bytesSeen, q.inCloud);
    }
  });

  const etaText = $derived.by(() => {
    if (etaRes.state === "calculating") return $t("progress.etaCalc");
    if (etaRes.state === "unknown") return $t("progress.etaNone");
    const f = formatRemaining(etaRes.seconds);
    return f.kind === "less" ? $t("progress.etaLess") : $t("progress.eta", { t: f.text });
  });

  function itemsLong(n: number): string {
    if (n < 100_000) return $t("progress.itemsLong", { n: formatNumber(n, $locale) });
    const txt = new Intl.NumberFormat($locale, { notation: "compact", compactDisplay: "long", maximumFractionDigits: 1 }).format(n);
    return $t(n >= 1_000_000 ? "progress.itemsLongDe" : "progress.itemsLong", { n: txt });
  }

  const mainLine = $derived.by(() => {
    if (!p) return $t("progress.starting");
    if (phase === "listing") {
      const items = itemsLong(p.itemsSeen);
      return hasBytes
        ? `${$t("progress.bytes", { seen: formatBytes(p.bytesSeen, $locale), total: formatBytes(p.bytesExpected, $locale) })} · ${items}`
        : items;
    }
    return $t("progress.saving");
  });

  const showEta = $derived(phase === "listing" && hasBytes);

  let cancelling = $state(false);
  function cancel() {
    cancelling = true;
    cancelRead();
  }
  const path = $derived(truncateMiddle(p?.currentPath ?? ""));
</script>

<div class="backdrop">
  <div class="modal" role="alertdialog" aria-modal="true" aria-label={$t("progress.title", { name: S.readName })} style="width:min(500px,calc(100vw - 32px))">
    <header>{$t("progress.title", { name: S.readName })}</header>
    <div class="body">
      <ol class="steps" aria-label="etapas">
        {#each steps as k, i}
          <li class:cur={step === i + 1} class:done={step > i + 1}>
            <span class="dot">{step > i + 1 ? "✓" : i + 1}</span><span class="lbl">{$t(k)}</span>
          </li>
        {/each}
      </ol>

      {#if determinate}
        <div class="track" role="progressbar" aria-valuemin="0" aria-valuemax="100" aria-valuenow={Math.round(pct)}>
          <div class="fill" style="width:{pct}%"></div>
        </div>
      {:else}
        <div class="track"><div class="ind"></div></div>
      {/if}

      <div class="main">{mainLine}</div>
      <div class="eta muted" class:hidden={!showEta}>{showEta ? etaText : ""}</div>
      {#if p?.inCloud && phase === "listing"}<div class="cloud">☁ {$t("progress.cloud")}</div>{/if}
      <div class="cur muted ellipsis" title={p?.currentPath ?? ""}>{phase === "listing" ? path : ""}</div>
      {#if cancelling}<div class="cancelling">{$t("progress.cancelling")}</div>{/if}
      {#if p?.phase === "error" && p.message}<div class="cur">{p.message}</div>{/if}
    </div>
    <footer><button class="btn" disabled={cancelling} onclick={cancel}>{$t("cancel")}</button></footer>
  </div>
</div>

<style>
  .steps { list-style: none; display: flex; gap: 14px; margin: 0 0 14px; padding: 0; font-size: 12px; color: var(--fg-2); }
  .steps li { display: flex; align-items: center; gap: 6px; }
  .dot { width: 18px; height: 18px; border-radius: 50%; border: 1px solid var(--bg-3); display: inline-flex; align-items: center; justify-content: center; font-size: 11px; }
  .steps li.cur { color: inherit; font-weight: 600; }
  .steps li.cur .dot { background: var(--accent); border-color: var(--accent); color: var(--accent-fg); }
  .steps li.done .dot { background: var(--accent-soft); border-color: var(--accent); }
  .track { height: 6px; border-radius: 3px; background: var(--bg-3); overflow: hidden; margin-bottom: 10px; }
  .fill { height: 100%; border-radius: 3px; background: var(--accent); transition: width 0.25s linear; }
  .ind { width: 35%; height: 100%; border-radius: 3px; background: var(--accent); animation: slide 1.2s ease-in-out infinite; }
  @keyframes slide { 0% { margin-left: -35%; } 100% { margin-left: 100%; } }
  .main { font-weight: 600; margin-bottom: 2px; font-variant-numeric: tabular-nums; }
  .eta { font-size: 12.5px; min-height: 18px; }
  .eta.hidden { visibility: hidden; }
  .cloud { font-size: 12.5px; margin: 4px 0; padding: 4px 8px; border-radius: 6px; background: var(--accent-soft); }
  .cur { font-size: 12px; height: 18px; margin-top: 4px; }
  .cancelling { font-size: 12.5px; margin-top: 6px; }
</style>
