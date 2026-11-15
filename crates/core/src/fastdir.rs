//! Listagem rápida de uma pasta (ADR 0012).
//!
//! No macOS usa `getattrlistbulk`: uma chamada traz nome, tipo, tamanho, datas e flags de
//! muitos itens de uma vez (sem um `stat` por arquivo). Em outros sistemas, ou se o sistema
//! de arquivos não suportar a chamada (alguns volumes de rede), cai em `read_dir` +
//! `symlink_metadata`. Nunca segue links simbólicos; só devolve arquivos e pastas comuns.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Bit `SF_DATALESS` de `st_flags`: o item só existe na nuvem (ADR 0014).
pub const SF_DATALESS: u32 = 0x4000_0000;

/// Os flags do item dizem "só na nuvem"?
pub fn flags_dataless(flags: u32) -> bool {
    flags & SF_DATALESS != 0
}

/// `errno` que o macOS devolve ao tentar abrir/listar algo só na nuvem com a política de
/// materialização desligada (`setiopolicy_np(3)`): EDEADLK.
pub fn is_policy_errno(errno: i32) -> bool {
    errno == libc_edeadlk()
}
#[cfg(unix)]
fn libc_edeadlk() -> i32 {
    libc::EDEADLK
}
#[cfg(not(unix))]
fn libc_edeadlk() -> i32 {
    -1
}

