//! Catálogo em SQLite: esquema, abertura, discos, configurações, exportação e importação.

use crate::error::{Error, Result};
use crate::model::*;
use rusqlite::{Connection, OptionalExtension, Row, params};
use std::path::{Path, PathBuf};
use unicode_normalization::UnicodeNormalization;

pub const SCHEMA_VERSION: i64 = 3;

/// Colunas de `items` na ordem lida por [`row_to_item`].
pub const ITEM_COLS: &str = "i.id, i.disk_id, i.parent_id, i.name, i.path, i.kind, i.ext, \
    i.size_bytes, i.created_at, i.modified_at, i.child_count, i.descendant_count";

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS disks (
    id INTEGER PRIMARY KEY,
    uuid TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    filesystem TEXT NOT NULL DEFAULT '',
    capacity_bytes INTEGER NOT NULL DEFAULT 0,
    free_bytes INTEGER NOT NULL DEFAULT 0,
    item_count INTEGER NOT NULL DEFAULT 0,
    total_bytes INTEGER NOT NULL DEFAULT 0,
    last_read_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS items (
    id INTEGER PRIMARY KEY,
    disk_id INTEGER NOT NULL REFERENCES disks(id) ON DELETE CASCADE,
    parent_id INTEGER,
    name TEXT NOT NULL,
    path TEXT NOT NULL,
    kind TEXT NOT NULL,
    ext TEXT,
    size_bytes INTEGER NOT NULL DEFAULT 0,
    created_at TEXT,
    modified_at TEXT,
    child_count INTEGER NOT NULL DEFAULT 0,
    descendant_count INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_items_parent ON items(disk_id, parent_id);
CREATE INDEX IF NOT EXISTS idx_items_dup ON items(name, size_bytes);
-- Busca: trigramas sobre o nome normalizado (minúsculo, sem acento). rowid = items.id.
CREATE VIRTUAL TABLE IF NOT EXISTS items_fts USING fts5(name_norm, tokenize='trigram');
CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
"#;

/// Takes one disk's items out of the search index. Deleting FTS rows one by one is slow
/// (minutes for millions of items), so when the disk holds most of the catalog the index is
/// rebuilt from the rows that stay, reusing their already-normalized text.
pub(crate) fn remove_disk_from_search(conn: &Connection, disk_id: i64) -> Result<()> {
    let mine: i64 = conn.query_row("SELECT COUNT(*) FROM items WHERE disk_id=?1", [disk_id], |r| r.get(0))?;
    if mine == 0 {
        return Ok(());
    }
    let total: i64 = conn.query_row("SELECT COUNT(*) FROM items", [], |r| r.get(0))?;
    if mine * 2 <= total {
        conn.execute("DELETE FROM items_fts WHERE rowid IN (SELECT id FROM items WHERE disk_id=?1)", [disk_id])?;
        return Ok(());
    }
    conn.execute_batch("DROP TABLE IF EXISTS temp.hdf_keep_fts;")?;
    conn.execute(
        "CREATE TEMP TABLE hdf_keep_fts AS SELECT f.rowid AS id, f.name_norm AS name_norm
         FROM items_fts f JOIN items i ON i.id = f.rowid WHERE i.disk_id <> ?1",
        [disk_id],
    )?;
    conn.execute_batch(
        "DROP TABLE items_fts;
         CREATE VIRTUAL TABLE items_fts USING fts5(name_norm, tokenize='trigram');
         INSERT INTO items_fts(rowid, name_norm) SELECT id, name_norm FROM temp.hdf_keep_fts;
         DROP TABLE temp.hdf_keep_fts;",
    )?;
    Ok(())
}

/// Minúsculas e sem acentos: "Vídeo" -> "video".
pub fn normalize(s: &str) -> String {
    s.nfd()
        .filter(|c| !('\u{0300}'..='\u{036f}').contains(c))
        .collect::<String>()
        .to_lowercase()
}

pub fn now_iso() -> String {
    crate::scan::iso_from_unix(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0),
    )
}

/// `<Documentos>/HD Fantasma/catalogo.sqlite`
pub fn default_catalog_path() -> Result<PathBuf> {
    let docs = dirs::document_dir()
        .ok_or_else(|| Error::msg("Não foi possível achar a pasta Documentos."))?;
    Ok(docs.join("HD Fantasma").join("catalogo.sqlite"))
}

/// The catalog was written by a newer HD Fantasma: never touch it (an older app could break it).
pub const MSG_NEWER_CATALOG: &str =
    "Este catálogo foi criado por uma versão mais nova do HD Fantasma. Atualize o app para abri-lo.";
/// The catalog file cannot be read as a catalog (damaged, or not a catalog at all).
pub const MSG_DAMAGED_CATALOG: &str = "O arquivo do catálogo está danificado.";

/// Extension of an exported disk file.
pub const EXPORT_EXT: &str = "hdfantasma";

pub struct Catalog {
    pub(crate) conn: Connection,
    path: PathBuf,
}

impl Catalog {
    /// Abre (e cria, se preciso) o catálogo no caminho dado.
    pub fn open(path: impl AsRef<Path>) -> Result<Catalog> {
        let path = path.as_ref().to_path_buf();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(&path)?;
        Self::init(conn, path)
    }

    pub fn open_default() -> Result<Catalog> {
        Self::open(default_catalog_path()?)
    }

    fn init(conn: Connection, path: PathBuf) -> Result<Catalog> {
        conn.busy_timeout(std::time::Duration::from_secs(10))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let cat = Catalog { conn, path };
        cat.migrate()?;
        Ok(cat)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn current_version(&self) -> Result<i64> {
        let exists: bool = self.conn.query_row(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='schema_version'",
            [],
            |r| r.get(0),
        )?;
        if !exists {
            return Ok(0);
        }
        Ok(self
            .conn
            .query_row("SELECT MAX(version) FROM schema_version", [], |r| {
                r.get::<_, Option<i64>>(0)
            })?
            .unwrap_or(0))
    }

    /// Cria/atualiza o esquema. Só pega a trava de escrita se houver algo a fazer.
    fn migrate(&self) -> Result<()> {
        let v = self.current_version()?;
        if v > SCHEMA_VERSION {
            return Err(Error::msg(MSG_NEWER_CATALOG));
        }
        if v == SCHEMA_VERSION {
            return Ok(());
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute_batch(SCHEMA)?;
        if v == 1 {
            // v2: o app nunca abre arquivos. Some o conteúdo compactado (inside_archive) e os
            // dados de vídeo. Catálogos antigos continuam abrindo; o resto fica como estava.
            tx.execute(
                "DELETE FROM items_fts WHERE rowid IN (SELECT id FROM items WHERE inside_archive=1)",
                [],
            )?;
            tx.execute("DELETE FROM items WHERE inside_archive=1", [])?;
            for col in ["inside_archive", "video_duration", "video_width", "video_height", "video_codec", "video_fps"] {
                tx.execute(&format!("ALTER TABLE items DROP COLUMN {col}"), [])?;
            }
            tx.execute(
                "UPDATE disks SET item_count=(SELECT COUNT(*) FROM items WHERE disk_id=disks.id),
                    total_bytes=(SELECT COALESCE(SUM(size_bytes),0) FROM items WHERE disk_id=disks.id AND kind!='folder')",
                [],
            )?;
        }
        if v >= 1 {
            // v3: total de itens dentro de cada pasta (`descendant_count`); calcula para o que já existe.
            // (Catálogo novo já nasce com a coluna no SCHEMA.)
            tx.execute("ALTER TABLE items ADD COLUMN descendant_count INTEGER NOT NULL DEFAULT 0", [])?;
            recompute_descendants(&tx, None).map_err(|_| Error::msg(MSG_DAMAGED_CATALOG))?;
        }
        tx.execute("DELETE FROM schema_version", [])?;
        tx.execute("INSERT INTO schema_version(version) VALUES (?1)", [SCHEMA_VERSION])?;
        tx.commit()?;
        Ok(())
    }

    // ---------------------------------------------------------------- discos

    pub(crate) fn row_to_disk(r: &Row) -> rusqlite::Result<Disk> {
        Ok(Disk {
            id: r.get(0)?,
            uuid: r.get(1)?,
            name: r.get(2)?,
            filesystem: r.get(3)?,
            capacity_bytes: r.get::<_, i64>(4)? as u64,
            free_bytes: r.get::<_, i64>(5)? as u64,
            item_count: r.get::<_, i64>(6)? as u64,
            total_bytes: r.get::<_, i64>(7)? as u64,
            last_read_at: r.get(8)?,
            present: false,
            mount_path: None,
        })
    }

    const DISK_COLS: &'static str =
        "id, uuid, name, filesystem, capacity_bytes, free_bytes, item_count, total_bytes, last_read_at";

    /// Discos do catálogo; `present`/`mountPath` vêm da lista de volumes montados.
    pub fn list_disks(&self, mounted: &[MountedVolume]) -> Result<Vec<Disk>> {
        let mut st = self
            .conn
            .prepare(&format!("SELECT {} FROM disks ORDER BY name COLLATE NOCASE, id", Self::DISK_COLS))?;
        let mut disks = st
            .query_map([], Self::row_to_disk)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for d in &mut disks {
            if let Some(v) = mounted.iter().find(|v| v.uuid == d.uuid) {
                d.present = true;
                d.mount_path = Some(v.mount_path.clone());
            }
        }
        Ok(disks)
    }

    pub fn get_disk(&self, id: i64) -> Result<Disk> {
        self.conn
            .query_row(
                &format!("SELECT {} FROM disks WHERE id=?1", Self::DISK_COLS),
                [id],
                Self::row_to_disk,
            )
            .optional()?
            .ok_or_else(|| Error::msg("Disco não encontrado no catálogo."))
    }

    pub fn find_disk_by_uuid(&self, uuid: &str) -> Result<Option<Disk>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {} FROM disks WHERE uuid=?1", Self::DISK_COLS),
                [uuid],
                Self::row_to_disk,
            )
            .optional()?)
    }

    pub fn rename_disk(&self, id: i64, name: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() {
            return Err(Error::msg("O nome do disco não pode ficar vazio."));
        }
        let n = self.conn.execute("UPDATE disks SET name=?1 WHERE id=?2", params![name, id])?;
        if n == 0 {
            return Err(Error::msg("Disco não encontrado no catálogo."));
        }
        Ok(())
    }

    /// Apaga só do catálogo (nunca toca no disco de verdade).
    pub fn delete_disk(&mut self, id: i64) -> Result<()> {
        let tx = self.conn.transaction()?;
        remove_disk_from_search(&tx, id)?;
        tx.execute("DELETE FROM items WHERE disk_id=?1", [id])?;
        tx.execute("DELETE FROM disks WHERE id=?1", [id])?;
        tx.commit()?;
        Ok(())
    }

    // ---------------------------------------------------------- configurações

    pub fn get_settings(&self) -> Result<Settings> {
        let raw: Option<String> = self
            .conn
            .query_row("SELECT value FROM settings WHERE key='app'", [], |r| r.get(0))
            .optional()?;
        Ok(raw
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default())
    }

    pub fn set_settings(&self, s: &Settings) -> Result<Settings> {
        let json = serde_json::to_string(s).map_err(|e| Error::msg(e.to_string()))?;
        self.conn.execute(
            "INSERT INTO settings(key,value) VALUES('app',?1)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [json],
        )?;
        Ok(s.clone())
    }

    // ------------------------------------------------- exportação / importação

    /// Grava um `.hdfantasma` (arquivo SQLite, mesmo esquema) só com este disco.
    pub fn export_disk(&self, disk_id: i64, dest: impl AsRef<Path>) -> Result<()> {
        let disk = self.get_disk(disk_id)?;
        let dest = dest.as_ref();
        // The only file this app ever replaces is an export the person chose: it must be a
        // `.hdfantasma`, and never the live catalog itself.
        let is_export = dest
            .extension()
            .map(|e| e.eq_ignore_ascii_case(EXPORT_EXT))
            .unwrap_or(false);
        if !is_export {
            return Err(Error::msg("O arquivo de exportação precisa terminar em .hdfantasma."));
        }
        if let (Ok(a), Ok(b)) = (std::fs::canonicalize(dest), std::fs::canonicalize(&self.path)) {
            if a == b {
                return Err(Error::msg("Escolha outro lugar: este é o catálogo em uso."));
            }
        }
        if dest.exists() {
            if !dest.is_file() {
                return Err(Error::msg("Já existe uma pasta com esse nome. Escolha outro nome."));
            }
            std::fs::remove_file(dest)?;
        }
        if let Some(dir) = dest.parent() {
            std::fs::create_dir_all(dir)?;
        }
        {
            // Cria o arquivo já com o esquema.
            let out = Catalog::open(dest)?;
            out.conn.pragma_update(None, "journal_mode", "DELETE")?;
        }
        self.conn
            .execute("ATTACH DATABASE ?1 AS exp", [dest.to_string_lossy().as_ref()])?;
        let r = (|| -> Result<()> {
            self.conn.execute_batch("BEGIN")?;
            self.conn.execute(
                "INSERT INTO exp.disks(id,uuid,name,filesystem,capacity_bytes,free_bytes,item_count,total_bytes,last_read_at)
                 SELECT id,uuid,name,filesystem,capacity_bytes,free_bytes,item_count,total_bytes,last_read_at
                 FROM main.disks WHERE id=?1",
                [disk.id],
            )?;
            self.conn.execute(
                "INSERT INTO exp.items SELECT * FROM main.items WHERE disk_id=?1",
                [disk.id],
            )?;
            self.conn.execute(
                "INSERT INTO exp.items_fts(rowid,name_norm)
                 SELECT f.rowid, f.name_norm FROM main.items_fts f
                 JOIN main.items i ON i.id=f.rowid WHERE i.disk_id=?1",
                [disk.id],
            )?;
            self.conn.execute_batch("COMMIT")?;
            Ok(())
        })();
        if r.is_err() {
            let _ = self.conn.execute_batch("ROLLBACK");
        }
        let _ = self.conn.execute_batch("DETACH DATABASE exp");
        r
    }

    /// Importa um `.hdfantasma`. Se o uuid já existe, esse disco é substituído.
    pub fn import_disk(&mut self, src: impl AsRef<Path>) -> Result<Disk> {
        let src = src.as_ref();
        if !src.is_file() {
            return Err(Error::msg("Arquivo de exportação não encontrado."));
        }
        // Valida antes de anexar.
        {
            let probe = Connection::open_with_flags(
                readonly_uri(src)?,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
            )
            .map_err(|_| Error::msg("Este arquivo não é uma exportação válida do HD Fantasma."))?;
            let ok: rusqlite::Result<i64> = probe.query_row(
                "SELECT (SELECT COUNT(*) FROM disks) + (SELECT COUNT(*) FROM items) + (SELECT COUNT(*) FROM schema_version)",
                [],
                |r| r.get(0),
            );
            if ok.is_err() {
                return Err(Error::msg("Este arquivo não é uma exportação válida do HD Fantasma."));
            }
            let v: i64 = probe
                .query_row("SELECT MAX(version) FROM schema_version", [], |r| r.get::<_, Option<i64>>(0))
                .ok()
                .flatten()
                .unwrap_or(0);
            if v > SCHEMA_VERSION {
                return Err(Error::msg(
                    "Esta exportação foi feita por uma versão mais nova do HD Fantasma. Atualize o app.",
                ));
            }
        }
        // Attached read-only and immutable: importing never writes anything next to the file
        // (no -wal/-shm, no lock), wherever it lives.
        self.conn.execute("ATTACH DATABASE ?1 AS imp", [readonly_uri(src)?])?;
        let r = (|| -> Result<i64> {
            validate_import(&self.conn)?;
            let src_disk: (String, String, String, i64, i64, i64, i64, String) = self
                .conn
                .query_row(
                    "SELECT uuid,name,filesystem,capacity_bytes,free_bytes,item_count,total_bytes,last_read_at
                     FROM imp.disks LIMIT 1",
                    [],
                    |r| {
                        Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?))
                    },
                )
                .optional()?
                .ok_or_else(|| Error::msg("A exportação não contém nenhum disco."))?;
            self.conn.execute_batch("BEGIN IMMEDIATE")?;
            let inner = (|| -> Result<i64> {
                let existing: Option<i64> = self
                    .conn
                    .query_row("SELECT id FROM disks WHERE uuid=?1", [&src_disk.0], |r| r.get(0))
                    .optional()?;
                let disk_id = match existing {
                    Some(id) => {
                        remove_disk_from_search(&self.conn, id)?;
                        self.conn.execute("DELETE FROM items WHERE disk_id=?1", [id])?;
                        self.conn.execute(
                            "UPDATE disks SET name=?2, filesystem=?3, capacity_bytes=?4, free_bytes=?5,
                             item_count=?6, total_bytes=?7, last_read_at=?8 WHERE id=?1",
                            params![id, src_disk.1, src_disk.2, src_disk.3, src_disk.4, src_disk.5, src_disk.6, src_disk.7],
                        )?;
                        id
                    }
                    None => {
                        self.conn.execute(
                            "INSERT INTO disks(uuid,name,filesystem,capacity_bytes,free_bytes,item_count,total_bytes,last_read_at)
                             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                            params![src_disk.0, src_disk.1, src_disk.2, src_disk.3, src_disk.4, src_disk.5, src_disk.6, src_disk.7],
                        )?;
                        self.conn.last_insert_rowid()
                    }
                };
                let offset: i64 = self.conn.query_row(
                    "SELECT COALESCE(MAX(id),0) FROM main.items",
                    [],
                    |r| r.get(0),
                )?;
                // Exportação v1 traz conteúdo compactado (inside_archive=1): não entra mais.
                let old_fmt: bool = self.conn.query_row(
                    "SELECT COUNT(*) > 0 FROM pragma_table_info('items','imp') WHERE name='inside_archive'",
                    [],
                    |r| r.get(0),
                )?;
                let keep = if old_fmt { "WHERE inside_archive=0" } else { "" };
                let keep_i = if old_fmt { "WHERE i.inside_archive=0" } else { "" };
                self.conn.execute(
                    &format!(
                        "INSERT INTO main.items(id,disk_id,parent_id,name,path,kind,ext,size_bytes,created_at,modified_at,child_count)
                         SELECT id+?1, ?2, CASE WHEN parent_id IS NULL THEN NULL ELSE parent_id+?1 END,
                            name,path,kind,ext,size_bytes,created_at,modified_at,child_count
                         FROM imp.items {keep}"
                    ),
                    params![offset, disk_id],
                )?;
                // The search text is rebuilt from the names, never taken from the file.
                let _ = keep_i;
                self.conn.create_scalar_function(
                    "hdf_norm",
                    1,
                    rusqlite::functions::FunctionFlags::SQLITE_UTF8
                        | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC,
                    |ctx| Ok(normalize(&ctx.get::<String>(0)?)),
                )?;
                self.conn.execute(
                    "INSERT INTO main.items_fts(rowid,name_norm)
                     SELECT id, hdf_norm(name) FROM main.items WHERE disk_id=?1",
                    [disk_id],
                )?;
                // Exportações antigas não trazem `descendant_count`: recalcula para este disco.
                recompute_descendants(&self.conn, Some(disk_id))?;
                Ok(disk_id)
            })();
            match inner {
                Ok(id) => {
                    self.conn.execute_batch("COMMIT")?;
                    Ok(id)
                }
                Err(e) => {
                    let _ = self.conn.execute_batch("ROLLBACK");
                    Err(e)
                }
            }
        })();
        let _ = self.conn.execute_batch("DETACH DATABASE imp");
        let id = r?;
        self.get_disk(id)
    }
}

