// Dados de exemplo para desenvolver a tela num navegador comum (sem Tauri).
import type {
  Disk, DuplicateGroup, Item, Kind, MountedVolume, ReadOptions, ReadProgress, SearchQuery, SearchResult, Settings, VideoInfo,
} from "./types";

const MB = 1e6, GB = 1e9, TB = 1e12;

interface N {
  n: string;
  size?: number;
  kids?: N[];
  zip?: boolean;
  v?: VideoInfo;
  mod?: string;
}

const D = (n: string, kids: N[]): N => ({ n, kids });
const F = (n: string, size: number, mod?: string): N => ({ n, size, mod });
const V = (n: string, size: number, durationSec: number, width: number, height: number, codec: string, fps = 24, mod?: string): N => ({
  n, size, mod, v: { durationSec, width, height, codec, fps },
});
const Z = (n: string, size: number, kids: N[]): N => ({ n, size, kids, zip: true });

const EXT_KIND: Record<string, Kind> = {
  mov: "video", mp4: "video", mxf: "video", mkv: "video", braw: "video", r3d: "video",
  jpg: "image", png: "image", psd: "image", tif: "image", cr3: "image", dng: "image",
  wav: "audio", mp3: "audio", aif: "audio", flac: "audio",
  pdf: "document", docx: "document", xlsx: "document", txt: "document", aep: "document", prproj: "document",
  zip: "archive", rar: "archive", "7z": "archive",
};

let seed = 42;
const rnd = () => ((seed = (seed * 1664525 + 1013904223) >>> 0) / 4294967296);

function rushes(prefix: string, count: number): N[] {
  const out: N[] = [];
  for (let i = 1; i <= count; i++) {
    const clip = String(i).padStart(5, "0");
    out.push(V(`${prefix}_C${clip}.mov`, Math.round((5 + rnd() * 60) * MB), Math.round(5 + rnd() * 240), 3840, 2160, "prores", 23.976,
      `2024-0${1 + Math.floor(rnd() * 8)}-${String(1 + Math.floor(rnd() * 27)).padStart(2, "0")}T1${Math.floor(rnd() * 9)}:2${Math.floor(rnd() * 9)}:00Z`));
  }
  return out;
}