/// Liga, NESTA thread, a política do macOS "nunca baixar arquivos/pastas só da nuvem"
/// (`IOPOL_TYPE_VFS_MATERIALIZE_DATALESS_FILES` = OFF). Chamar no início de cada thread
/// leitora. Falha é ignorada (devolve `false`); fora do macOS não faz nada.
pub fn disable_materialize_this_thread() -> bool {
    #[cfg(target_os = "macos")]
    {
        // Valores do SDK (sys/resource.h).
        const IOPOL_TYPE_VFS_MATERIALIZE_DATALESS_FILES: i32 = 3;
        const IOPOL_SCOPE_THREAD: i32 = 1;
        const IOPOL_MATERIALIZE_DATALESS_FILES_OFF: i32 = 1;
        unsafe extern "C" {
            fn setiopolicy_np(iotype: i32, scope: i32, policy: i32) -> i32;
        }
        unsafe {
            setiopolicy_np(
                IOPOL_TYPE_VFS_MATERIALIZE_DATALESS_FILES,
                IOPOL_SCOPE_THREAD,
                IOPOL_MATERIALIZE_DATALESS_FILES_OFF,
            ) == 0
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// Por que uma pasta não foi listada.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListErr {
    /// A política "não baixar da nuvem" barrou (a pasta só existe na nuvem): pular em silêncio.
    Policy,
    /// Não abriu (permissão, sumiu etc.).
    Failed,
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
    /// Só preenchido quando o nome não é UTF-8 válido (para montar o caminho real).
    pub raw_name: Option<OsString>,
    pub is_dir: bool,
    /// Tamanho lógico (só arquivos).
    pub size: u64,
    /// Espaço realmente ocupado no disco (só arquivos; 0 se só existe na nuvem). Só o progresso usa.
    pub alloc: u64,
    pub created: Option<i64>,
    pub modified: Option<i64>,
    /// Arquivo ou pasta que só existe na nuvem (`SF_DATALESS`): fica fora do catálogo (ADR 0014).
    pub dataless: bool,
    /// Id do dispositivo da pasta (só pastas; para não atravessar pontos de montagem).
    pub dev: Option<u64>,
}

impl Entry {
    pub fn path_in(&self, dir: &Path) -> PathBuf {
        match &self.raw_name {
            Some(r) => dir.join(r),
            None => dir.join(&self.name),
        }
    }
}

pub struct Listing {
    pub entries: Vec<Entry>,
    /// Itens que não deu para ler (erro de permissão etc.).
    pub skipped: u64,
}

/// Lista `dir`. `None` = a pasta não pôde ser aberta.
pub fn list_dir(dir: &Path) -> Option<Listing> {
    list_dir_ex(dir).ok()
}

/// Igual a [`list_dir`], dizendo se foi a política da nuvem que barrou.
pub fn list_dir_ex(dir: &Path) -> Result<Listing, ListErr> {
    #[cfg(target_os = "macos")]
    {
        match bulk::list(dir) {
            Ok(l) => return Ok(l),
            Err(bulk::BulkErr::Open(e)) => {
                return Err(if is_policy_errno(e) { ListErr::Policy } else { ListErr::Failed });
            }
            Err(bulk::BulkErr::Unsupported) => {}
        }
    }
    list_std_ex(dir)
}

/// Caminho "portátil": `read_dir` + `symlink_metadata`.
pub fn list_std(dir: &Path) -> Option<Listing> {
    list_std_ex(dir).ok()
}

fn list_std_ex(dir: &Path) -> Result<Listing, ListErr> {
    let rd = std::fs::read_dir(dir).map_err(|e| {
        if e.raw_os_error().is_some_and(is_policy_errno) { ListErr::Policy } else { ListErr::Failed }
    })?;
    let mut entries = Vec::new();
    let mut skipped = 0u64;
    for e in rd {
        let Ok(e) = e else {
            skipped += 1;
            continue;
        };
        let Ok(ft) = e.file_type() else {
            skipped += 1;
            continue;
        };
        if ft.is_symlink() || !(ft.is_dir() || ft.is_file()) {
            continue;
        }
        let Ok(md) = e.metadata() else {
            skipped += 1;
            continue;
        };
        let os = e.file_name();
        let name = os.to_string_lossy().into_owned();
        let raw_name = if os.to_str().is_some() { None } else { Some(os) };
        entries.push(Entry {
            name,
            raw_name,
            is_dir: ft.is_dir(),
            size: if ft.is_dir() { 0 } else { md.len() },
            alloc: if ft.is_dir() || flags_dataless(md_flags(&md)) { 0 } else { md_alloc(&md) },
            created: secs(md.created()),
            modified: secs(md.modified()),
            dataless: flags_dataless(md_flags(&md)),
            dev: if ft.is_dir() { md_dev(&md) } else { None },
        });
    }
    Ok(Listing { entries, skipped })
}

fn secs(t: std::io::Result<std::time::SystemTime>) -> Option<i64> {
    let t = t.ok()?;
    Some(match t.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(e) => -(e.duration().as_secs() as i64),
    })
}

#[cfg(target_os = "macos")]
fn md_flags(md: &std::fs::Metadata) -> u32 {
    use std::os::macos::fs::MetadataExt;
    md.st_flags()
}
/// Windows (OneDrive, Dropbox and other cloud-files providers): an "online-only" file has
/// RECALL_ON_DATA_ACCESS (or the older OFFLINE) attribute; a folder whose list of contents was
/// never downloaded has RECALL_ON_OPEN. Both mean "only in the cloud" (ADR 0014). The
/// attributes come with the folder listing itself, so nothing is opened or downloaded.
#[cfg(windows)]
fn md_flags(md: &std::fs::Metadata) -> u32 {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_OFFLINE: u32 = 0x0000_1000;
    const FILE_ATTRIBUTE_RECALL_ON_OPEN: u32 = 0x0004_0000;
    const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;
    let a = md.file_attributes();
    if a & (FILE_ATTRIBUTE_OFFLINE | FILE_ATTRIBUTE_RECALL_ON_OPEN | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS) != 0 {
        SF_DATALESS
    } else {
        0
    }
}
#[cfg(not(any(target_os = "macos", windows)))]
fn md_flags(_md: &std::fs::Metadata) -> u32 {
    0
}

/// Espaço alocado no disco: `st_blocks * 512` no unix; tamanho lógico nos outros sistemas.
#[cfg(unix)]
fn md_alloc(md: &std::fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    md.blocks() * 512
}
#[cfg(not(unix))]
fn md_alloc(md: &std::fs::Metadata) -> u64 {
    md.len()
}

#[cfg(unix)]
pub fn md_dev(md: &std::fs::Metadata) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    Some(md.dev())
}
#[cfg(not(unix))]
pub fn md_dev(_md: &std::fs::Metadata) -> Option<u64> {
    None
}

