//! Catálogo em SQLite: esquema, abertura, discos, configurações, exportação e importação.

use crate::error::{Error, Result};
use crate::model::*;
use rusqlite::{Connection, OptionalExtension, Row, params};
use std::path::{Path, PathBuf};
use unicode_normalization::UnicodeNormalization;

pub const SCHEMA_VERSION: i64 = 1;

/// Colunas de `items` na ordem lida por [`row_to_item`].
pub const ITEM_COLS: &str = "i.id, i.disk_id, i.parent_id, i.name, i.path, i.kind, i.ext, \
    i.size_bytes, i.created_at, i.modified_at, i.inside_archive, i.child_count, \
    i.video_duration, i.video_width, i.video_height, i.video_codec, i.video_fps";

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
    inside_archive INTEGER NOT NULL DEFAULT 0,
    child_count INTEGER NOT NULL DEFAULT 0,
    video_duration REAL,
    video_width INTEGER,
    video_height INTEGER,
    video_codec TEXT,
    video_fps REAL
);
CREATE INDEX IF NOT EXISTS idx_items_parent ON items(disk_id, parent_id);
CREATE INDEX IF NOT EXISTS idx_items_dup ON items(name, size_bytes);
-- Busca: trigramas sobre o nome normalizado (minúsculo, sem acento). rowid = items.id.
CREATE VIRTUAL TABLE IF NOT EXISTS items_fts USING fts5(name_norm, tokenize='trigram');
CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
"#;

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
        if v >= SCHEMA_VERSION {
            return Ok(());
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute_batch(SCHEMA)?;
        // Migrações futuras entram aqui: `if v < 2 { ... }`.
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
        tx.execute(
            "DELETE FROM items_fts WHERE rowid IN (SELECT id FROM items WHERE disk_id=?1)",
            [id],
        )?;
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
        if dest.exists() {
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
                src,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
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
        self.conn
            .execute("ATTACH DATABASE ?1 AS imp", [src.to_string_lossy().as_ref()])?;
        let r = (|| -> Result<i64> {
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
                        self.conn.execute(
                            "DELETE FROM items_fts WHERE rowid IN (SELECT id FROM items WHERE disk_id=?1)",
                            [id],
                        )?;
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
                self.conn.execute(
                    "INSERT INTO main.items(id,disk_id,parent_id,name,path,kind,ext,size_bytes,created_at,modified_at,
                        inside_archive,child_count,video_duration,video_width,video_height,video_codec,video_fps)
                     SELECT id+?1, ?2, CASE WHEN parent_id IS NULL THEN NULL ELSE parent_id+?1 END,
                        name,path,kind,ext,size_bytes,created_at,modified_at,
                        inside_archive,child_count,video_duration,video_width,video_height,video_codec,video_fps
                     FROM imp.items",
                    params![offset, disk_id],
                )?;
                self.conn.execute(
                    "INSERT INTO main.items_fts(rowid,name_norm) SELECT rowid+?1, name_norm FROM imp.items_fts",
                    [offset],
                )?;
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

pub fn row_to_item(r: &Row) -> rusqlite::Result<Item> {
    let dur: Option<f64> = r.get(12)?;
    let w: Option<i64> = r.get(13)?;
    let h: Option<i64> = r.get(14)?;
    let codec: Option<String> = r.get(15)?;
    let fps: Option<f64> = r.get(16)?;
    let video = VideoInfo {
        duration_sec: dur,
        width: w.map(|v| v as u32),
        height: h.map(|v| v as u32),
        codec,
        fps,
    };
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
        inside_archive: r.get::<_, i64>(10)? != 0,
        child_count: r.get::<_, i64>(11)? as u64,
        video: if video.is_empty() { None } else { Some(video) },
    })
}
