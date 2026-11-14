//! A Leitura: percorre um disco presente e grava os itens no catálogo.
//!
//! A Leitura NUNCA abre arquivos: só lista pastas e lê atributos (nome, tamanho, datas).
//!
//! Tudo acontece numa única transação: se a Leitura for cancelada ou falhar, o catálogo
//! fica exatamente como estava. Os itens vão direto para o banco (streaming), então
//! discos com milhões de itens não esgotam a memória.
//!
//! Nuvem (ADR 0014): cada thread leitora liga a política "não baixar da nuvem"; itens
//! `SF_DATALESS` ficam fora e pastas de nuvem que sobram vazias são podadas no fim.
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
use crate::fastdir::{self, Entry as DirEntry, ListErr};
use crate::model::*;
use rusqlite::{Connection, Statement, params};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::mpsc::{RecvTimeoutError, SyncSender};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

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

/// O caminho está numa pasta de nuvem (leitura lenta, que espera a rede).
fn is_cloud_path(p: &str) -> bool {
    p.contains("/Library/CloudStorage/") || p.contains("/Library/Mobile Documents/")
}

/// Caminho relativo `rel` (dentro de `root`) está numa pasta de nuvem?
fn is_cloud_rel(root: &str, rel: &str) -> bool {
    let full = if rel.is_empty() { format!("{}/", root.trim_end_matches('/')) } else { format!("{}/{}/", root.trim_end_matches('/'), rel) };
    is_cloud_path(&full)
}

/// Item só na nuvem (arquivo ou pasta `SF_DATALESS`): fora do catálogo, sem contar nem descer.
fn skip_entry(e: &DirEntry) -> bool {
    e.dataless
}

fn ext_of(name: &str) -> Option<String> {
    Path::new(name)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .filter(|e| !e.is_empty())
}

// ---------------------------------------------------------------- Leitura

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

/// Dados de só leitura compartilhados pelas threads leitoras.
struct Shared<'a> {
    disk_id: i64,
    include_hidden: bool,
    is_slash: bool,
    allowed_devs: Vec<u64>,
    cancel: &'a AtomicBool,
    /// Pedido de parada interno (erro ou cancelamento no escritor).
    stop: AtomicBool,
    next_id: &'a AtomicI64,
    skipped: AtomicU64,
    /// Soma do espaço alocado dos arquivos vistos (só para o progresso).
    bytes_seen: &'a AtomicU64,
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

    /// Monta a linha de um arquivo (só com o que a listagem já trouxe; nunca abre o arquivo).
    fn make_file(&self, e: &DirEntry, rel: String, parent: Option<i64>) -> Out {
        let ext = ext_of(&e.name);
        let kind = Kind::from_ext(ext.as_deref());
        Out::File(Item {
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
            child_count: 0,
            descendant_count: 0,
        })
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
    File(Item),
}

/// Resumo de uma pasta já listada.
struct Agg {
    id: i64,
    /// Soma dos arquivos diretos.
    size: u64,
    /// Nº de filhos diretos (arquivos + pastas).
    count: u64,
    /// Nº de arquivos diretos.
    files: u64,
    /// A política da nuvem barrou a listagem (a pasta só existe na nuvem).
    policy_skip: bool,
}

struct Batch {
    rows: Vec<Out>,
    /// Quando a pasta acabou.
    agg: Option<Agg>,
}