#[cfg(target_os = "macos")]
mod bulk {
    use super::{Entry, Listing, md_dev};
    use libc::{
        ATTR_BIT_MAP_COUNT, ATTR_CMN_CRTIME, ATTR_CMN_FLAGS, ATTR_CMN_MODTIME, ATTR_CMN_NAME,
        ATTR_CMN_OBJTYPE, ATTR_CMN_RETURNED_ATTRS, ATTR_FILE_ALLOCSIZE, ATTR_FILE_DATALENGTH, FSOPT_PACK_INVAL_ATTRS,
        attrlist,
    };
    use std::ffi::{CString, OsString};
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    use std::path::Path;

    /// `ATTR_CMN_ERROR`: erro ao ler os atributos de UM item (vem logo depois do conjunto devolvido).
    const ATTR_CMN_ERROR: u32 = 0x2000_0000;
    const VREG: u32 = 1;
    const VDIR: u32 = 2;
    const BUF_SIZE: usize = 256 * 1024;

    pub enum BulkErr {
        /// Não conseguiu abrir a pasta (permissão, sumiu): não adianta tentar de outro jeito.
        Open(i32),
        /// A chamada não funciona neste sistema de arquivos: usar o caminho portátil.
        Unsupported,
    }

    struct Fd(i32);
    impl Drop for Fd {
        fn drop(&mut self) {
            unsafe { libc::close(self.0) };
        }
    }

    fn errno() -> i32 {
        std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
    }

    fn rd32(buf: &[u8], off: usize) -> u32 {
        u32::from_ne_bytes(buf[off..off + 4].try_into().unwrap())
    }
    fn rd64(buf: &[u8], off: usize) -> i64 {
        i64::from_ne_bytes(buf[off..off + 8].try_into().unwrap())
    }

    pub fn list(dir: &Path) -> Result<Listing, BulkErr> {
        let c = CString::new(dir.as_os_str().as_bytes()).map_err(|_| BulkErr::Open(0))?;
        let fd = unsafe { libc::open(c.as_ptr(), libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC) };
        if fd < 0 {
            return Err(BulkErr::Open(errno()));
        }
        let fd = Fd(fd);

        let mut al = attrlist {
            bitmapcount: ATTR_BIT_MAP_COUNT,
            reserved: 0,
            commonattr: ATTR_CMN_RETURNED_ATTRS
                | ATTR_CMN_ERROR
                | ATTR_CMN_NAME
                | ATTR_CMN_OBJTYPE
                | ATTR_CMN_CRTIME
                | ATTR_CMN_MODTIME
                | ATTR_CMN_FLAGS,
            volattr: 0,
            dirattr: 0,
            fileattr: ATTR_FILE_ALLOCSIZE | ATTR_FILE_DATALENGTH,
            forkattr: 0,
        };
        // u64 só para garantir alinhamento do buffer.
        let mut store = vec![0u64; BUF_SIZE / 8];
        let mut entries = Vec::new();
        let mut skipped = 0u64;
        loop {
            let n = unsafe {
                libc::getattrlistbulk(
                    fd.0,
                    &mut al as *mut attrlist as *mut libc::c_void,
                    store.as_mut_ptr() as *mut libc::c_void,
                    BUF_SIZE,
                    FSOPT_PACK_INVAL_ATTRS as u64,
                )
            };
            if n < 0 {
                let e = errno();
                if super::is_policy_errno(e) {
                    return Err(BulkErr::Open(e));
                }
                return Err(BulkErr::Unsupported);
            }
            if n == 0 {
                break;
            }
            let buf: &[u8] =
                unsafe { std::slice::from_raw_parts(store.as_ptr() as *const u8, BUF_SIZE) };
            let mut p = 0usize;
            for _ in 0..n {
                let len = rd32(buf, p) as usize;
                if len < 24 || p + len > BUF_SIZE {
                    return Err(BulkErr::Unsupported);
                }
                match parse(&buf[p..p + len], dir) {
                    Parsed::Entry(e) => entries.push(e),
                    Parsed::Skip => {}
                    Parsed::Failed => skipped += 1,
                    Parsed::Bad => return Err(BulkErr::Unsupported),
                }
                p += len;
            }
        }
        Ok(Listing { entries, skipped })
    }

