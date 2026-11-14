// Tipos do contrato motor <-> telas (docs/CONTRATO.md).
export type Kind = "folder" | "video" | "image" | "audio" | "document" | "archive" | "other";

export interface Disk {
  id: number;
  uuid: string;
  name: string;
  filesystem: string;
  capacityBytes: number;
  freeBytes: number;
  itemCount: number;
  totalBytes: number;
  lastReadAt: string;
  present: boolean;
  mountPath: string | null;
}

export interface Item {
  id: number;
  diskId: number;
  parentId: number | null;
  name: string;
  path: string;
  kind: Kind;
  ext: string | null;
  sizeBytes: number;
  createdAt: string | null;
  modifiedAt: string | null;
  childCount: number;
  descendantCount: number;
}

export interface SearchQuery {
  text: string;
  kinds: Kind[];
  minBytes: number | null;
  maxBytes: number | null;
  modifiedAfter: string | null;
  modifiedBefore: string | null;
  diskIds: number[];
  limit: number;
  offset: number;
}

export interface SearchResult {
  items: Item[];
  total: number;
}

export interface DuplicateGroup {
  name: string;
  sizeBytes: number;
  items: Item[];
}

export interface MountedVolume {
  uuid: string;
  name: string;
  mountPath: string;
  filesystem: string;
  capacityBytes: number;
  freeBytes: number;
  catalogedDiskId: number | null;
}

export interface ReadOptions {
  mode: "quick" | "full";
  includeHidden: boolean;
}

export interface ReadProgress {
  diskUuid: string;
  phase: "listing" | "saving" | "done" | "error";
  itemsSeen: number;
  currentPath: string | null;
  message: string | null;
  bytesSeen: number;      // espaço real ocupado (alocado) somado até agora; nuvem-only = 0
  bytesExpected: number;  // espaço ocupado do volume no início (capacidade − livre); 0 = desconhecido
  inCloud: boolean;       // currentPath está numa pasta de nuvem
}

export interface Settings {
  language: "pt" | "en";
  includeHidden: boolean;
  viewMode: "columns" | "list";
  showInspector: boolean;
}
