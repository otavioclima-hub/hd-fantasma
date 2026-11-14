// Estado e ações da tela principal (Svelte 5 runes).
import * as api from "./api";
import { lang, t, type Key } from "./i18n";
import { get } from "svelte/store";
import type { Disk, DuplicateGroup, Item, Kind, ReadOptions, ReadProgress, SearchQuery, Settings } from "./types";

export const PAGE = 500;
const tr = (k: Key, p?: Record<string, string | number>) => get(t)(k, p);

export const S = $state({
  ready: false,
  disks: [] as Disk[],
  selDiskId: null as number | null,

  // árvore de pastas do disco selecionado
  rootItems: null as Item[] | null,
  treeChildren: {} as Record<number, Item[]>,
  expanded: {} as Record<number, boolean>,
  treeLoading: {} as Record<number, boolean>,

  // pasta aberta (coluna 3 no modo navegação)
  folderId: null as number | null,
  trail: [] as Item[],
  rows: [] as Item[],
  rowsLoading: false,
  tableChildren: {} as Record<number, Item[]>,
  tableExpanded: {} as Record<number, boolean>,
  sort: { key: "name", dir: 1 } as { key: string; dir: 1 | -1 },
  selItem: null as Item | null,

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
  settings: { language: "pt", includeHidden: false } as Settings,
  reading: false,
  readName: "",
  progress: null as ReadProgress | null,
  toast: "" as string,
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
  await loadDisks();
  S.ready = true;
  await api.onReadProgress((p) => { S.progress = p; });
}

export async function loadDisks() {
  try {
    S.disks = await api.listDisks();
    if (S.selDiskId != null && !S.disks.some((d) => d.id === S.selDiskId)) clearSelection();
    if (S.selDiskId == null && S.disks.length) await selectDisk(S.disks[0].id);
  } catch (e) { fail(e); }
}

function clearSelection() {
  S.selDiskId = null;
  S.rootItems = null;
  S.rows = [];
  S.trail = [];
  S.folderId = null;
  S.selItem = null;
}

// ---------- discos, árvore e pasta ----------
export async function selectDisk(id: number) {
  if (S.selDiskId === id && S.rootItems) { await openFolder(null); return; }
  S.selDiskId = id;
  S.rootItems = null;
  S.treeChildren = {};
  S.expanded = {};
  S.tableChildren = {};
  S.tableExpanded = {};
  S.selItem = null;
  try {
    S.rootItems = await api.listChildren(id, null);
  } catch (e) { fail(e); }
  await openFolder(null);
}

const isDir = (i: Item) => i.kind === "folder";

export async function toggleTree(item: Item) {
  if (S.expanded[item.id]) { S.expanded[item.id] = false; return; }
  await ensureTreeChildren(item);
  S.expanded[item.id] = true;
}

async function ensureTreeChildren(item: Item) {
  if (S.treeChildren[item.id]) return;
  S.treeLoading[item.id] = true;
  try {
    S.treeChildren[item.id] = await api.listChildren(item.diskId, item.id);
  } catch (e) { fail(e); } finally { S.treeLoading[item.id] = false; }
}

/** Abre uma pasta na coluna 3 (null = raiz do disco). */
export async function openFolder(folder: Item | null, opts: { keepSearch?: boolean; select?: Item | null } = {}) {
  if (S.view === "dup") S.view = "browse";
  S.folderId = folder?.id ?? null;
  S.rowsLoading = true;
  S.tableChildren = {};
  S.tableExpanded = {};
  S.selItem = opts.select ?? folder;
  if (!opts.keepSearch) clearSearchFields();
  try {
    // trilha (migalhas): sobe pelos pais
    const trail: Item[] = [];
    let cur = folder;
    while (cur) {
      trail.unshift(cur);
      cur = cur.parentId != null ? await api.getItem(cur.parentId) : null;
    }
    S.trail = trail;
    for (const a of trail.slice(0, -1)) { await ensureTreeChildren(a); S.expanded[a.id] = true; }
    if (folder && isDir(folder)) { await ensureTreeChildren(folder); S.expanded[folder.id] = true; }
    S.rows = S.selDiskId != null ? await api.listChildren(S.selDiskId, folder?.id ?? null) : [];
  } catch (e) { fail(e); } finally { S.rowsLoading = false; }
}

export async function goUp() {
  if (!S.trail.length) return;
  const parent = S.trail.length > 1 ? S.trail[S.trail.length - 2] : null;
  await openFolder(parent, { keepSearch: true });
}

export async function toggleRowArchive(item: Item) {
  if (S.tableExpanded[item.id]) { S.tableExpanded[item.id] = false; return; }
  if (!S.tableChildren[item.id]) {
    try { S.tableChildren[item.id] = await api.listChildren(item.diskId, item.id); } catch (e) { fail(e); return; }
  }
  S.tableExpanded[item.id] = true;
}

export const expandable = (i: Item) => i.kind === "archive" || (i.insideArchive && i.kind === "folder") ? i.childCount > 0 : false;

/** Duplo clique / Enter numa linha. */
export async function activate(item: Item) {
  if (isDir(item) && !item.insideArchive) {
    if (S.selDiskId !== item.diskId) {
      S.selDiskId = item.diskId;
      S.rootItems = await api.listChildren(item.diskId, null).catch((e) => (fail(e), null));
      S.treeChildren = {};
      S.expanded = {};
    }
    await openFolder(item);
  } else if (expandable(item)) {
    await toggleRowArchive(item);
  } else if (searchActive()) {
    await gotoResult(item, true);
  }
}

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
    S.results.push(...r.items);
    S.total = r.total;
  } catch (e) { fail(e); } finally { S.loadingMore = false; }
}

/** Clique num resultado: seleciona o disco e a pasta dele; com `leave`, sai da busca. */
export async function gotoResult(item: Item, leave = false) {
  S.selItem = item;
  if (S.selDiskId !== item.diskId) {
    S.selDiskId = item.diskId;
    S.treeChildren = {};
    S.expanded = {};
    try { S.rootItems = await api.listChildren(item.diskId, null); } catch (e) { fail(e); }
  }
  if (!leave) {
    // só marca disco/pastas na árvore; a lista de resultados continua na coluna 3
    try {
      const chain: Item[] = [];
      let cur: Item | null = item.parentId != null ? await api.getItem(item.parentId) : null;
      while (cur) { chain.unshift(cur); cur = cur.parentId != null ? await api.getItem(cur.parentId) : null; }
      for (const a of chain) { await ensureTreeChildren(a); S.expanded[a.id] = true; }
      S.folderId = chain.length ? chain[chain.length - 1].id : null;
      S.trail = chain;
    } catch (e) { fail(e); }
    return;
  }
  let parent: Item | null = null;
  if (item.parentId != null) { try { parent = await api.getItem(item.parentId); } catch (e) { fail(e); } }
  await openFolder(parent, { select: item });
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
  try { await api.deleteDisk(d.id); await loadDisks(); toast(tr("toast.removed")); if (searchActive()) runSearch(); } catch (e) { fail(e); }
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
