//! A Leitura: percorre um disco presente e grava os itens no catálogo.
//!
//! Tudo acontece numa única transação: se a Leitura for cancelada ou falhar, o catálogo
//! fica exatamente como estava. Os itens vão direto para o banco (streaming), então
//! discos com milhões de itens não esgotam a memória: só o que é caro de recalcular
//! (dados de vídeo / conteúdo compactado) é guardado em memória na Atualização rápida.
//!
//! Desenho (ADR 0011 e 0012): várias threads "leitoras" tiram pastas de uma fila
//! compartilhada, listam cada uma (`fastdir::list_dir`) e devolvem as pastas filhas à fila.
//! Os ids dos itens saem de um contador atômico, então o pai já é conhecido quando o filho
//! nasce. Os resultados seguem por um canal limitado até UMA thread escritora (a que chama
//! `read_disk_as`), a única que mexe no SQLite. Tamanho e contagem das pastas são somados
//! no fim, em memória (só as pastas, não os arquivos), e o índice de busca (FTS5) é
//! montado de uma vez depois de tudo.

use crate::db::{Catalog, normalize, now_iso};
use crate::error::{Error, Result};
use crate::fastdir::{self, Entry as DirEntry};
use crate::model::*;
use rusqlite::{Connection, Statement, params};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{RecvTimeoutError, SyncSender};
use std::sync::{Condvar, Mutex};
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

fn ext_of(name: &str) -> Option<String> {
    Path::new(name)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .filter(|e| !e.is_empty())
}

// ---------------------------------------------------------------- Leitura

struct OldInfo {
    id: i64,
    size: u64,
    modified: Option<String>,
    video: Option<VideoInfo>,
    child_count: u64,
}

struct Pending {
    item: Item,
    full: PathBuf,
}

/// Quantos itens o extrator processa ao mesmo tempo (ffprobe / abrir .zip e .rar).
const DETAIL_WORKERS: usize = 4;

/// Itens por lote enviado ao escritor.
const BATCH_ROWS: usize = 2000;

/// Quantas pastas são listadas ao mesmo tempo. Em pastas de nuvem cada listagem espera a
/// rede, então mais threads que núcleos ajudam; `HDF_WALK_THREADS` força outro valor.
fn walk_threads() -> usize {
    if let Some(n) = std::env::var("HDF_WALK_THREADS").ok().and_then(|v| v.parse::<usize>().ok()) {
        return n.clamp(1, 128);
    }
    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    (cores * 2).clamp(4, 16)
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

/// Decide se o extrator caro pode rodar num item. Arquivo que só existe na nuvem
/// (`dataless`) NUNCA passa por ele: ffprobe ou abrir o compactado baixaria o arquivo
/// inteiro (ADR 0011). Vale também na Atualização rápida.
pub(crate) fn should_extract(
    extractor: &dyn DetailExtractor,
    item: &Item,
    dataless: bool,
    reused: bool,
) -> bool {
    !reused && !dataless && extractor.wants(item)
}

/// Dados de só leitura compartilhados pelas threads leitoras.
struct Shared<'a> {
    disk_id: i64,
    include_hidden: bool,
    is_slash: bool,
    allowed_devs: Vec<u64>,
    extractor: &'a dyn DetailExtractor,
    cancel: &'a AtomicBool,
    /// Pedido de parada interno (erro ou cancelamento no escritor).
    stop: AtomicBool,
    old: HashMap<String, OldInfo>,
    next_id: &'a AtomicI64,
    skipped: AtomicU64,
}