/// Lista uma pasta e devolve as pastas filhas a visitar. `false` = parar (receptor sumiu).
fn process_job(sh: &Shared, job: &Job, tx: &SyncSender<Batch>, children: &mut Vec<Job>) -> bool {
    let listing = match fastdir::list_dir_ex(&job.dir) {
        Ok(l) => l,
        Err(ListErr::Policy) => {
            // Pasta só na nuvem: pula em silêncio (não é falha de leitura).
            let Some(id) = job.id else { return true };
            let agg = Agg { id, size: 0, count: 0, files: 0, policy_skip: true };
            return tx.send(Batch { rows: Vec::new(), agg: Some(agg) }).is_ok();
        }
        Err(ListErr::Failed) => {
            sh.skipped.fetch_add(1, Ordering::Relaxed);
            return true;
        }
    };
    sh.skipped.fetch_add(listing.skipped, Ordering::Relaxed);
    let (mut size, mut count, mut files) = (0u64, 0u64, 0u64);
    let mut rows: Vec<Out> = Vec::with_capacity(listing.entries.len().min(BATCH_ROWS));
    for e in &listing.entries {
        if skip_entry(e) {
            continue;
        }
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
                child_count: 0,
                descendant_count: 0,
            }));
            children.push(Job { dir: e.path_in(&job.dir), rel, id: Some(id) });
            count += 1;
        } else {
            rows.push(sh.make_file(e, rel, job.id));
            sh.bytes_seen.fetch_add(e.alloc, Ordering::Relaxed);
            size = size.saturating_add(e.size);
            count += 1;
            files += 1;
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
    let agg = job.id.map(|id| Agg { id, size, count, files, policy_skip: false });
    tx.send(Batch { rows, agg }).is_ok()
}

fn worker(sh: &Shared, q: &Queue, tx: SyncSender<Batch>) {
    // Nunca baixar nada da nuvem para o app (ADR 0014); a política vale por thread.
    fastdir::disable_materialize_this_thread();
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
    /// Arquivos diretos.
    files: u64,
    /// A pasta está numa pasta de nuvem (candidata à poda).
    cloud: bool,
    /// A listagem foi barrada pela política da nuvem (sempre podada).
    policy_skip: bool,
}

/// Poda de pastas vazias: `dirs` = (id, pai, arquivos diretos, é candidata). Devolve os ids
/// a remover (candidatas sem nenhum arquivo embaixo, recursivamente).
fn empty_dirs(dirs: &[(i64, Option<i64>, u64, bool)]) -> HashSet<i64> {
    let mut order: Vec<&(i64, Option<i64>, u64, bool)> = dirs.iter().collect();
    order.sort_unstable_by_key(|d| d.0);
    let mut has_file: HashSet<i64> = HashSet::new();
    let mut dead = HashSet::new();
    // Filho tem id maior que o pai: do maior para o menor, os filhos já foram vistos.
    for &&(id, parent, files, candidate) in order.iter().rev() {
        if files > 0 || has_file.contains(&id) {
            if let Some(p) = parent {
                has_file.insert(p);
            }
        } else if candidate {
            dead.insert(id);
        }
    }
    dead
}

/// A thread escritora: dona da conexão SQLite.
struct Writer<'a> {
    ins: Statement<'a>,
    upd: Statement<'a>,
    del: Statement<'a>,
    uuid: String,
    cancel: &'a AtomicBool,
    progress: &'a mut dyn FnMut(ReadProgress),
    dirs: HashMap<i64, DirAgg>,
    seen: u64,
    item_count: u64,
    total_bytes: u64,
    phase: Phase,
    last_emit: Instant,
    bytes_seen: &'a AtomicU64,
    bytes_expected: u64,
    last_path: Option<String>,
    root_str: String,
}

impl<'a> Writer<'a> {
    fn emit(&mut self, path: Option<&str>, message: Option<String>) {
        self.last_emit = Instant::now();
        if let Some(p) = path {
            self.last_path = Some(p.to_string());
        }
        let in_cloud = self.last_path.as_deref().is_some_and(is_cloud_path);
        (self.progress)(ReadProgress {
            disk_uuid: self.uuid.clone(),
            phase: self.phase,
            items_seen: self.seen,
            current_path: path.map(|s| s.to_string()),
            message,
            bytes_seen: self.bytes_seen.load(Ordering::Relaxed),
            bytes_expected: self.bytes_expected,
            in_cloud,
        });
    }

