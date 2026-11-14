// Estado e ações da tela principal (Svelte 5 runes).
import * as api from "./api";
import { lang, t, type Key } from "./i18n";
import { get } from "svelte/store";
import type { ClaudeStatus, ClaudeTarget, Disk, DuplicateGroup, Item, Kind, ReadOptions, ReadProgress, SearchQuery, Settings } from "./types";

export const PAGE = 500;
const tr = (k: Key, p?: Record<string, string | number>) => get(t)(k, p);

export const S = $state({
  ready: false,
  disks: [] as Disk[],
  selDiskId: null as number | null,

  // navegação: caminho selecionado (disco -> ... -> item selecionado) e pastas abertas na Lista
  path: [] as Item[],
  selItem: null as Item | null,
  expanded: {} as Record<number, boolean>,
  loadingKids: {} as Record<string, boolean>,
  listSort: { key: "name", dir: 1 } as { key: string; dir: 1 | -1 },

  // tabela de resultados da busca
  sort: { key: "name", dir: 1 } as { key: string; dir: 1 | -1 },

  // busca
  text: "",
  kinds: [] as Kind[],
  minVal: "",
  maxVal: "",
  minUnit: "MB" as "MB" | "GB",
  maxUnit: "GB" as "MB" | "GB",
  after: "",
  before: "",
  diskFilter: [] as number[],
  filtersOpen: false,
  results: [] as Item[],
  total: 0,
  searching: false,
  loadingMore: false,

  // telas e janelas
  view: "browse" as "browse" | "dup",
  dupGroups: [] as DuplicateGroup[],
  dupLoading: false,
  dupMin: 0,
  sheetOpen: false,
  settingsOpen: false,
  settings: { language: "pt", includeHidden: false, viewMode: "columns", showInspector: true } as Settings,
  reading: false,
  readName: "",
  progress: null as ReadProgress | null,
  toast: "" as string,
  claude: null as ClaudeStatus | null,
  claudeBusy: false,
  claudeError: "",
  claudeJustDesktop: false,
  dialog: null as null | {
    kind: "confirm" | "prompt";
    title: string;
    text?: string;
    value?: string;
    okLabel: string;
    danger?: boolean;
    resolve: (v: boolean | string | null) => void;
  },
});

// ---------- utilidades ----------
let toastTimer: ReturnType<typeof setTimeout> | undefined;
export function toast(msg: string) {
  S.toast = msg;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (S.toast = ""), 5000);
}
const fail = (e: unknown) => toast(e instanceof Error ? e.message : String(e));

export const askConfirm = (title: string, text: string, okLabel: string, danger = false) =>
  new Promise<boolean>((resolve) => {
    S.dialog = { kind: "confirm", title, text, okLabel, danger, resolve: (v) => resolve(v === true) };
  });
export const askText = (title: string, value: string, okLabel: string) =>
  new Promise<string | null>((resolve) => {
    S.dialog = { kind: "prompt", title, value, okLabel, resolve: (v) => resolve(typeof v === "string" ? v : null) };
  });

export const selDisk = () => S.disks.find((d) => d.id === S.selDiskId) ?? null;
export const diskById = (id: number) => S.disks.find((d) => d.id === id) ?? null;

export const searchActive = () =>
  S.text.trim() !== "" || S.kinds.length > 0 || S.minVal !== "" || S.maxVal !== "" || S.after !== "" || S.before !== "" || S.diskFilter.length > 0;

// ---------- início ----------
export async function init() {
  try {
    S.settings = await api.getSettings();
    lang.set(S.settings.language);
  } catch (e) { fail(e); }
  void refreshClaude(true);
  await loadDisks();
  S.ready = true;
  await api.onReadProgress((p) => { S.progress = p; });
}

// ---------- ligação com o Claude (ADR 0017) ----------
export async function refreshClaude(silent = false) {
  try {
    S.claude = await api.claudeStatus();
    if (!silent) S.claudeError = "";
  } catch (e) { if (!silent) S.claudeError = e instanceof Error ? e.message : String(e); }
}

async function claudeAct(fn: () => Promise<ClaudeStatus>, desktopTouched: boolean) {
  if (S.claudeBusy) return;
  S.claudeBusy = true;
  S.claudeError = "";
  try {
    S.claude = await fn();
    S.claudeJustDesktop = desktopTouched && !!S.claude.desktop.connected;
  } catch (e) {
    S.claudeError = e instanceof Error ? e.message : String(e);
    await refreshClaude(true);
  } finally { S.claudeBusy = false; }
}

export const claudeConnect = (target?: ClaudeTarget) => claudeAct(() => api.claudeConnect(target), target !== "code");
export const claudeDisconnect = (target?: ClaudeTarget) => claudeAct(() => api.claudeDisconnect(target), false);