impl Shared<'_> {
    fn alloc(&self) -> i64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    fn stopped(&self) -> bool {
        self.stop.load(Ordering::Relaxed) || self.cancel.load(Ordering::Relaxed)
    }

    fn skip_dir(&self, rel: &str, dev: Option<u64>) -> bool {
        if self.is_slash && ROOT_SYSTEM_DIRS.contains(&rel) {
            return true;
        }
        // Não atravessa para outro volume montado.
        if let Some(d) = dev {
            if !self.allowed_devs.is_empty() && !self.allowed_devs.contains(&d) {
                return true;
            }
        }
        false
    }

    /// Monta a linha de um arquivo (na Atualização rápida reaproveita o que não mudou).
    fn make_file(&self, e: &DirEntry, dir: &Path, rel: String, parent: Option<i64>) -> Out {
        let ext = ext_of(&e.name);
        let kind = Kind::from_ext(ext.as_deref());
        let mut item = Item {
            id: self.alloc(),
            disk_id: self.disk_id,
            parent_id: parent,
            name: e.name.clone(),
            path: rel,
            kind,
            ext,
            size_bytes: e.size,
            created_at: e.created.map(iso_from_unix),
            modified_at: e.modified.map(iso_from_unix),
            inside_archive: false,
            child_count: 0,
            video: None,
        };
        let mut reuse_archive = None;
        let mut reused = false;
        if let Some(o) = self.old.get(&item.path) {
            if o.size == item.size_bytes && o.modified == item.modified_at {
                let has_video = o.video.is_some();
                let has_archive = kind == Kind::Archive && o.child_count > 0;
                if has_video || has_archive {
                    reused = true;
                    item.video = o.video.clone();
                    if has_archive {
                        reuse_archive = Some(o.id);
                    }
                }
            }
        }
        let full = if should_extract(self.extractor, &item, e.dataless, reused) {
            Some(e.path_in(dir))
        } else {
            None
        };
        Out::File { item, full, reuse_archive }
    }
}

struct Job {
    dir: PathBuf,
    rel: String,
    /// Id da própria pasta (None = raiz do disco, que não é um item).
    id: Option<i64>,
}

struct QState {
    jobs: VecDeque<Job>,
    /// Pastas sendo listadas agora; a fila só acaba quando está vazia E ninguém trabalha.
    active: usize,
}

struct Queue {
    st: Mutex<QState>,
    cv: Condvar,
}

impl Queue {
    fn lock(&self) -> std::sync::MutexGuard<'_, QState> {
        self.st.lock().unwrap_or_else(|e| e.into_inner())
    }
}

enum Out {
    Folder(Item),
    File { item: Item, full: Option<PathBuf>, reuse_archive: Option<i64> },
}

struct Batch {
    rows: Vec<Out>,
    /// (id da pasta, soma dos arquivos diretos, nº de filhos diretos) quando a pasta acabou.
    agg: Option<(i64, u64, u64)>,
}

/// Lista uma pasta e devolve as pastas filhas a visitar. `false` = parar (receptor sumiu).
fn process_job(sh: &Shared, job: &Job, tx: &SyncSender<Batch>, children: &mut Vec<Job>) -> bool {
    let Some(listing) = fastdir::list_dir(&job.dir) else {
        sh.skipped.fetch_add(1, Ordering::Relaxed);
        return true;
    };
    sh.skipped.fetch_add(listing.skipped, Ordering::Relaxed);
    let (mut size, mut count) = (0u64, 0u64);
    let mut rows: Vec<Out> = Vec::with_capacity(listing.entries.len().min(BATCH_ROWS));
    for e in &listing.entries {
        if !sh.include_hidden && is_ignored(&e.name) {
            continue;
        }
        let rel = if job.rel.is_empty() { e.name.clone() } else { format!("{}/{}", job.rel, e.name) };
        if e.is_dir {
            if sh.skip_dir(&rel, e.dev) {
                continue;
            }
            let id = sh.alloc();
            rows.push(Out::Folder(Item {
                id,
                disk_id: sh.disk_id,
                parent_id: job.id,
                name: e.name.clone(),
                path: rel.clone(),
                kind: Kind::Folder,
                ext: None,
                size_bytes: 0,
                created_at: e.created.map(iso_from_unix),
                modified_at: e.modified.map(iso_from_unix),
                inside_archive: false,
                child_count: 0,
                video: None,
            }));
            children.push(Job { dir: e.path_in(&job.dir), rel, id: Some(id) });
            count += 1;
        } else {
            rows.push(sh.make_file(e, &job.dir, rel, job.id));
            size += e.size;
            count += 1;
        }
        if rows.len() >= BATCH_ROWS {
            if sh.stopped() {
                return false;
            }
            let full = std::mem::replace(&mut rows, Vec::with_capacity(BATCH_ROWS));
            if tx.send(Batch { rows: full, agg: None }).is_err() {
                return false;
            }
        }
    }
    let agg = job.id.map(|id| (id, size, count));
    tx.send(Batch { rows, agg }).is_ok()
}