    enum Parsed {
        Entry(Entry),
        /// Link simbólico, socket etc.
        Skip,
        /// O sistema não conseguiu ler este item.
        Failed,
        /// Registro que não bate com o formato esperado.
        Bad,
    }

    /// Registro: `u32 length`, `attribute_set_t` (5 x u32), o erro (u32, se pedido) e então, na ordem dos bits:
    /// NAME (attrreference_t, 8), OBJTYPE (4), CRTIME (timespec, 16), MODTIME (16),
    /// FLAGS (4), ALLOCSIZE (off_t, 8), DATALENGTH (off_t, 8). Com `FSOPT_PACK_INVAL_ATTRS` o espaço de cada
    /// atributo existe sempre; o conjunto devolvido diz quais valem de fato.
    fn parse(rec: &[u8], dir: &Path) -> Parsed {
        let ret_common = rd32(rec, 4);
        let ret_file = rd32(rec, 4 + 12);
        let mut o = 24usize;
        if ret_common & ATTR_CMN_ERROR != 0 {
            // O campo de erro vem logo depois do conjunto devolvido; diferente de 0 = o
            // sistema não leu este item (e o resto do registro não vale).
            if rec.len() < o + 4 {
                return Parsed::Failed;
            }
            let err = rd32(rec, o) as i32;
            if err != 0 {
                // Política da nuvem barrou este item: ele só existe na nuvem, pula em silêncio.
                return if super::is_policy_errno(err) { Parsed::Skip } else { Parsed::Failed };
            }
            o += 4;
        }
        // NAME
        if ret_common & ATTR_CMN_NAME == 0 || rec.len() < o + 8 + 4 + 32 + 4 + 8 + 8 {
            return Parsed::Bad;
        }
        let name_ref = o;
        let name_off = rd32(rec, o) as i32;
        let name_len = rd32(rec, o + 4) as usize;
        o += 8;
        let objtype = rd32(rec, o);
        o += 4;
        let crtime = rd64(rec, o);
        o += 16;
        let modtime = rd64(rec, o);
        o += 16;
        let flags = rd32(rec, o);
        o += 4;
        // Bits do fileattr em ordem crescente: ALLOCSIZE (0x4) vem antes de DATALENGTH (0x200).
        let alloclen = rd64(rec, o);
        o += 8;
        let datalen = rd64(rec, o);

        let start = name_ref as i64 + name_off as i64;
        if start < 0 || name_len == 0 || start as usize + name_len > rec.len() {
            return Parsed::Bad;
        }
        let bytes = &rec[start as usize..start as usize + name_len];
        let bytes = bytes.strip_suffix(&[0]).unwrap_or(bytes);
        if bytes.is_empty() {
            return Parsed::Bad;
        }
        let is_dir = match objtype {
            VDIR => true,
            VREG => false,
            _ => return Parsed::Skip,
        };
        let (name, raw_name) = match std::str::from_utf8(bytes) {
            Ok(s) => (s.to_string(), None),
            Err(_) => (
                String::from_utf8_lossy(bytes).into_owned(),
                Some(OsString::from_vec(bytes.to_vec())),
            ),
        };
        let dataless = ret_common & ATTR_CMN_FLAGS != 0 && super::flags_dataless(flags);
        let dev = if is_dir && !dataless {
            // O dev da própria pasta (não o da pasta-pai) diz se é um ponto de montagem.
            let full = match &raw_name {
                Some(r) => dir.join(r),
                None => dir.join(&name),
            };
            match std::fs::symlink_metadata(&full) {
                Ok(md) => md_dev(&md),
                Err(_) => return Parsed::Failed,
            }
        } else {
            None
        };
        Parsed::Entry(Entry {
            name,
            raw_name,
            is_dir,
            size: if !is_dir && ret_file & ATTR_FILE_DATALENGTH != 0 { datalen.max(0) as u64 } else { 0 },
            alloc: if !is_dir
                && ret_file & ATTR_FILE_ALLOCSIZE != 0
                && !dataless
            {
                alloclen.max(0) as u64
            } else {
                0
            },
            created: if ret_common & ATTR_CMN_CRTIME != 0 { Some(crtime) } else { None },
            modified: if ret_common & ATTR_CMN_MODTIME != 0 { Some(modtime) } else { None },
            dataless,
            dev,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn dataless_flag_and_policy_errno() {
        assert!(flags_dataless(0x4000_0060));
        assert!(!flags_dataless(0x0000_0060));
        #[cfg(unix)]
        {
            assert!(is_policy_errno(libc::EDEADLK));
            assert!(!is_policy_errno(libc::EACCES));
            assert!(!is_policy_errno(libc::ENOENT));
        }
        // Pasta normal: a política ligada não atrapalha a listagem.
        disable_materialize_this_thread();
        let d = tempfile::tempdir().unwrap();
        assert_eq!(list_dir_ex(d.path()).map(|l| l.entries.len()), Ok(0));
        assert_eq!(list_dir_ex(&d.path().join("nao-existe")).err(), Some(ListErr::Failed));
    }

    fn brief(l: &Listing) -> BTreeMap<String, (bool, u64, Option<i64>, Option<i64>, bool)> {
        l.entries
            .iter()
            .map(|e| (e.name.clone(), (e.is_dir, e.size, e.created, e.modified, e.dataless)))
            .collect()
    }

    /// O caminho rápido (getattrlistbulk no Mac) tem que dar o mesmo resultado que o portátil.
    #[test]
    fn fast_listing_matches_portable_listing() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path();
        std::fs::create_dir(p.join("pasta")).unwrap();
        std::fs::create_dir(p.join("vazia")).unwrap();
        std::fs::write(p.join("a.txt"), vec![0u8; 1234]).unwrap();
        std::fs::write(p.join("Vídeo ção.mov"), vec![0u8; 77]).unwrap();
        std::fs::write(p.join("vazio"), b"").unwrap();
        for i in 0..700 {
            std::fs::write(p.join(format!("arquivo-com-nome-bem-longo-{i:04}.dat")), vec![0u8; i]).unwrap();
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(p.join("a.txt"), p.join("link")).unwrap();
            std::os::unix::fs::symlink(p.join("pasta"), p.join("linkpasta")).unwrap();
        }
        let fast = list_dir(p).unwrap();
        let slow = list_std(p).unwrap();
        assert_eq!(fast.entries.len(), 705);
        assert_eq!(brief(&fast), brief(&slow));
        // O espaço alocado também bate entre os dois caminhos (e não é zero para arquivos com conteúdo).
        let allocs = |l: &Listing| -> BTreeMap<String, u64> { l.entries.iter().map(|e| (e.name.clone(), e.alloc)).collect() };
        assert_eq!(allocs(&fast), allocs(&slow));
        assert!(fast.entries.iter().find(|e| e.name == "a.txt").unwrap().alloc >= 1234);
        assert!(!fast.entries.iter().any(|e| e.name.starts_with("link")), "links são ignorados");
        let dir = fast.entries.iter().find(|e| e.name == "pasta").unwrap();
        assert!(dir.is_dir);
        // Windows has no device id here (junctions and mount folders are skipped as links).
        assert_eq!(dir.dev.is_some(), cfg!(unix));
        assert!(list_dir(&p.join("nao-existe")).is_none());
    }
}