const SPECS: { disk: Omit<Disk, "id" | "itemCount" | "totalBytes">; tree: N[] }[] = [
  {
    disk: {
      uuid: "7B1A-4E2C", name: "BACKUP 2024", filesystem: "exfat", capacityBytes: 4 * TB, freeBytes: 1.3 * TB,
      lastReadAt: "2026-11-10T21:14:00Z", present: true, mountPath: "/Volumes/BACKUP 2024",
    },
    tree: [
      D("Projetos", [
        D("Vita_Comercial_Verao", [
          D("Footage", [
            D("Rushes_Dia_01", rushes("A001", 20000)),
            D("Rushes_Dia_02", rushes("B002", 60)),
            V("Entrevista_Marina_A001.mov", 12.3 * GB, 1260, 3840, 2160, "prores", 23.976, "2024-03-12T15:02:00Z"),
          ]),
          D("Projeto", [
            F("Vita_Verao_v3.aep", 84 * MB, "2024-04-02T18:00:00Z"),
            F("Vita_Verao_v4.aep", 91 * MB, "2024-04-09T11:30:00Z"),
            F("Vita_Verao_v5.prproj", 22 * MB, "2024-04-11T09:00:00Z"),
          ]),
          D("Exports", [
            V("Vita_Verao_FINAL_16x9.mov", 18.4 * GB, 62, 3840, 2160, "prores", 24, "2024-04-12T20:00:00Z"),
            V("Vita_Verao_FINAL_9x16.mp4", 410 * MB, 62, 1080, 1920, "h264", 30, "2024-04-12T20:40:00Z"),
          ]),
          Z("Entrega_Cliente.zip", 2.1 * GB, [
            V("Vita_Verao_FINAL_16x9.mp4", 1.2 * GB, 62, 3840, 2160, "hevc", 24),
            D("Legendas", [F("pt-BR.srt", 4000), F("en.srt", 3900)]),
            F("Briefing.pdf", 1.2 * MB),
          ]),
        ]),
        D("Banco_Alimentos_Institucional", [
          V("Depoimento_01.mov", 8.8 * GB, 540, 3840, 2160, "prores", 23.976, "2024-06-20T14:00:00Z"),
          V("Depoimento_02.mov", 9.1 * GB, 585, 3840, 2160, "prores", 23.976, "2024-06-20T16:30:00Z"),
          D("Trilha", [F("Tema_Principal.wav", 62 * MB), F("Tema_Principal_instrumental.wav", 58 * MB), F("Ambiente_Rua.aif", 120 * MB)]),
        ]),
      ]),
      D("Fotos", [
        D("Making_Of", [F("IMG_0001.cr3", 31 * MB), F("IMG_0002.cr3", 30 * MB), F("IMG_0003.dng", 54 * MB), F("capa.psd", 410 * MB)]),
      ]),
      D("Documentos", [F("Contrato_Vita.pdf", 2.3 * MB), F("Orcamento_2024.xlsx", 340000), F("Notas.txt", 1200)]),
      Z("Arquivo_Antigo.zip", 6.4 * GB, [D("2019", [F("clipe_a.mov", 2 * GB), F("clipe_b.mov", 3 * GB)]), F("leia-me.txt", 900)]),
    ],
  },
  {
    disk: {
      uuid: "C0FFEE00-1111-4B7E-9A6D-0A5F7E2B3C10", name: "Untitled", filesystem: "apfs", capacityBytes: 2 * TB, freeBytes: 0.4 * TB,
      lastReadAt: "2026-09-02T10:05:00Z", present: false, mountPath: null,
    },
    tree: [
      D("Movies", [
        V("Entrevista_Marina_A001.mov", 12.3 * GB, 1260, 3840, 2160, "prores", 23.976, "2024-03-12T15:02:00Z"),
        V("Viagem_Lisboa.mp4", 3.2 * GB, 840, 3840, 2160, "hevc", 30, "2025-05-01T12:00:00Z"),
        V("Teste_Camera.mov", 5.6 * GB, 95, 4096, 2160, "prores", 24, "2025-01-18T09:00:00Z"),
      ]),
      D("Fotos", [F("IMG_0001.cr3", 31 * MB), F("IMG_0002.cr3", 30 * MB), D("Lisboa", [F("DSC_1001.jpg", 8 * MB), F("DSC_1002.jpg", 9 * MB)])]),
      D("Documentos", [F("Notas.txt", 1200), F("Curriculo.pdf", 480000)]),
      Z("Projetos_Antigos.zip", 4 * GB, [F("a.aep", 40 * MB), F("b.aep", 52 * MB)]),
    ],
  },
  {
    disk: {
      uuid: "NAS-ESTUDIO-0001", name: "NAS Estúdio", filesystem: "smbfs", capacityBytes: 16 * TB, freeBytes: 5.2 * TB,
      lastReadAt: "2026-10-21T18:40:00Z", present: false, mountPath: null,
    },
    tree: [
      D("Institucional", [
        D("Banco_Alimentos", [
          V("Depoimento_01.mov", 8.8 * GB, 540, 3840, 2160, "prores", 23.976, "2024-06-20T14:00:00Z"),
          V("Entrevista_Marina_A001.mov", 12.3 * GB, 1260, 3840, 2160, "prores", 23.976, "2024-03-12T15:02:00Z"),
        ]),
        D("Vita", [F("Vita_Verao_v5.prproj", 22 * MB, "2024-04-11T09:00:00Z"), F("Briefing.pdf", 1.2 * MB)]),
      ]),
      D("Motion", [
        D("Templates", [F("Lower_Third_01.aep", 12 * MB), F("Lower_Third_02.aep", 14 * MB), F("Transicoes.aep", 38 * MB)]),
        D("Renders", [
          V("Abertura_Canal_4K.mov", 2.8 * GB, 12, 3840, 2160, "prores", 30, "2025-08-03T10:00:00Z"),
          V("Abertura_Canal_HD.mp4", 90 * MB, 12, 1920, 1080, "h264", 30, "2025-08-03T10:20:00Z"),
        ]),
      ]),
      D("Audio", [F("Tema_Principal.wav", 62 * MB), F("Locucao_01.wav", 88 * MB), F("Locucao_02.mp3", 9 * MB)]),
    ],
  },
];

// ---------- montagem do catálogo ----------
let nextId = 1;
const items: Item[] = [];
const byId = new Map<number, Item>();
const childMap = new Map<string, Item[]>();
const disks: Disk[] = [];
const key = (d: number, p: number | null) => `${d}:${p ?? "r"}`;

function extOf(name: string) {
  const i = name.lastIndexOf(".");
  return i > 0 ? name.slice(i + 1).toLowerCase() : null;
}