fn worker(sh: &Shared, q: &Queue, tx: SyncSender<Batch>) {
    loop {
        let job = {
            let mut g = q.lock();
            loop {
                if sh.stopped() {
                    return;
                }
                if let Some(j) = g.jobs.pop_front() {
                    g.active += 1;
                    break j;
                }
                if g.active == 0 {
                    return;
                }
                g = q.cv.wait_timeout(g, Duration::from_millis(100)).unwrap_or_else(|e| e.into_inner()).0;
            }
        };
        let mut children = Vec::new();
        let ok = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            process_job(sh, &job, &tx, &mut children)
        }))
        .unwrap_or_else(|_| {
            sh.skipped.fetch_add(1, Ordering::Relaxed);
            true
        });
        {
            // As pastas filhas só entram na fila DEPOIS de enviadas as linhas desta pasta,
            // então o escritor sempre vê a pasta antes do conteúdo dela.
            let mut g = q.lock();
            g.jobs.extend(children);
            g.active -= 1;
        }
        q.cv.notify_all();
        if !ok {
            return;
        }
    }
}

#[derive(Default)]
struct DirAgg {
    parent: Option<i64>,
    size: u64,
    count: u64,
}

/// A thread escritora: dona da conexão SQLite.
struct Writer<'a> {
    conn: &'a Connection,
    ins: Statement<'a>,
    upd: Statement<'a>,
    disk_id: i64,
    uuid: String,
    extractor: &'a dyn DetailExtractor,
    cancel: &'a AtomicBool,
    progress: &'a mut dyn FnMut(ReadProgress),
    next_id: &'a AtomicI64,
    /// Itens novos/alterados que precisam do trabalho caro; feito em paralelo depois da listagem.
    pending: Vec<Pending>,
    dirs: HashMap<i64, DirAgg>,
    seen: u64,
    item_count: u64,
    total_bytes: u64,
    phase: Phase,
    last_emit: Instant,
}