const INVALID_EXPORT: &str = "Este arquivo não é uma exportação válida do HD Fantasma.";

/// `file:` URI that opens `path` read-only and immutable (SQLite never writes or locks it).
fn readonly_uri(path: &Path) -> Result<String> {
    let abs = std::fs::canonicalize(path)?;
    let mut uri = String::from("file:");
    for ch in abs.to_string_lossy().chars() {
        match ch {
            '%' => uri.push_str("%25"),
            '?' => uri.push_str("%3F"),
            '#' => uri.push_str("%23"),
            c => uri.push(c),
        }
    }
    uri.push_str("?immutable=1");
    Ok(uri)
}

/// An export comes from someone else's computer: check every value before copying it in.
/// SQLite accepts any type in any column, so types are checked here; paths must stay inside
/// the disk (relative, no `..`), and every parent must exist in the same file.
fn validate_import(conn: &Connection) -> Result<()> {
    let bad = |sql: &str| -> Result<bool> {
        match conn.query_row(sql, [], |r| r.get::<_, i64>(0)) {
            Ok(n) => Ok(n != 0),
            Err(_) => Ok(true),
        }
    };
    let disks = "SELECT COUNT(*) FROM imp.disks WHERE typeof(uuid)!='text' OR uuid='' OR length(uuid)>256
         OR typeof(name)!='text' OR length(name)>1024
         OR typeof(filesystem)!='text' OR typeof(last_read_at)!='text'
         OR typeof(capacity_bytes)!='integer' OR capacity_bytes<0
         OR typeof(free_bytes)!='integer' OR free_bytes<0
         OR typeof(item_count)!='integer' OR item_count<0
         OR typeof(total_bytes)!='integer' OR total_bytes<0";
    // Ids stay far from the integer limit (id + offset must never overflow); names and paths
    // keep the sizes a real disk can have.
    let items = "SELECT COUNT(*) FROM imp.items WHERE typeof(id)!='integer' OR id<1 OR id>1000000000000
         OR typeof(parent_id) NOT IN ('integer','null') OR parent_id=id
         OR typeof(name)!='text' OR name='' OR instr(name,'/')>0 OR instr(name,char(0))>0 OR length(name)>1024
         OR typeof(path)!='text' OR path='' OR instr(path,char(0))>0 OR length(path)>32768
         OR path LIKE '/%' OR path='..' OR path LIKE '../%' OR path LIKE '%/../%' OR path LIKE '%/..'
         OR typeof(kind)!='text'
         OR typeof(ext) NOT IN ('text','null')
         OR typeof(size_bytes)!='integer' OR size_bytes<0
         OR typeof(child_count)!='integer' OR child_count<0
         OR typeof(created_at) NOT IN ('text','null')
         OR typeof(modified_at) NOT IN ('text','null')";
    // Only plain tables are read. A view (or trigger) in someone else's file could return
    // different rows on each read, so what was validated would not be what gets copied.
    let schema = "SELECT (SELECT COUNT(*) FROM imp.sqlite_master WHERE type NOT IN ('table','index'))
         + (SELECT COUNT(*) FROM (SELECT 'disks' n UNION ALL SELECT 'items' UNION ALL SELECT 'schema_version'
                                  UNION ALL SELECT 'items_fts') w
            WHERE NOT EXISTS (SELECT 1 FROM imp.sqlite_master m WHERE m.name=w.n AND m.type='table'))";
    if bad(schema)? {
        return Err(Error::msg(INVALID_EXPORT));
    }
    let orphans = "SELECT COUNT(*) FROM imp.items i WHERE i.parent_id IS NOT NULL
         AND NOT EXISTS (SELECT 1 FROM imp.items p WHERE p.id=i.parent_id)";
    if bad(disks)? || bad(items)? || bad(orphans)? {
        return Err(Error::msg(INVALID_EXPORT));
    }
    Ok(())
}