export async function loadDisks() {
  try {
    S.disks = await api.listDisks();
    if (S.selDiskId != null && !S.disks.some((d) => d.id === S.selDiskId)) clearSelection();
    if (S.selDiskId == null && S.disks.length) await selectDisk(S.disks[0].id);
  } catch (e) { fail(e); }
}

function clearSelection() {
  S.selDiskId = null;
  S.path = [];
  S.selItem = null;
  S.expanded = {};
}

// ---------- cache de pastas (fora do $state profundo: pastas podem ter 50 mil itens) ----------
let kidsRaw = $state.raw<Record<string, Item[]>>({});
const memo = new Map<number, Item>();
const inflight = new Map<string, Promise<Item[]>>();
const kidsKey = (diskId: number, parentId: number | null) => (parentId == null ? `r${diskId}` : String(parentId));

/** Filhos já carregados de uma pasta (undefined = ainda carregando). Reativo. */
export const kidsOf = (diskId: number, parentId: number | null): Item[] | undefined => kidsRaw[kidsKey(diskId, parentId)];
export const remember = (items: Item[]) => { for (const i of items) memo.set(i.id, i); };

export function ensureKids(diskId: number, parentId: number | null): Promise<Item[]> {
  const k = kidsKey(diskId, parentId);
  const have = kidsRaw[k];
  if (have) return Promise.resolve(have);
  let p = inflight.get(k);
  if (!p) {
    S.loadingKids[k] = true;
    p = api.listChildren(diskId, parentId)
      .then((list) => { remember(list); kidsRaw = { ...kidsRaw, [k]: list }; return list; })
      .catch((e) => { fail(e); return [] as Item[]; })
      .finally(() => { inflight.delete(k); S.loadingKids[k] = false; });
    inflight.set(k, p);
  }
  return p;
}

/** Esquece tudo o que foi carregado (depois de ler/remover/importar discos). */
function resetNav() {
  kidsRaw = {};
  memo.clear();
  inflight.clear();
  S.expanded = {};
  S.path = [];
  S.selItem = null;
}

export const isDir = (i: Item) => i.kind === "folder";

/** Pastas acima do item, da raiz até o pai. */
async function ancestorsOf(item: Item): Promise<Item[]> {
  const chain: Item[] = [];
  let pid = item.parentId;
  while (pid != null) {
    let p = memo.get(pid);
    if (!p) { p = await api.getItem(pid); memo.set(p.id, p); }
    chain.unshift(p);
    pid = p.parentId;
  }
  return chain;
}

// ---------- discos e navegação ----------
export async function selectDisk(id: number) {
  if (S.view === "dup") S.view = "browse";
  clearSearchFields();
  S.selDiskId = id;
  S.path = [];
  S.selItem = null;
  S.expanded = {};
  await ensureKids(id, null);
}

let navToken = 0;

/** Seleciona um item (Lista ou busca) e mantém o caminho em dia para a vista de Colunas. */
export async function selectItem(item: Item, opts: { reveal?: boolean } = {}) {
  const tok = ++navToken;
  S.selItem = item;
  if (S.selDiskId !== item.diskId) { S.selDiskId = item.diskId; S.expanded = {}; }
  try {
    const anc = await ancestorsOf(item);
    if (tok !== navToken) return;
    // abre os pais na Lista e carrega o que as Colunas vão mostrar
    await ensureKids(item.diskId, null);
    for (const a of anc) {
      await ensureKids(a.diskId, a.id);
      if (opts.reveal) S.expanded[a.id] = true;
    }
    if (isDir(item)) await ensureKids(item.diskId, item.id);
    if (tok !== navToken) return;
    S.path = [...anc, item];
  } catch (e) { fail(e); }
}

/** Clique numa linha da coluna `col` (vista de Colunas): as colunas à direita são trocadas. */
export function pickInColumn(col: number, item: Item) {
  ++navToken;
  S.path = [...S.path.slice(0, col), item];
  S.selItem = item;
  if (isDir(item)) void ensureKids(item.diskId, item.id);
}

/** Seleciona o disco inteiro (sem item). */
export function selectNothing() {
  ++navToken;
  S.path = [];
  S.selItem = null;
}

export async function toggleExpand(item: Item) {
  if (!isDir(item)) return;
  if (S.expanded[item.id]) { S.expanded[item.id] = false; return; }
  await ensureKids(item.diskId, item.id);
  S.expanded[item.id] = true;
}

/** Clique num resultado da busca: seleciona o item e leva as duas vistas até a pasta dele.
 *  Com `leave`, sai da busca e mostra a vista de navegação. */
export async function gotoResult(item: Item, leave = false) {
  if (S.view === "dup") S.view = "browse";
  if (leave) clearSearchFields();
  await selectItem(item, { reveal: true });
}

async function revealSelectionInList() {
  const anc = S.path.slice(0, -1);
  for (const a of anc) { await ensureKids(a.diskId, a.id); S.expanded[a.id] = true; }
}

