// Camada fina sobre os comandos Tauri (docs/CONTRATO.md).
// Fora do Tauri (navegador comum), usa dados falsos de src/lib/mock.ts.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { mock, onProgress as mockOnProgress } from "./mock";
import type {
  ClaudeStatus, ClaudeTarget, Disk, DuplicateGroup, Item, MountedVolume, ReadOptions, ReadProgress, SearchQuery, SearchResult, Settings,
} from "./types";

export const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
export const isMock = !isTauri;

async function call<T>(cmd: string, args: Record<string, unknown> | undefined, fake: () => Promise<T>): Promise<T> {
  if (!isTauri) return fake();
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    throw new Error(typeof e === "string" ? e : (e as Error)?.message ?? String(e));
  }
}

const m = mock;
export const listDisks = () => call<Disk[]>("list_disks", undefined, () => m.list_disks());
export const listMountedVolumes = () => call<MountedVolume[]>("list_mounted_volumes", undefined, () => m.list_mounted_volumes());
export const readDisk = (mountPath: string, options: ReadOptions) =>
  call<Disk>("read_disk", { mountPath, options }, () => m.read_disk({ mountPath, options }));
export const cancelRead = () => call<void>("cancel_read", undefined, () => m.cancel_read());
export const listChildren = (diskId: number, parentId: number | null) =>
  call<Item[]>("list_children", { diskId, parentId }, () => m.list_children({ diskId, parentId }));
export const getItem = (id: number) => call<Item>("get_item", { id }, () => m.get_item({ id }));
export const search = (query: SearchQuery) => call<SearchResult>("search", { query }, () => m.search({ query }));
export const findDuplicates = (diskIds: number[], minBytes: number) =>
  call<DuplicateGroup[]>("find_duplicates", { diskIds, minBytes }, () => m.find_duplicates({ diskIds, minBytes }));
export const renameDisk = (diskId: number, name: string) => call<void>("rename_disk", { diskId, name }, () => m.rename_disk({ diskId, name }));
export const deleteDisk = (diskId: number) => call<void>("delete_disk", { diskId }, () => m.delete_disk({ diskId }));
export const exportDisk = (diskId: number, destPath: string) => call<void>("export_disk", { diskId, destPath }, () => m.export_disk({ diskId, destPath }));
export const importDisk = (srcPath: string) => call<Disk>("import_disk", { srcPath }, () => m.import_disk({ srcPath }));
export const revealItem = (id: number) => call<void>("reveal_item", { id }, () => m.reveal_item({ id }));
export const getSettings = () => call<Settings>("get_settings", undefined, () => m.get_settings());
export const setSettings = (settings: Settings) => call<Settings>("set_settings", { settings }, () => m.set_settings({ settings }));

export const claudeStatus = () => call<ClaudeStatus>("claude_status", undefined, () => m.claude_status());
export const claudeConnect = (target?: ClaudeTarget) =>
  call<ClaudeStatus>("claude_connect", { target: target ?? null }, () => m.claude_connect({ target }));
export const claudeDisconnect = (target?: ClaudeTarget) =>
  call<ClaudeStatus>("claude_disconnect", { target: target ?? null }, () => m.claude_disconnect({ target }));

/** Escuta o evento "read-progress". Devolve a função para parar de escutar. */
export async function onReadProgress(cb: (p: ReadProgress) => void): Promise<() => void> {
  if (!isTauri) return mockOnProgress(cb);
  const un = await listen<ReadProgress>("read-progress", (e) => cb(e.payload));
  return un;
}

// ---- diálogos de arquivo (plugin-dialog; falsos no navegador) ----
export async function pickSavePath(defaultName: string): Promise<string | null> {
  if (!isTauri) return `/mock/${defaultName}`;
  const { save } = await import("@tauri-apps/plugin-dialog");
  return save({ defaultPath: defaultName, filters: [{ name: "HD Fantasma", extensions: ["hdfantasma"] }] });
}

export async function pickOpenPath(): Promise<string | null> {
  if (!isTauri) return "/mock/catalogo.hdfantasma";
  const { open } = await import("@tauri-apps/plugin-dialog");
  const r = await open({ multiple: false, directory: false, filters: [{ name: "HD Fantasma", extensions: ["hdfantasma"] }] });
  return typeof r === "string" ? r : null;
}