/// Recalcula `descendant_count` das pastas (de um disco, ou de todos com `None`):
/// cada item soma 1 em todas as pastas acima dele.
pub fn recompute_descendants(conn: &Connection, disk_id: Option<i64>) -> Result<()> {
    let mut parent: std::collections::HashMap<i64, Option<i64>> = std::collections::HashMap::new();
    let mut folders: Vec<i64> = Vec::new();
    {
        let sql = if disk_id.is_some() {
            "SELECT id, parent_id, kind FROM items WHERE disk_id=?1"
        } else {
            "SELECT id, parent_id, kind FROM items WHERE ?1 IS NULL"
        };
        let mut st = conn.prepare(sql)?;
        let mut rows = st.query([disk_id])?;
        while let Some(r) = rows.next()? {
            let id: i64 = r.get(0)?;
            parent.insert(id, r.get(1)?);
            if r.get::<_, String>(2)? == "folder" {
                folders.push(id);
            }
        }
    }
    // Linear, one pass: every item is visited once, from the top folders down; each folder then
    // adds up its children on the way back. An item never reached from the top sits in a loop
    // (a folder that is its own ancestor: damaged or hostile file) and is refused.
    let mut kids: std::collections::HashMap<i64, Vec<i64>> = std::collections::HashMap::new();
    let mut roots: Vec<i64> = Vec::new();
    for (&id, p) in parent.iter() {
        match p {
            Some(pid) if parent.contains_key(pid) => kids.entry(*pid).or_default().push(id),
            _ => roots.push(id),
        }
    }
    let mut order: Vec<i64> = Vec::with_capacity(parent.len());
    let mut stack = roots;
    while let Some(id) = stack.pop() {
        order.push(id);
        if let Some(k) = kids.get(&id) {
            stack.extend_from_slice(k);
        }
    }
    if order.len() != parent.len() {
        return Err(Error::msg(INVALID_EXPORT));
    }
    let mut count: std::collections::HashMap<i64, i64> = std::collections::HashMap::new();
    for &id in order.iter().rev() {
        let below = count.get(&id).copied().unwrap_or(0);
        if let Some(Some(pid)) = parent.get(&id) {
            if parent.contains_key(pid) {
                *count.entry(*pid).or_default() += 1 + below;
            }
        }
    }
    let mut upd = conn.prepare("UPDATE items SET descendant_count=?1 WHERE id=?2")?;
    for id in folders {
        upd.execute(params![count.get(&id).copied().unwrap_or(0), id])?;
    }
    Ok(())
}

pub fn row_to_item(r: &Row) -> rusqlite::Result<Item> {
    Ok(Item {
        id: r.get(0)?,
        disk_id: r.get(1)?,
        parent_id: r.get(2)?,
        name: r.get(3)?,
        path: r.get(4)?,
        kind: Kind::parse(&r.get::<_, String>(5)?),
        ext: r.get(6)?,
        size_bytes: r.get::<_, i64>(7)? as u64,
        created_at: r.get(8)?,
        modified_at: r.get(9)?,
        child_count: r.get::<_, i64>(10)? as u64,
        descendant_count: r.get::<_, i64>(11)? as u64,
    })
}