impl<'a> Writer<'a> {
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
        Ok(())
    }

    fn apply(&mut self, out: Out) -> Result<()> {
        match out {
            Out::Folder(item) => {
                self.insert(&item)?;
                self.item_count += 1;
                self.dirs.entry(item.id).or_default().parent = item.parent_id;
            }
            Out::File { mut item, full, reuse_archive } => {
                let entries = match reuse_archive {
                    Some(oid) => self.old_archive_entries(oid, &item.path)?,
                    None => Vec::new(),
                };
                let nodes = build_tree(&entries);
                item.child_count = nodes.iter().filter(|n| n.parent.is_none()).count() as u64;
                self.insert(&item)?;
                self.item_count += 1;
                self.total_bytes += item.size_bytes;
                if !nodes.is_empty() {
                    self.insert_archive_nodes(&item, &nodes)?;
                }
                if let Some(full) = full {
                    // O trabalho caro roda depois da listagem, em paralelo (ver `run_details`).
                    self.pending.push(Pending { item, full });
                }
            }
        }
        Ok(())
    }

    /// Lista o disco com várias threads; esta thread grava o que chega.
    fn run_listing(&mut self, sh: &Shared, root: &Path, workers: usize) -> Result<()> {
        let q = Queue {
            st: Mutex::new(QState {
                jobs: VecDeque::from([Job { dir: root.to_path_buf(), rel: String::new(), id: None }]),
                active: 0,
            }),
            cv: Condvar::new(),
        };
        let mut outcome: Result<()> = Ok(());
        std::thread::scope(|scope| {
            let (tx, rx) = std::sync::mpsc::sync_channel::<Batch>(workers * 4);
            for _ in 0..workers {
                let tx = tx.clone();
                let q = &q;
                scope.spawn(move || worker(sh, q, tx));
            }
            drop(tx);
            loop {
                match rx.recv_timeout(Duration::from_millis(100)) {
                    Ok(batch) => {
                        if self.cancel.load(Ordering::Relaxed) {
                            outcome = Err(Error::Cancelled);
                            break;
                        }
                        let mut last: Option<String> = None;
                        if let Some(o) = batch.rows.last() {
                            last = Some(match o {
                                Out::Folder(i) => i.path.clone(),
                                Out::File { item, .. } => item.path.clone(),
                            });
                        }
                        self.seen += batch.rows.len() as u64;
                        for out in batch.rows {
                            if let Err(e) = self.apply(out) {
                                outcome = Err(e);
                                break;
                            }
                        }
                        if outcome.is_err() {
                            break;
                        }
                        if let Some((id, size, count)) = batch.agg {
                            let a = self.dirs.entry(id).or_default();
                            a.size = size;
                            a.count = count;
                        }
                        if self.last_emit.elapsed() >= Duration::from_millis(200) {
                            self.emit(last.as_deref(), None);
                        }
                    }
                    Err(RecvTimeoutError::Timeout) => {
                        if self.cancel.load(Ordering::Relaxed) {
                            outcome = Err(Error::Cancelled);
                            break;
                        }
                        if self.last_emit.elapsed() >= Duration::from_millis(200) {
                            self.emit(None, None);
                        }
                    }
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }
            sh.stop.store(true, Ordering::Relaxed);
            q.cv.notify_all();
            drop(rx); // destrava quem estiver esperando para enviar
        });
        if outcome.is_ok() && self.cancel.load(Ordering::Relaxed) {
            outcome = Err(Error::Cancelled);
        }
        outcome
    }

    /// Soma tamanho e nº de filhos das pastas, de baixo para cima. Um filho sempre tem id
    /// maior que o do pai (o pai nasce antes de a pasta ser listada), então basta andar
    /// os ids do maior para o menor.
    fn fold_dirs(&mut self) -> Result<()> {
        let mut ids: Vec<i64> = self.dirs.keys().copied().collect();
        ids.sort_unstable();
        let mut sub: HashMap<i64, u64> = HashMap::new();
        for &id in ids.iter().rev() {
            let a = &self.dirs[&id];
            let total = a.size + sub.remove(&id).unwrap_or(0);
            if let Some(p) = a.parent {
                *sub.entry(p).or_default() += total;
            }
            if total != 0 || a.count != 0 {
                self.upd.execute(params![total as i64, a.count as i64, id])?;
            }
        }
        Ok(())
    }

    /// Roda o extrator nos itens pendentes com um pequeno grupo de threads e grava cada
    /// resultado na mesma transação (só a thread principal mexe no banco).
    fn run_details(&mut self) -> Result<()> {
        let pending = std::mem::take(&mut self.pending);
        let total = pending.len();
        if total == 0 {
            return Ok(());
        }
        self.phase = Phase::Details;
        self.emit(None, Some(format!("Lendo detalhes: 0 de {total}")));

        let extractor = self.extractor;
        let cancel = self.cancel;
        let stop = AtomicBool::new(false);
        let next = AtomicUsize::new(0);
        let workers = DETAIL_WORKERS.min(total);
        let mut outcome: Result<()> = Ok(());

        std::thread::scope(|scope| {
            let (tx, rx) = std::sync::mpsc::sync_channel::<(usize, Details)>(workers * 2);
            for _ in 0..workers {
                let tx = tx.clone();
                let (pending, next, stop) = (&pending, &next, &stop);
                scope.spawn(move || {
                    loop {
                        if stop.load(Ordering::Relaxed) || cancel.load(Ordering::Relaxed) {
                            break;
                        }
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        let Some(p) = pending.get(i) else { break };
                        let d = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            extractor.extract(&p.full)
                        }))
                        .unwrap_or_default();
                        if tx.send((i, d)).is_err() {
                            break;
                        }
                    }
                });
            }
            drop(tx);

            let mut done = 0usize;
            for (i, d) in rx.iter() {
                if cancel.load(Ordering::Relaxed) {
                    outcome = Err(Error::Cancelled);
                    break;
                }
                if let Err(e) = self.apply_details(&pending[i].item, d) {
                    outcome = Err(e);
                    break;
                }
                done += 1;
                if self.last_emit.elapsed() >= Duration::from_millis(200) || done == total {
                    let path = pending[i].item.path.clone();
                    self.emit(Some(&path), Some(format!("Lendo detalhes: {done} de {total}")));
                }
            }
            if outcome.is_ok() && done < total {
                // Os trabalhadores pararam por cancelamento.
                outcome = Err(Error::Cancelled);
            }
            stop.store(true, Ordering::Relaxed);
            drop(rx); // destrava quem estiver esperando para enviar
        });
        outcome
    }

    fn apply_details(&mut self, item: &Item, d: Details) -> Result<()> {
        let video = d.video.filter(|v| !v.is_empty());
        let nodes = build_tree(&d.archive_entries);
        let roots = nodes.iter().filter(|n| n.parent.is_none()).count() as i64;
        if video.is_none() && nodes.is_empty() {
            return Ok(());
        }
        let v = video.as_ref();
        self.conn
            .prepare_cached(
                "UPDATE items SET child_count=?2, video_duration=?3, video_width=?4,
                    video_height=?5, video_codec=?6, video_fps=?7 WHERE id=?1",
            )?
            .execute(params![
                item.id,
                roots,
                v.and_then(|v| v.duration_sec),
                v.and_then(|v| v.width),
                v.and_then(|v| v.height),
                v.and_then(|v| v.codec.clone()),
                v.and_then(|v| v.fps),
            ])?;
        if !nodes.is_empty() {
            self.insert_archive_nodes(item, &nodes)?;
        }
        Ok(())
    }

    fn insert_archive_nodes(&mut self, archive: &Item, nodes: &[Node]) -> Result<()> {
        let base = self.next_id.fetch_add(nodes.len() as i64, Ordering::Relaxed);
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
    read_disk_threads(catalog, vol, opts, extractor, cancel, progress, walk_threads())
}

