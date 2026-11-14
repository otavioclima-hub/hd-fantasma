//! A Leitura: percorre um disco presente e grava os itens no catálogo.
//!
//! Tudo acontece numa única transação: se a Leitura for cancelada ou falhar, o catálogo
//! fica exatamente como estava. Os itens vão direto para o banco (streaming), então
//! discos com milhões de itens não esgotam a memória: só o que é caro de recalcular
//! (dados de vídeo / conteúdo compactado) é guardado em memória na Atualização rápida.

use crate::db::{Catalog, normalize, now_iso};
use crate::error::{Error, Result};
use crate::model::*;
use rusqlite::{Connection, Statement, params};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

// ------------------------------------------------------------ ponto de extensão

/// Uma entrada dentro de um .zip/.rar (caminho interno com "/").
#[derive(Debug, Clone, PartialEq)]
pub struct ArchiveEntry {
    pub path: String,
    pub size_bytes: u64,
    pub modified_at: Option<String>,
    pub is_dir: bool,
}

/// O que um extrator caro devolve para um item.
#[derive(Debug, Clone, Default)]
pub struct Details {
    pub video: Option<VideoInfo>,
    pub archive_entries: Vec<ArchiveEntry>,
}

/// Ponto de encaixe para o trabalho caro (ffprobe, abrir .zip/.rar).
/// `wants` decide se o item precisa disso; `extract` faz o trabalho.
pub trait DetailExtractor: Send + Sync {
    fn wants(&self, item: &Item) -> bool;
    fn extract(&self, path: &Path) -> Details;
}

/// Extrator padrão: não faz nada.
pub struct NoopExtractor;

impl DetailExtractor for NoopExtractor {
    fn wants(&self, _item: &Item) -> bool {
        false
    }
    fn extract(&self, _path: &Path) -> Details {
        Details::default()
    }
}

// ------------------------------------------------------------------ utilidades

const IGNORED: &[&str] = &[
    ".DS_Store",
    ".Spotlight-V100",
    ".Trashes",
    ".fseventsd",
    ".TemporaryItems",
    ".DocumentRevisions-V100",
    "$RECYCLE.BIN",
    "System Volume Information",
    "Thumbs.db",
    "desktop.ini",
];

/// Pastas na raiz de "/" que são só do sistema (ou ruído).
const ROOT_SYSTEM_DIRS: &[&str] = &[
    "System", "dev", "Volumes", "cores", ".vol", "private", "bin", "sbin", "usr", "etc", "var",
    "tmp", ".Spotlight-V100", ".fseventsd", ".DocumentRevisions-V100", ".PKInstallSandboxManager",
];

fn is_ignored(name: &str) -> bool {
    name.starts_with("._") || IGNORED.iter().any(|x| x.eq_ignore_ascii_case(name))
}

/// Segundos desde 1970 -> "YYYY-MM-DDTHH:MM:SSZ".
pub fn iso_from_unix(secs: i64) -> String {
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400);
    // Algoritmo de Howard Hinnant (civil_from_days).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

fn iso(t: std::io::Result<std::time::SystemTime>) -> Option<String> {
    let t = t.ok()?;
    let secs = match t.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(e) => -(e.duration().as_secs() as i64),
    };
    Some(iso_from_unix(secs))
}

fn ext_of(name: &str) -> Option<String> {
    Path::new(name)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .filter(|e| !e.is_empty())
}

#[cfg(unix)]
fn dev_of(md: &std::fs::Metadata) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    Some(md.dev())
}
#[cfg(not(unix))]
fn dev_of(_md: &std::fs::Metadata) -> Option<u64> {
    None
}

// ---------------------------------------------------------------- Leitura

struct OldInfo {
    id: i64,
    size: u64,
    modified: Option<String>,
    video: Option<VideoInfo>,
    child_count: u64,
}

struct Walker<'a> {
    conn: &'a Connection,
    ins: Statement<'a>,
    ins_fts: Statement<'a>,
    upd: Statement<'a>,
    disk_id: i64,
    uuid: String,
    root: PathBuf,
    include_hidden: bool,
    is_slash: bool,
    allowed_devs: Vec<u64>,
    extractor: &'a dyn DetailExtractor,
    cancel: &'a AtomicBool,
    progress: &'a mut dyn FnMut(ReadProgress),
    old: HashMap<String, OldInfo>,
    next_id: i64,
    seen: u64,
    skipped: u64,
    item_count: u64,
    total_bytes: u64,
    phase: Phase,
    last_emit: Instant,
}