function addNodes(diskId: number, parent: Item | null, nodes: N[], inArchive: boolean): number {
  let sum = 0;
  const list: Item[] = [];
  childMap.set(key(diskId, parent?.id ?? null), list);
  for (const n of nodes) {
    const ext = n.kids ? (n.zip ? extOf(n.n) : null) : extOf(n.n);
    const kind: Kind = n.zip ? "archive" : n.kids ? "folder" : EXT_KIND[ext ?? ""] ?? "other";
    const it: Item = {
      id: nextId++, diskId, parentId: parent?.id ?? null, name: n.n,
      path: parent ? `${parent.path}/${n.n}` : n.n,
      kind, ext, sizeBytes: n.size ?? 0,
      createdAt: n.mod ?? "2024-01-15T10:00:00Z", modifiedAt: n.mod ?? "2024-01-15T10:00:00Z",
      insideArchive: inArchive, childCount: n.kids?.length ?? 0, video: n.v ?? null,
    };
    items.push(it);
    byId.set(it.id, it);
    list.push(it);
    if (n.kids) {
      const s = addNodes(diskId, it, n.kids, inArchive || !!n.zip);
      if (!n.zip) it.sizeBytes = s;
    }
    if (!inArchive) sum += it.sizeBytes;
  }
  list.sort(cmpItems);
  return sum;
}

function cmpItems(a: Item, b: Item) {
  const fa = a.kind === "folder" ? 0 : 1, fb = b.kind === "folder" ? 0 : 1;
  return fa - fb || a.name.localeCompare(b.name, "pt-BR", { numeric: true });
}

function buildDisk(spec: Omit<Disk, "id" | "itemCount" | "totalBytes">, tree: N[]): Disk {
  const id = disks.length + 1;
  const before = items.length;
  const total = addNodes(id, null, tree, false);
  const d: Disk = { ...spec, id, itemCount: items.length - before, totalBytes: total };
  disks.push(d);
  return d;
}

for (const s of SPECS) buildDisk(s.disk, s.tree);

// ---------- API falsa ----------
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
let settings: Settings = { language: "pt", includeHidden: false };
const progressListeners = new Set<(p: ReadProgress) => void>();
let cancelled = false;

export const onProgress = (cb: (p: ReadProgress) => void) => {
  progressListeners.add(cb);
  return () => progressListeners.delete(cb);
};
const emit = (p: ReadProgress) => progressListeners.forEach((l) => l(p));