    fn insert(&mut self, it: &Item) -> Result<()> {
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
            it.child_count as i64,
        ])?;
        Ok(())
    }

    fn apply(&mut self, out: Out) -> Result<()> {
        match out {
            Out::Folder(item) => {
                self.insert(&item)?;
                self.item_count += 1;
                let cloud = is_cloud_rel(&self.root_str, &item.path);
                let d = self.dirs.entry(item.id).or_default();
                d.parent = item.parent_id;
                d.cloud = cloud;
            }
            Out::File(item) => {
                self.insert(&item)?;
                self.item_count += 1;
                self.total_bytes = self.total_bytes.saturating_add(item.size_bytes);
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
                                Out::File(item) => item.path.clone(),
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
                        if let Some(g) = batch.agg {
                            let a = self.dirs.entry(g.id).or_default();
                            a.size = g.size;
                            a.count = g.count;
                            a.files = g.files;
                            a.policy_skip = g.policy_skip;
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

    /// Poda as pastas de nuvem que ficaram sem nenhum arquivo embaixo (e as que a política da
    /// nuvem barrou), antes de somar tamanhos. Ajusta a contagem de filhos dos pais.
    fn prune_empty_cloud_dirs(&mut self) -> Result<()> {
        let flat: Vec<(i64, Option<i64>, u64, bool)> =
            self.dirs.iter().map(|(&id, a)| (id, a.parent, a.files, a.cloud || a.policy_skip)).collect();
        let dead = empty_dirs(&flat);
        if dead.is_empty() {
            return Ok(());
        }
        for &id in &dead {
            if let Some(p) = self.dirs[&id].parent {
                if !dead.contains(&p) {
                    if let Some(pa) = self.dirs.get_mut(&p) {
                        pa.count = pa.count.saturating_sub(1);
                    }
                }
            }
        }
        for &id in &dead {
            self.del.execute([id])?;
        }
        for id in &dead {
            self.dirs.remove(id);
        }
        self.item_count -= dead.len() as u64;
        Ok(())
    }

    /// Soma tamanho e nº de filhos das pastas, de baixo para cima. Um filho sempre tem id
    /// maior que o do pai (o pai nasce antes de a pasta ser listada), então basta andar
    /// os ids do maior para o menor.
    fn fold_dirs(&mut self) -> Result<()> {
        let mut ids: Vec<i64> = self.dirs.keys().copied().collect();
        ids.sort_unstable();
        let mut sub: HashMap<i64, u64> = HashMap::new();
        // Itens dentro de cada pasta (recursivo): filhos diretos + o que há dentro das pastas filhas.
        let mut sub_items: HashMap<i64, u64> = HashMap::new();
        for &id in ids.iter().rev() {
            let a = &self.dirs[&id];
            let total = a.size.saturating_add(sub.remove(&id).unwrap_or(0));
            let inside = a.count + sub_items.remove(&id).unwrap_or(0);
            if let Some(p) = a.parent {
                let e = sub.entry(p).or_default();
                *e = e.saturating_add(total);
                *sub_items.entry(p).or_default() += inside;
            }
            if total != 0 || a.count != 0 {
                self.upd.execute(params![total.min(i64::MAX as u64) as i64, a.count as i64, inside as i64, id])?;
            }
        }
        Ok(())
    }

}

/// Lê o disco montado em `mount_path` (descobre uuid/nome/etc. sozinho).
pub fn read_disk(
    catalog: &mut Catalog,
    mount_path: &Path,
    opts: &ReadOptions,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(ReadProgress),
) -> Result<Disk> {
    let vol = crate::volumes::identify(mount_path)?;
    read_disk_as(catalog, &vol, opts, cancel, progress)
}

/// Igual a [`read_disk`], mas com a identidade do volume já conhecida (útil em testes).
pub fn read_disk_as(
    catalog: &mut Catalog,
    vol: &MountedVolume,
    opts: &ReadOptions,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(ReadProgress),
) -> Result<Disk> {
    read_disk_threads(catalog, vol, opts, cancel, progress, walk_threads())
}

/// Igual a [`read_disk_as`], com o nº de threads leitoras escolhido (testes e medições).
#[doc(hidden)]
pub fn read_disk_threads(
    catalog: &mut Catalog,
    vol: &MountedVolume,
    opts: &ReadOptions,
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
    let r = read_disk_inner(catalog, vol, opts, cancel, progress, threads.max(1));
    let _ = catalog.conn.execute_batch("PRAGMA cache_size=-2000; PRAGMA temp_store=DEFAULT;");
    r
}

fn read_disk_inner(
    catalog: &mut Catalog,
    vol: &MountedVolume,
    opts: &ReadOptions,
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

    // A Leitura nunca abre arquivos, então não há nada caro a reaproveitar: a Atualização rápida
    // e a Releitura completa fazem o mesmo (descartam o que havia do disco e refazem).
    let _ = opts.mode;
    if existing.is_some() {
        tx.execute(
            "DELETE FROM items_fts WHERE rowid IN (SELECT id FROM items WHERE disk_id=?1)",
            [disk_id],
        )?;
        tx.execute("DELETE FROM items WHERE disk_id=?1", [disk_id])?;
    }

    let max_id: i64 = tx.query_row("SELECT COALESCE(MAX(id),0) FROM items", [], |r| r.get(0))?;
    let first_new_id = max_id
        .checked_add(1)
        .ok_or_else(|| crate::error::Error::msg("O catálogo chegou ao limite de itens."))?;
    let next_id = AtomicI64::new(first_new_id);

    let bytes_seen = AtomicU64::new(0);
    let bytes_expected = vol.capacity_bytes.saturating_sub(vol.free_bytes);
    let (item_count, total_bytes, skipped);
    {
        let conn: &Connection = &tx;
        let sh = Shared {
            disk_id,
            include_hidden: opts.include_hidden,
            is_slash,
            allowed_devs,
            cancel,
            stop: AtomicBool::new(false),
            next_id: &next_id,
            skipped: AtomicU64::new(0),
            bytes_seen: &bytes_seen,
        };
        let mut w = Writer {
            ins: conn.prepare(
                "INSERT INTO items(id,disk_id,parent_id,name,path,kind,ext,size_bytes,created_at,modified_at,child_count)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            )?,
            upd: conn.prepare("UPDATE items SET size_bytes=?1, child_count=?2, descendant_count=?3 WHERE id=?4")?,
            del: conn.prepare("DELETE FROM items WHERE id=?1")?,
            uuid: vol.uuid.clone(),
            cancel,
            progress: &mut *progress,
            dirs: HashMap::new(),
            seen: 0,
            item_count: 0,
            total_bytes: 0,
            phase: Phase::Listing,
            last_emit: Instant::now() - Duration::from_secs(1),
            bytes_seen: &bytes_seen,
            bytes_expected,
            last_path: None,
            root_str: vol.mount_path.clone(),
        };
        w.emit(None, None);
        w.run_listing(&sh, &root, threads)?;
        w.prune_empty_cloud_dirs()?;
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
        bytes_seen: bytes_seen.load(Ordering::Relaxed),
        bytes_expected,
        ..ReadProgress::new(vol.uuid.clone(), Phase::Saving, item_count, Some("Montando o índice de busca".into()))
    });
    tx.execute(
        "DELETE FROM items_fts WHERE rowid IN (SELECT id FROM items WHERE disk_id=?1)",
        [disk_id],
    )?;
    // Sem fusões automáticas de segmentos durante a carga em massa (voltam ao normal depois).
    tx.execute("INSERT INTO items_fts(items_fts, rank) VALUES('automerge', 0)", [])?;
    tx.execute(
        "INSERT INTO items_fts(rowid,name_norm) SELECT id, hdf_norm(name) FROM items WHERE disk_id=?1",
        [disk_id],
    )?;
    tx.execute("INSERT INTO items_fts(items_fts, rank) VALUES('automerge', 4)", [])?;
    tx.execute(
        "UPDATE disks SET item_count=?2, total_bytes=?3, last_read_at=?4 WHERE id=?1",
        params![disk_id, item_count as i64, total_bytes.min(i64::MAX as u64) as i64, now],
    )?;
    tx.commit()?;

    let mut disk = catalog.get_disk(disk_id)?;
    disk.present = true;
    disk.mount_path = Some(vol.mount_path.clone());
    progress(ReadProgress {
        bytes_seen: bytes_seen.load(Ordering::Relaxed),
        bytes_expected,
        ..ReadProgress::new(
            vol.uuid.clone(),
            Phase::Done,
            item_count,
            Some(if skipped > 0 {
                format!("Leitura concluída. {skipped} itens sem permissão foram ignorados.")
            } else {
                "Leitura concluída.".to_string()
            }),
        )
    });
    Ok(disk)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shared<'a>(cancel: &'a AtomicBool, next: &'a AtomicI64, devs: Vec<u64>) -> Shared<'a> {
        Shared {
            disk_id: 1,
            include_hidden: false,
            is_slash: false,
            allowed_devs: devs,
            cancel,
            stop: AtomicBool::new(false),
            next_id: next,
            skipped: AtomicU64::new(0),
            bytes_seen: Box::leak(Box::new(AtomicU64::new(0))),
        }
    }

    #[test]
    fn other_volumes_are_not_crossed() {
        let (cancel, next) = (AtomicBool::new(false), AtomicI64::new(1));
        let sh = shared(&cancel, &next, vec![10, 11]);
        assert!(sh.skip_dir("Volumes-x", Some(99)), "outro dispositivo = ponto de montagem");
        assert!(!sh.skip_dir("a", Some(11)));
        assert!(!sh.skip_dir("a", None));
        let mut slash = shared(&cancel, &next, vec![10]);
        slash.is_slash = true;
        assert!(slash.skip_dir("Volumes", Some(10)));
        assert!(!slash.skip_dir("Users", Some(10)));
    }

    fn fake_entry(is_dir: bool, dataless: bool) -> DirEntry {
        DirEntry {
            name: "x".into(),
            raw_name: None,
            is_dir,
            size: 0,
            alloc: 0,
            created: None,
            modified: None,
            dataless,
            dev: None,
        }
    }

    #[test]
    fn dataless_entries_are_skipped() {
        let flags = 0x4000_0060u32; // SF_DATALESS + outros bits, como medido no Google Drive
        assert!(fastdir::flags_dataless(flags));
        assert!(skip_entry(&fake_entry(false, fastdir::flags_dataless(flags))), "arquivo");
        assert!(skip_entry(&fake_entry(true, fastdir::flags_dataless(flags))), "pasta");
        assert!(!skip_entry(&fake_entry(false, fastdir::flags_dataless(0x60))));
        assert!(!skip_entry(&fake_entry(true, false)));
    }

    #[test]
    fn policy_error_maps_to_silent_skip() {
        #[cfg(unix)]
        {
            assert!(fastdir::is_policy_errno(libc::EDEADLK));
            assert!(!fastdir::is_policy_errno(libc::EACCES));
        }
        // Uma pasta barrada pela política vira "pular", não "falha de leitura".
        let (cancel, next) = (AtomicBool::new(false), AtomicI64::new(1));
        let sh = shared(&cancel, &next, vec![]);
        let (tx, rx) = std::sync::mpsc::sync_channel::<Batch>(4);
        let job = Job { dir: PathBuf::from("/nao/existe/mesmo"), rel: "x".into(), id: Some(7) };
        let mut kids = Vec::new();
        assert!(process_job(&sh, &job, &tx, &mut kids));
        assert_eq!(sh.skipped.load(Ordering::Relaxed), 1, "erro comum conta como falha");
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn empty_cloud_chain_is_pruned_but_sibling_with_file_stays() {
        // 1 = nuvem/X (tem filho com arquivo); 2 = vazia; 3 = vazia/sub; 4 = com arquivo.
        // 5 = pasta fora da nuvem, vazia (não é candidata).
        let dirs = vec![
            (1, None, 0, true),
            (2, Some(1), 0, true),
            (3, Some(2), 0, true),
            (4, Some(1), 3, true),
            (5, None, 0, false),
        ];
        let dead = empty_dirs(&dirs);
        assert_eq!(dead, HashSet::from([2, 3]));
    }

    #[test]
    fn cloud_relative_paths() {
        assert!(is_cloud_rel("/Users/x/Library/CloudStorage", "GoogleDrive-a/b"));
        assert!(is_cloud_rel("/Users/x/Library", "CloudStorage/GoogleDrive-a"));
        assert!(is_cloud_rel("/", "Users/x/Library/Mobile Documents/c"));
        assert!(!is_cloud_rel("/Users/x/Library", "Caches/a"));
        assert!(!is_cloud_rel("/Volumes/HD", "Projetos"));
    }

    #[test]
    fn cloud_paths_are_detected() {
        assert!(is_cloud_path("/Users/x/Library/CloudStorage/Dropbox/a"));
        assert!(is_cloud_path("/Users/x/Library/Mobile Documents/com~apple~CloudDocs/a"));
        assert!(!is_cloud_path("/Volumes/HD/Projetos"));
    }
}