/// Igual a [`read_disk_as`], com o nº de threads leitoras escolhido (testes e medições).
#[doc(hidden)]
pub fn read_disk_threads(
    catalog: &mut Catalog,
    vol: &MountedVolume,
    opts: &ReadOptions,
    extractor: &dyn DetailExtractor,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(ReadProgress),
    threads: usize,
) -> Result<Disk> {
    // Ajustes do SQLite só durante a leitura em massa (voltam ao normal no fim, com ou sem erro).
    let _ = catalog.conn.execute_batch("PRAGMA cache_size=-262144; PRAGMA temp_store=MEMORY;");
    catalog.conn.create_scalar_function(
        "hdf_norm",
        1,
        rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| Ok(normalize(&ctx.get::<String>(0)?)),
    )?;
    let r = read_disk_inner(catalog, vol, opts, extractor, cancel, progress, threads.max(1));
    let _ = catalog.conn.execute_batch("PRAGMA cache_size=-2000; PRAGMA temp_store=DEFAULT;");
    r
}

fn read_disk_inner(
    catalog: &mut Catalog,
    vol: &MountedVolume,
    opts: &ReadOptions,
    extractor: &dyn DetailExtractor,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(ReadProgress),
    threads: usize,
) -> Result<Disk> {
    let root = PathBuf::from(&vol.mount_path);
    if !root.is_dir() {
        return Err(Error::msg("O disco não está plugado."));
    }
    let root_md = std::fs::metadata(&root)?;
    let is_slash = root == Path::new("/");
    let mut allowed_devs: Vec<u64> = fastdir::md_dev(&root_md).into_iter().collect();
    if is_slash {
        // No macOS os arquivos do usuário ficam no volume "Data", ligado a "/" por firmlinks.
        if let Ok(m) = std::fs::metadata("/System/Volumes/Data") {
            allowed_devs.extend(fastdir::md_dev(&m));
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
    let next_id = AtomicI64::new(first_new_id);

    let (item_count, total_bytes, skipped);
    {
        let conn: &Connection = &tx;
        let sh = Shared {
            disk_id,
            include_hidden: opts.include_hidden,
            is_slash,
            allowed_devs,
            extractor,
            cancel,
            stop: AtomicBool::new(false),
            old,
            next_id: &next_id,
            skipped: AtomicU64::new(0),
        };
        let mut w = Writer {
            conn,
            ins: conn.prepare(
                "INSERT INTO items(id,disk_id,parent_id,name,path,kind,ext,size_bytes,created_at,modified_at,
                    inside_archive,child_count,video_duration,video_width,video_height,video_codec,video_fps)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)",
            )?,
            upd: conn.prepare("UPDATE items SET size_bytes=?1, child_count=?2 WHERE id=?3")?,
            disk_id,
            uuid: vol.uuid.clone(),
            extractor,
            cancel,
            progress: &mut *progress,
            next_id: &next_id,
            pending: Vec::new(),
            dirs: HashMap::new(),
            seen: 0,
            item_count: 0,
            total_bytes: 0,
            phase: Phase::Listing,
            last_emit: Instant::now() - Duration::from_secs(1),
        };
        w.emit(None, None);
        w.run_listing(&sh, &root, threads)?;
        w.run_details()?;
        w.phase = Phase::Saving;
        w.emit(None, Some("Somando o tamanho das pastas".into()));
        w.fold_dirs()?;
        item_count = w.item_count;
        total_bytes = w.total_bytes;
        skipped = sh.skipped.load(Ordering::Relaxed);
    }

    // Índice de busca (ADR 0012): só agora, de uma vez. Apaga as linhas de busca antigas do
    // disco (os itens antigos ainda existem aqui) e refaz tudo com um INSERT ... SELECT.
    progress(ReadProgress {
        disk_uuid: vol.uuid.clone(),
        phase: Phase::Saving,
        items_seen: item_count,
        current_path: None,
        message: Some("Montando o índice de busca".into()),
    });
    tx.execute(
        "DELETE FROM items_fts WHERE rowid IN (SELECT id FROM items WHERE disk_id=?1)",
        [disk_id],
    )?;
    if quick {
        // Troca as linhas antigas pelas novas.
        tx.execute("DELETE FROM items WHERE disk_id=?1 AND id<?2", params![disk_id, first_new_id])?;
    }
    // Sem fusões automáticas de segmentos durante a carga em massa (voltam ao normal depois).
    tx.execute("INSERT INTO items_fts(items_fts, rank) VALUES('automerge', 0)", [])?;
    tx.execute(
        "INSERT INTO items_fts(rowid,name_norm) SELECT id, hdf_norm(name) FROM items WHERE disk_id=?1",
        [disk_id],
    )?;
    tx.execute("INSERT INTO items_fts(items_fts, rank) VALUES('automerge', 4)", [])?;
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

#[cfg(test)]
mod tests {
    use super::*;

    struct Counting {
        wants: AtomicUsize,
        extracts: AtomicUsize,
    }
    impl DetailExtractor for Counting {
        fn wants(&self, item: &Item) -> bool {
            self.wants.fetch_add(1, Ordering::SeqCst);
            matches!(item.kind, Kind::Video | Kind::Archive)
        }
        fn extract(&self, _path: &Path) -> Details {
            self.extracts.fetch_add(1, Ordering::SeqCst);
            Details::default()
        }
    }

    fn entry(name: &str, dataless: bool) -> DirEntry {
        DirEntry {
            name: name.into(),
            raw_name: None,
            is_dir: false,
            size: 123,
            created: Some(0),
            modified: Some(0),
            dataless,
            dev: None,
        }
    }

    fn shared<'a>(
        ex: &'a dyn DetailExtractor,
        cancel: &'a AtomicBool,
        next: &'a AtomicI64,
        devs: Vec<u64>,
    ) -> Shared<'a> {
        Shared {
            disk_id: 1,
            include_hidden: false,
            is_slash: false,
            allowed_devs: devs,
            extractor: ex,
            cancel,
            stop: AtomicBool::new(false),
            old: HashMap::new(),
            next_id: next,
            skipped: AtomicU64::new(0),
        }
    }

    #[test]
    fn dataless_files_never_reach_the_extractor() {
        let ex = Counting { wants: AtomicUsize::new(0), extracts: AtomicUsize::new(0) };
        let (cancel, next) = (AtomicBool::new(false), AtomicI64::new(1));
        let sh = shared(&ex, &cancel, &next, vec![]);
        let dir = Path::new("/x");

        // Só na nuvem: entra no catálogo, mas sem trabalho caro (nem sequer pergunta ao extrator).
        let Out::File { item, full, .. } = sh.make_file(&entry("nuvem.mov", true), dir, "nuvem.mov".into(), None)
        else {
            panic!()
        };
        assert_eq!(item.size_bytes, 123);
        assert!(full.is_none());
        assert_eq!(ex.wants.load(Ordering::SeqCst), 0);

        // Arquivo normal do mesmo tipo: vai para o extrator.
        let Out::File { full, .. } = sh.make_file(&entry("local.mov", false), dir, "local.mov".into(), None)
        else {
            panic!()
        };
        assert_eq!(full.as_deref(), Some(Path::new("/x/local.mov")));
        assert_eq!(ex.wants.load(Ordering::SeqCst), 1);

        // A função de decisão, direto.
        let it = item;
        assert!(!should_extract(&ex, &it, true, false));
        assert!(should_extract(&ex, &it, false, false));
        assert!(!should_extract(&ex, &it, false, true));
        assert_eq!(ex.extracts.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn other_volumes_are_not_crossed() {
        let (ex, cancel, next) = (NoopExtractor, AtomicBool::new(false), AtomicI64::new(1));
        let sh = shared(&ex, &cancel, &next, vec![10, 11]);
        assert!(sh.skip_dir("Volumes-x", Some(99)), "outro dispositivo = ponto de montagem");
        assert!(!sh.skip_dir("a", Some(11)));
        assert!(!sh.skip_dir("a", None));
        let mut slash = shared(&ex, &cancel, &next, vec![10]);
        slash.is_slash = true;
        assert!(slash.skip_dir("Volumes", Some(10)));
        assert!(!slash.skip_dir("Users", Some(10)));
    }
}