struct Node {
    name: String,
    rel: String,
    is_dir: bool,
    size: u64,
    modified: Option<String>,
    parent: Option<usize>,
    children: u64,
}

impl<'a> Walker<'a> {
    fn emit(&mut self, path: Option<&str>, message: Option<String>) {
        self.last_emit = Instant::now();
        (self.progress)(ReadProgress {
            disk_uuid: self.uuid.clone(),
            phase: self.phase,
            items_seen: self.seen,
            current_path: path.map(|s| s.to_string()),
            message,
        });
    }

    fn tick(&mut self, rel: &str) -> Result<()> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        self.seen += 1;
        if self.last_emit.elapsed() >= Duration::from_millis(200) {
            self.emit(Some(rel), None);
        }
        Ok(())
    }

    fn insert(&mut self, it: &Item) -> Result<()> {
        let v = it.video.as_ref();
        self.ins.execute(params![
            it.id,
            it.disk_id,
            it.parent_id,
            it.name,
            it.path,
            it.kind.as_str(),
            it.ext,
            it.size_bytes as i64,
            it.created_at,
            it.modified_at,
            it.inside_archive as i64,
            it.child_count as i64,
            v.and_then(|v| v.duration_sec),
            v.and_then(|v| v.width),
            v.and_then(|v| v.height),
            v.and_then(|v| v.codec.clone()),
            v.and_then(|v| v.fps),
        ])?;
        self.ins_fts.execute(params![it.id, normalize(&it.name)])?;
        Ok(())
    }

    fn alloc(&mut self) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn skip_dir(&self, rel: &str, md: &std::fs::Metadata) -> bool {
        if self.is_slash && ROOT_SYSTEM_DIRS.contains(&rel) {
            return true;
        }
        // Não atravessa para outro volume montado.
        if let Some(d) = dev_of(md) {
            if !self.allowed_devs.is_empty() && !self.allowed_devs.contains(&d) {
                return true;
            }
        }
        false
    }

    /// Percorre uma pasta; devolve (tamanho recursivo, nº de filhos diretos).
    fn walk_dir(&mut self, dir: &Path, rel: &str, parent: Option<i64>) -> Result<(u64, u64)> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        let rd = match std::fs::read_dir(dir) {
            Ok(r) => r,
            Err(_) => {
                self.skipped += 1;
                return Ok((0, 0));
            }
        };
        let (mut size, mut count) = (0u64, 0u64);
        for entry in rd {
            let Ok(entry) = entry else {
                self.skipped += 1;
                continue;
            };
            let name = entry.file_name().to_string_lossy().into_owned();
            if !self.include_hidden && is_ignored(&name) {
                continue;
            }
            let Ok(ft) = entry.file_type() else {
                self.skipped += 1;
                continue;
            };
            if ft.is_symlink() {
                continue;
            }
            let Ok(md) = entry.metadata() else {
                self.skipped += 1;
                continue;
            };
            let child_rel = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
            if ft.is_dir() {
                if self.skip_dir(&child_rel, &md) {
                    continue;
                }
                self.tick(&child_rel)?;
                let id = self.alloc();
                let mut folder = Item {
                    id,
                    disk_id: self.disk_id,
                    parent_id: parent,
                    name,
                    path: child_rel.clone(),
                    kind: Kind::Folder,
                    ext: None,
                    size_bytes: 0,
                    created_at: iso(md.created()),
                    modified_at: iso(md.modified()),
                    inside_archive: false,
                    child_count: 0,
                    video: None,
                };
                self.insert(&folder)?;
                self.item_count += 1;
                let (s, c) = self.walk_dir(&entry.path(), &child_rel, Some(id))?;
                if s != 0 || c != 0 {
                    folder.size_bytes = s;
                    folder.child_count = c;
                    self.upd.execute(params![s as i64, c as i64, id])?;
                }
                size += s;
                count += 1;
            } else if ft.is_file() {
                self.tick(&child_rel)?;
                self.file(entry.path(), name, child_rel, parent, &md)?;
                size += md.len();
                count += 1;
            }
        }
        Ok((size, count))
    }

    fn file(
        &mut self,
        full: PathBuf,
        name: String,
        rel: String,
        parent: Option<i64>,
        md: &std::fs::Metadata,
    ) -> Result<()> {
        let ext = ext_of(&name);
        let kind = Kind::from_ext(ext.as_deref());
        let id = self.alloc();
        let modified_at = iso(md.modified());
        let mut item = Item {
            id,
            disk_id: self.disk_id,
            parent_id: parent,
            name,
            path: rel,
            kind,
            ext,
            size_bytes: md.len(),
            created_at: iso(md.created()),
            modified_at,
            inside_archive: false,
            child_count: 0,
            video: None,
        };

        // Atualização rápida: reaproveita o que já foi calculado se nada mudou.
        let mut entries: Vec<ArchiveEntry> = Vec::new();
        let mut reused = false;
        if let Some(o) = self.old.get(&item.path) {
            if o.size == item.size_bytes && o.modified == item.modified_at {
                let has_video = o.video.is_some();
                let has_archive = kind == Kind::Archive && o.child_count > 0;
                if has_video || has_archive {
                    reused = true;
                    item.video = o.video.clone();
                    if has_archive {
                        let (oid, opath) = (o.id, item.path.clone());
                        entries = self.old_archive_entries(oid, &opath)?;
                    }
                }
            }
        }
        if !reused && self.extractor.wants(&item) {
            self.phase = Phase::Details;
            let d = self.extractor.extract(&full);
            self.phase = Phase::Listing;
            item.video = d.video.filter(|v| !v.is_empty());
            entries = d.archive_entries;
        }

        let nodes = build_tree(&entries);
        item.child_count = nodes.iter().filter(|n| n.parent.is_none()).count() as u64;
        self.insert(&item)?;
        self.item_count += 1;
        self.total_bytes += item.size_bytes;
        if !nodes.is_empty() {
            self.insert_archive_nodes(&item, &nodes)?;
        }
        Ok(())
    }

    fn insert_archive_nodes(&mut self, archive: &Item, nodes: &[Node]) -> Result<()> {
        let base = self.next_id;
        self.next_id += nodes.len() as i64;
        for (i, n) in nodes.iter().enumerate() {
            let ext = if n.is_dir { None } else { ext_of(&n.name) };
            let it = Item {
                id: base + i as i64,
                disk_id: self.disk_id,
                parent_id: Some(n.parent.map(|p| base + p as i64).unwrap_or(archive.id)),
                name: n.name.clone(),
                path: format!("{}/{}", archive.path, n.rel),
                kind: if n.is_dir { Kind::Folder } else { Kind::from_ext(ext.as_deref()) },
                ext,
                size_bytes: n.size,
                created_at: None,
                modified_at: n.modified.clone(),
                inside_archive: true,
                child_count: n.children,
                video: None,
            };
            self.insert(&it)?;
        }
        Ok(())
    }

    /// Conteúdo compactado já gravado (linhas antigas) de um arquivo que não mudou.
    fn old_archive_entries(&self, old_id: i64, archive_path: &str) -> Result<Vec<ArchiveEntry>> {
        let mut st = self.conn.prepare_cached(
            "WITH RECURSIVE sub(id) AS (
                SELECT id FROM items WHERE parent_id=?1
                UNION ALL SELECT i.id FROM items i JOIN sub s ON i.parent_id=s.id
             )
             SELECT path, size_bytes, modified_at, kind FROM items WHERE id IN (SELECT id FROM sub)
             ORDER BY id",
        )?;
        let prefix = format!("{archive_path}/");
        let rows = st.query_map([old_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?;
        let mut out = Vec::new();
        for r in rows {
            let (path, size, modified, kind) = r?;
            let inner = path.strip_prefix(&prefix).unwrap_or(&path).to_string();
            out.push(ArchiveEntry {
                path: inner,
                size_bytes: size as u64,
                modified_at: modified,
                is_dir: kind == "folder",
            });
        }
        Ok(out)
    }
}