export async function setViewMode(mode: "columns" | "list") {
  if (S.view === "dup") S.view = "browse";
  if (S.settings.viewMode === mode) return;
  await saveSettings({ viewMode: mode });
  if (mode === "list") await revealSelectionInList();
}
export const toggleInspector = () => saveSettings({ showInspector: !S.settings.showInspector });

// ---------- busca ----------
export function clearSearchFields() {
  S.text = ""; S.kinds = []; S.minVal = ""; S.maxVal = ""; S.after = ""; S.before = ""; S.diskFilter = [];
  S.results = []; S.total = 0;
}

function toBytes(v: string, unit: "MB" | "GB"): number | null {
  const n = parseFloat(v.replace(",", "."));
  return isFinite(n) && v.trim() !== "" ? Math.round(n * (unit === "GB" ? 1e9 : 1e6)) : null;
}

const buildQuery = (offset: number): SearchQuery => ({
  text: S.text.trim(),
  kinds: [...S.kinds],
  minBytes: toBytes(S.minVal, S.minUnit),
  maxBytes: toBytes(S.maxVal, S.maxUnit),
  modifiedAfter: S.after || null,
  modifiedBefore: S.before || null,
  diskIds: [...S.diskFilter],
  limit: PAGE,
  offset,
});

let searchToken = 0;
export async function runSearch() {
  const tok = ++searchToken;
  if (!searchActive()) { S.results = []; S.total = 0; S.searching = false; return; }
  S.searching = true;
  try {
    const r = await api.search(buildQuery(0));
    if (tok !== searchToken) return;
    remember(r.items);
    S.results = r.items;
    S.total = r.total;
  } catch (e) { if (tok === searchToken) fail(e); } finally { if (tok === searchToken) S.searching = false; }
}

export async function loadMore() {
  if (S.loadingMore || S.searching || S.results.length >= S.total) return;
  const tok = searchToken;
  S.loadingMore = true;
  try {
    const r = await api.search(buildQuery(S.results.length));
    if (tok !== searchToken) return;
    remember(r.items);
    S.results.push(...r.items);
    S.total = r.total;
  } catch (e) { fail(e); } finally { S.loadingMore = false; }
}

// ---------- ações de disco ----------
export async function startRead(mountPath: string, name: string, options: ReadOptions) {
  S.reading = true;
  S.readName = name;
  S.progress = null;
  S.sheetOpen = false;
  try {
    const d = await api.readDisk(mountPath, options);
    await loadDisks();
    toast(tr("toast.readDone", { name: d.name }));
    resetNav();
    S.selDiskId = null;
    await selectDisk(d.id);
  } catch (e) { fail(e); } finally { S.reading = false; }
}

export const cancelRead = () => api.cancelRead().catch(fail);

export async function renameDisk(d: Disk) {
  const name = await askText(tr("rename.title"), d.name, tr("save"));
  if (!name || !name.trim() || name === d.name) return;
  try { await api.renameDisk(d.id, name.trim()); await loadDisks(); toast(tr("toast.renamed")); } catch (e) { fail(e); }
}

export async function removeDisk(d: Disk) {
  const ok = await askConfirm(tr("confirm.removeTitle", { name: d.name }), tr("confirm.removeText"), tr("remove"), true);
  if (!ok) return;
  try { await api.deleteDisk(d.id); resetNav(); await loadDisks(); toast(tr("toast.removed")); if (searchActive()) runSearch(); } catch (e) { fail(e); }
}

export async function exportDisk(d: Disk) {
  try {
    const p = await api.pickSavePath(`${d.name}.hdfantasma`);
    if (!p) return;
    await api.exportDisk(d.id, p);
    toast(tr("toast.exported"));
  } catch (e) { fail(e); }
}

export async function importCatalog() {
  try {
    const p = await api.pickOpenPath();
    if (!p) return;
    const d = await api.importDisk(p);
    resetNav();
    await loadDisks();
    toast(tr("toast.imported"));
    await selectDisk(d.id);
  } catch (e) { fail(e); }
}

export async function revealSelected() {
  if (!S.selItem) return;
  try { await api.revealItem(S.selItem.id); } catch (e) { fail(e); }
}

// ---------- duplicados ----------
export async function loadDuplicates() {
  S.dupLoading = true;
  try { S.dupGroups = await api.findDuplicates([...S.diskFilter], S.dupMin); } catch (e) { fail(e); } finally { S.dupLoading = false; }
}
export async function openDuplicates() { S.view = "dup"; await loadDuplicates(); }

export async function saveSettings(patch: Partial<Settings>) {
  S.settings = { ...S.settings, ...patch };
  lang.set(S.settings.language);
  try { S.settings = await api.setSettings(S.settings); } catch (e) { fail(e); }
}
