# Contrato motor ↔ telas (v1)

Fonte única dos tipos e comandos entre `crates/core` + `src-tauri` (Rust) e `src/` (Svelte).
Mudou aqui → mudar os dois lados. Nomes seguem `CONTEXT.md` (Disco, Item, Leitura...).
JSON em camelCase (Rust usa `#[serde(rename_all = "camelCase")]`).

## Tipos

```ts
type Kind = "folder" | "video" | "image" | "audio" | "document" | "archive" | "other";

interface Disk {
  id: number;            // id interno do catálogo
  uuid: string;          // identidade do volume (ADR 0003)
  name: string;          // nome do volume na última leitura
  filesystem: string;    // "apfs", "hfs", "exfat", "msdos", "ntfs", "smbfs"...
  capacityBytes: number;
  freeBytes: number;
  itemCount: number;
  totalBytes: number;    // soma dos arquivos catalogados
  lastReadAt: string;    // ISO 8601
  present: boolean;      // está plugado agora?
  mountPath: string | null; // onde está montado, se presente
}

interface Item {
  id: number;
  diskId: number;
  parentId: number | null;   // null = raiz do disco
  name: string;
  path: string;              // caminho relativo à raiz do disco, com "/" ("Projetos/2025/a.mov")
  kind: Kind;
  ext: string | null;        // minúsculo, sem ponto
  sizeBytes: number;         // pasta = soma recursiva
  createdAt: string | null;
  modifiedAt: string | null;
  childCount: number;        // pastas: filhos diretos; 0 nos demais
  descendantCount: number;   // pastas: total de itens dentro, recursivo; 0 nos demais
}

interface SearchQuery {
  text: string;              // vazio = só filtros
  kinds: Kind[];             // vazio = todos
  minBytes: number | null;
  maxBytes: number | null;
  modifiedAfter: string | null;   // "2025-01-01"
  modifiedBefore: string | null;
  diskIds: number[];         // vazio = todos
  limit: number;             // padrão 500
  offset: number;
}

interface SearchResult { items: Item[]; total: number; }

interface DuplicateGroup { name: string; sizeBytes: number; items: Item[]; }

interface MountedVolume {    // disco presente que pode ser lido
  uuid: string; name: string; mountPath: string; filesystem: string;
  capacityBytes: number; freeBytes: number;
  catalogedDiskId: number | null;  // já está no catálogo?
}

interface ReadOptions { mode: "quick" | "full"; includeHidden: boolean; }

// evento "read-progress", emitido durante a Leitura
interface ReadProgress {
  diskUuid: string; phase: "listing" | "saving" | "done" | "error";
  itemsSeen: number; currentPath: string | null; message: string | null;
  bytesSeen: number;        // espaço real ocupado (alocado) somado até agora; nuvem-only = 0
  bytesExpected: number;    // espaço ocupado do volume no início (capacidade − livre); 0 se desconhecido
  inCloud: boolean;         // currentPath está numa pasta de nuvem (CloudStorage, Mobile Documents)
}
// A UI calcula porcentagem (teto 99% até "done") e o tempo restante
// pela velocidade média dos últimos ~20 s; "Calculando…" nos primeiros 5 s.
```

## Comandos Tauri (`invoke`)

| Comando | Argumentos | Retorno |
|---|---|---|
| `list_disks` | — | `Disk[]` |
| `list_mounted_volumes` | — | `MountedVolume[]` |
| `read_disk` | `{ mountPath: string, options: ReadOptions }` | `Disk` (emite `read-progress`) |
| `cancel_read` | — | `void` |
| `list_children` | `{ diskId: number, parentId: number \| null }` | `Item[]` (pastas primeiro, depois nome) |
| `get_item` | `{ id: number }` | `Item` |
| `search` | `{ query: SearchQuery }` | `SearchResult` |
| `find_duplicates` | `{ diskIds: number[], minBytes: number }` | `DuplicateGroup[]` |
| `rename_disk` | `{ diskId: number, name: string }` | `void` |
| `delete_disk` | `{ diskId: number }` | `void` (só apaga do catálogo) |
| `export_disk` | `{ diskId: number, destPath: string }` | `void` |
| `import_disk` | `{ srcPath: string }` | `Disk` |
| `reveal_item` | `{ id: number }` | `void` (abre no Finder/Explorer se o disco estiver presente) |
| `disk_access` | `{ mountPath: string }` | `"ok" \| "needed"` (macOS: disco interno sem Acesso Total ao Disco) |
| `open_disk_access_settings` | — | `void` (abre Ajustes › Privacidade › Acesso Total ao Disco; página local, sem rede) |
| `wait_catalog_ready` | — | `void` (espera o catálogo abrir; um catálogo antigo é convertido na abertura, ADR 0018) |
| `get_settings` / `set_settings` | `{ settings: { language: "pt" \| "en", includeHidden: boolean, viewMode: "columns" \| "list", showInspector: boolean } }` | settings |

Erros: string legível em pt-BR (ex.: `"O disco não está plugado."`).

O app nunca abre arquivos (ADR 0013): sem dados de vídeo, sem conteúdo de compactados.

## Ligação com o Claude (ADR 0017)

```ts
interface ClaudeStatus {
  mcpPath: string;                 // binário hd-fantasma-mcp ao lado do app
  desktop: { installed: boolean; connected: boolean; configPath: string };
  code: { cliFound: boolean; connected: boolean; command: string }; // command = linha pronta para copiar
}
```

| Comando | Argumentos | Retorno |
|---|---|---|
| `claude_status` | — | `ClaudeStatus` |
| `claude_connect` | `{ target?: "desktop" \| "code" }` | `ClaudeStatus` (sem target: conecta nos que existirem) |
| `claude_disconnect` | `{ target?: "desktop" \| "code" }` | `ClaudeStatus` |