/// Monta a árvore (pastas intermediárias, tamanhos recursivos e contagem de filhos).
fn build_tree(entries: &[ArchiveEntry]) -> Vec<Node> {
    let mut nodes: Vec<Node> = Vec::new();
    let mut dirs: HashMap<String, usize> = HashMap::new();

    fn ensure_dir(nodes: &mut Vec<Node>, dirs: &mut HashMap<String, usize>, parts: &[&str]) -> Option<usize> {
        let mut parent: Option<usize> = None;
        let mut acc = String::new();
        for p in parts {
            if !acc.is_empty() {
                acc.push('/');
            }
            acc.push_str(p);
            let idx = match dirs.get(&acc) {
                Some(&i) => i,
                None => {
                    let i = nodes.len();
                    nodes.push(Node {
                        name: (*p).to_string(),
                        rel: acc.clone(),
                        is_dir: true,
                        size: 0,
                        modified: None,
                        parent,
                        children: 0,
                    });
                    if let Some(pi) = parent {
                        nodes[pi].children += 1;
                    }
                    dirs.insert(acc.clone(), i);
                    i
                }
            };
            parent = Some(idx);
        }
        parent
    }

    for e in entries {
        let norm = e.path.replace('\\', "/");
        let parts: Vec<&str> = norm.split('/').filter(|p| !p.is_empty()).collect();
        if parts.is_empty() {
            continue;
        }
        if e.is_dir {
            if let Some(i) = ensure_dir(&mut nodes, &mut dirs, &parts) {
                if nodes[i].modified.is_none() {
                    nodes[i].modified = e.modified_at.clone();
                }
            }
            continue;
        }
        let (name, dir_parts) = parts.split_last().unwrap();
        let parent = ensure_dir(&mut nodes, &mut dirs, dir_parts);
        nodes.push(Node {
            name: (*name).to_string(),
            rel: parts.join("/"),
            is_dir: false,
            size: e.size_bytes,
            modified: e.modified_at.clone(),
            parent,
            children: 0,
        });
        let mut p = parent;
        if let Some(pi) = p {
            nodes[pi].children += 1;
        }
        while let Some(pi) = p {
            nodes[pi].size += e.size_bytes;
            p = nodes[pi].parent;
        }
    }
    nodes
}