export const mock = {
  async list_disks(): Promise<Disk[]> {
    await sleep(80);
    return disks.map((d) => ({ ...d }));
  },
  async list_mounted_volumes(): Promise<MountedVolume[]> {
    await sleep(150);
    const b = disks[0];
    return [
      { uuid: b.uuid, name: b.name, mountPath: b.mountPath!, filesystem: b.filesystem, capacityBytes: b.capacityBytes, freeBytes: b.freeBytes, catalogedDiskId: b.id },
      { uuid: "A1B2-C3D4", name: "CARTÃO SD 128", mountPath: "/Volumes/CARTAO SD", filesystem: "exfat", capacityBytes: 128 * GB, freeBytes: 31 * GB, catalogedDiskId: null },
      { uuid: "MAC-INTERNAL", name: "Macintosh HD", mountPath: "/", filesystem: "apfs", capacityBytes: 1 * TB, freeBytes: 0.3 * TB, catalogedDiskId: null },
    ];
  },
  async read_disk(a: { mountPath: string; options: ReadOptions }): Promise<Disk> {
    cancelled = false;
    const vol = (await mock.list_mounted_volumes()).find((v) => v.mountPath === a.mountPath);
    if (!vol) throw "O disco não está plugado.";
    const uuid = vol.uuid;
    const paths = ["Projetos/Cliente", "Fotos/2025", "Documentos", "Videos/Rushes", "Audio/Trilhas"];
    let seen = 0;
    emit({ diskUuid: uuid, phase: "listing", itemsSeen: 0, currentPath: null, message: null });
    for (let i = 0; i < 24; i++) {
      await sleep(90);
      if (cancelled) { emit({ diskUuid: uuid, phase: "error", itemsSeen: seen, currentPath: null, message: "Leitura cancelada." }); throw "Leitura cancelada."; }
      seen += 380 + Math.floor(Math.random() * 400);
      emit({ diskUuid: uuid, phase: i < 14 ? "listing" : "details", itemsSeen: seen, currentPath: `${paths[i % paths.length]}/arquivo_${seen}.mov`, message: null });
    }
    emit({ diskUuid: uuid, phase: "saving", itemsSeen: seen, currentPath: null, message: null });
    await sleep(500);
    let d = disks.find((x) => x.uuid === uuid);
    if (d) {
      d.lastReadAt = new Date().toISOString();
      d.present = true;
      d.mountPath = vol.mountPath;
    } else {
      d = buildDisk(
        { uuid, name: vol.name, filesystem: vol.filesystem, capacityBytes: vol.capacityBytes, freeBytes: vol.freeBytes, lastReadAt: new Date().toISOString(), present: true, mountPath: vol.mountPath },
        [D("DCIM", [V("MVI_0001.mp4", 1.1 * GB, 180, 3840, 2160, "h264", 30), F("IMG_5001.jpg", 6 * MB)]), F("LEIAME.txt", 800)],
      );
    }
    emit({ diskUuid: uuid, phase: "done", itemsSeen: seen, currentPath: null, message: null });
    return { ...d };
  },
  async cancel_read(): Promise<void> { cancelled = true; },
  async list_children(a: { diskId: number; parentId: number | null }): Promise<Item[]> {
    await sleep(a.parentId == null ? 40 : 90);
    return (childMap.get(key(a.diskId, a.parentId)) ?? []).slice();
  },
  async get_item(a: { id: number }): Promise<Item> {
    const it = byId.get(a.id);
    if (!it) throw "Item não encontrado.";
    return it;
  },
  async search({ query: q }: { query: SearchQuery }): Promise<SearchResult> {
    await sleep(120);
    const terms = q.text.toLowerCase().split(/\s+/).filter(Boolean);
    const after = q.modifiedAfter ? Date.parse(q.modifiedAfter) : null;
    const before = q.modifiedBefore ? Date.parse(q.modifiedBefore) + 86400000 : null;
    const hit = items.filter((it) => {
      if (q.diskIds.length && !q.diskIds.includes(it.diskId)) return false;
      if (q.kinds.length && !q.kinds.includes(it.kind)) return false;
      if (q.minBytes != null && it.sizeBytes < q.minBytes) return false;
      if (q.maxBytes != null && it.sizeBytes > q.maxBytes) return false;
      const m = it.modifiedAt ? Date.parse(it.modifiedAt) : null;
      if (after != null && (m == null || m < after)) return false;
      if (before != null && (m == null || m > before)) return false;
      if (terms.length) {
        const hay = it.path.toLowerCase();
        if (!terms.every((t) => hay.includes(t))) return false;
      }
      return true;
    });
    hit.sort((a, b) => a.name.localeCompare(b.name, "pt-BR", { numeric: true }));
    return { items: hit.slice(q.offset, q.offset + q.limit), total: hit.length };
  },
  async find_duplicates(a: { diskIds: number[]; minBytes: number }): Promise<DuplicateGroup[]> {
    await sleep(200);
    const m = new Map<string, Item[]>();
    for (const it of items) {
      if (it.kind === "folder" || it.insideArchive || it.sizeBytes < a.minBytes || it.sizeBytes <= 0) continue;
      if (a.diskIds.length && !a.diskIds.includes(it.diskId)) continue;
      const k = `${it.name}|${it.sizeBytes}`;
      (m.get(k) ?? m.set(k, []).get(k)!).push(it);
    }
    return [...m.values()]
      .filter((g) => g.length > 1)
      .map((g) => ({ name: g[0].name, sizeBytes: g[0].sizeBytes, items: g }))
      .sort((x, y) => y.sizeBytes * y.items.length - x.sizeBytes * x.items.length);
  },
  async rename_disk(a: { diskId: number; name: string }) {
    const d = disks.find((x) => x.id === a.diskId);
    if (d) d.name = a.name;
  },
  async delete_disk(a: { diskId: number }) {
    const i = disks.findIndex((x) => x.id === a.diskId);
    if (i >= 0) disks.splice(i, 1);
  },
  async export_disk(_a: { diskId: number; destPath: string }) { await sleep(300); },
  async import_disk(_a: { srcPath: string }): Promise<Disk> {
    await sleep(300);
    return buildDisk(
      { uuid: "IMPORTED-" + Date.now(), name: "Disco importado", filesystem: "hfs", capacityBytes: 1 * TB, freeBytes: 0.5 * TB, lastReadAt: new Date().toISOString(), present: false, mountPath: null },
      [D("Importado", [F("exemplo.pdf", 1 * MB)])],
    );
  },
  async reveal_item(_a: { id: number }) { /* nada no navegador */ },
  async get_settings(): Promise<Settings> { return { ...settings }; },
  async set_settings(a: { settings: Settings }): Promise<Settings> { settings = { ...a.settings }; return { ...settings }; },
};