/// Lê o disco montado em `mount_path` (descobre uuid/nome/etc. sozinho).
pub fn read_disk(
    catalog: &mut Catalog,
    mount_path: &Path,
    opts: &ReadOptions,
    extractor: &dyn DetailExtractor,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(ReadProgress),
) -> Result<Disk> {
    let vol = crate::volumes::identify(mount_path)?;
    read_disk_as(catalog, &vol, opts, extractor, cancel, progress)
}

/// Igual a [`read_disk`], mas com a identidade do volume já conhecida (útil em testes).
pub fn read_disk_as(
    catalog: &mut Catalog,
    vol: &MountedVolume,
    opts: &ReadOptions,
    extractor: &dyn DetailExtractor,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(ReadProgress),
) -> Result<Disk> {
    let root = PathBuf::from(&vol.mount_path);
    if !root.is_dir() {
        return Err(Error::msg("O disco não está plugado."));
    }
    let root_md = std::fs::metadata(&root)?;
    let is_slash = root == Path::new("/");
    let mut allowed_devs: Vec<u64> = dev_of(&root_md).into_iter().collect();
    if is_slash {
        // No macOS os arquivos do usuário ficam no volume "Data", ligado a "/" por firmlinks.
        if let Ok(m) = std::fs::metadata("/System/Volumes/Data") {
            allowed_devs.extend(dev_of(&m));
        }
    }

    let tx = catalog.conn.transaction()?;
    let now = now_iso();

    // Disco no catálogo (cria ou atualiza as informações).
    let existing: Option<i64> = tx
        .query_row("SELECT id FROM disks WHERE uuid=?1", [&vol.uuid], |r| r.get(0))
        .ok();
    let disk_id = match existing {
        Some(id) => {
            tx.execute(
                "UPDATE disks SET name=?2, filesystem=?3, capacity_bytes=?4, free_bytes=?5 WHERE id=?1",
                params![id, vol.name, vol.filesystem, vol.capacity_bytes as i64, vol.free_bytes as i64],
            )?;
            id
        }
        None => {
            tx.execute(
                "INSERT INTO disks(uuid,name,filesystem,capacity_bytes,free_bytes,last_read_at)
                 VALUES (?1,?2,?3,?4,?5,?6)",
                params![vol.uuid, vol.name, vol.filesystem, vol.capacity_bytes as i64, vol.free_bytes as i64, now],
            )?;
            tx.last_insert_rowid()
        }
    };

    let quick = opts.mode == ReadMode::Quick && existing.is_some();
    let mut old: HashMap<String, OldInfo> = HashMap::new();
    if quick {
        let mut st = tx.prepare(
            "SELECT id, path, size_bytes, modified_at, video_duration, video_width, video_height,
                    video_codec, video_fps, child_count
             FROM items WHERE disk_id=?1 AND inside_archive=0 AND kind IN ('video','archive')",
        )?;
        let rows = st.query_map([disk_id], |r| {
            let v = VideoInfo {
                duration_sec: r.get(4)?,
                width: r.get::<_, Option<i64>>(5)?.map(|x| x as u32),
                height: r.get::<_, Option<i64>>(6)?.map(|x| x as u32),
                codec: r.get(7)?,
                fps: r.get(8)?,
            };
            Ok((
                r.get::<_, String>(1)?,
                OldInfo {
                    id: r.get(0)?,
                    size: r.get::<_, i64>(2)? as u64,
                    modified: r.get(3)?,
                    video: if v.is_empty() { None } else { Some(v) },
                    child_count: r.get::<_, i64>(9)? as u64,
                },
            ))
        })?;
        for r in rows {
            let (p, o) = r?;
            old.insert(p, o);
        }
    } else if existing.is_some() {
        // Releitura completa: descarta tudo o que havia (ainda dentro da transação).
        tx.execute(
            "DELETE FROM items_fts WHERE rowid IN (SELECT id FROM items WHERE disk_id=?1)",
            [disk_id],
        )?;
        tx.execute("DELETE FROM items WHERE disk_id=?1", [disk_id])?;
    }

    let first_new_id: i64 = tx.query_row("SELECT COALESCE(MAX(id),0)+1 FROM items", [], |r| r.get(0))?;

    let (item_count, total_bytes, skipped);
    {
        let conn: &Connection = &tx;
        let mut w = Walker {
            conn,
            ins: conn.prepare(
                "INSERT INTO items(id,disk_id,parent_id,name,path,kind,ext,size_bytes,created_at,modified_at,
                    inside_archive,child_count,video_duration,video_width,video_height,video_codec,video_fps)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)",
            )?,
            ins_fts: conn.prepare("INSERT INTO items_fts(rowid,name_norm) VALUES (?1,?2)")?,
            upd: conn.prepare("UPDATE items SET size_bytes=?1, child_count=?2 WHERE id=?3")?,
            disk_id,
            uuid: vol.uuid.clone(),
            root: root.clone(),
            include_hidden: opts.include_hidden,
            is_slash,
            allowed_devs,
            extractor,
            cancel,
            progress: &mut *progress,
            old,
            next_id: first_new_id,
            seen: 0,
            skipped: 0,
            item_count: 0,
            total_bytes: 0,
            phase: Phase::Listing,
            last_emit: Instant::now() - Duration::from_secs(1),
        };
        w.emit(None, None);
        let root_path = w.root.clone();
        w.walk_dir(&root_path, "", None)?;
        w.phase = Phase::Saving;
        w.emit(None, None);
        item_count = w.item_count;
        total_bytes = w.total_bytes;
        skipped = w.skipped;
    }

    if quick {
        // Troca as linhas antigas pelas novas.
        tx.execute(
            "DELETE FROM items_fts WHERE rowid IN (SELECT id FROM items WHERE disk_id=?1 AND id<?2)",
            params![disk_id, first_new_id],
        )?;
        tx.execute("DELETE FROM items WHERE disk_id=?1 AND id<?2", params![disk_id, first_new_id])?;
    }
    tx.execute(
        "UPDATE disks SET item_count=?2, total_bytes=?3, last_read_at=?4 WHERE id=?1",
        params![disk_id, item_count as i64, total_bytes as i64, now],
    )?;
    tx.commit()?;

    let mut disk = catalog.get_disk(disk_id)?;
    disk.present = true;
    disk.mount_path = Some(vol.mount_path.clone());
    progress(ReadProgress {
        disk_uuid: vol.uuid.clone(),
        phase: Phase::Done,
        items_seen: item_count,
        current_path: None,
        message: Some(if skipped > 0 {
            format!("Leitura concluída. {skipped} itens sem permissão foram ignorados.")
        } else {
            "Leitura concluída.".to_string()
        }),
    });
    Ok(disk)
}
