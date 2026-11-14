//! Discos montados agora (candidatos à Leitura).
//!
//! macOS: entradas de /Volumes mais "/" (disco interno). UUID/formatação/nome vêm do
//! `diskutil info -plist`; tamanhos vêm do `statfs`. Compartilhamentos de rede
//! (smbfs/afpfs/nfs/webdav) não têm VolumeUUID: o id estável é montado a partir de
//! servidor + compartilhamento.
//! Windows: etapa 2 (ver `platform_volumes` abaixo).

use crate::db::Catalog;
use crate::error::{Error, Result};
use crate::model::MountedVolume;
use std::path::Path;

/// Lista os discos presentes, marcando `catalogedDiskId` pelo uuid.
pub fn list_mounted_volumes(catalog: &Catalog) -> Result<Vec<MountedVolume>> {
    let mut vols = platform_volumes();
    for v in &mut vols {
        v.cataloged_disk_id = catalog.find_disk_by_uuid(&v.uuid)?.map(|d| d.id);
    }
    Ok(vols)
}

/// Identifica o volume que contém `mount_path` (usado pela Leitura).
/// True when the volume is on a disk inside this computer (the system disk, or any other
/// internal volume), as far as the system can tell. Unknown means "internal": safer.
pub fn is_internal_volume(mount_path: &Path) -> bool {
    #[cfg(target_os = "macos")]
    {
        let canon = std::fs::canonicalize(mount_path).unwrap_or_else(|_| mount_path.to_path_buf());
        if canon == Path::new("/") || canon.starts_with("/System/Volumes") {
            return true;
        }
        match mac::diskutil(&canon) {
            Some(i) => i.internal,
            None => !mac::statfs(&canon).map(|s| mac::is_network_fs(&s.fstype)).unwrap_or(false),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = mount_path;
        true
    }
}

pub fn identify(mount_path: &Path) -> Result<MountedVolume> {
    if !mount_path.is_dir() {
        return Err(Error::msg("O disco não está plugado."));
    }
    platform_identify(mount_path)
}

// ------------------------------------------------------------------- macOS

#[cfg(target_os = "macos")]
mod mac {
    use super::*;
    use std::ffi::{CStr, CString};
    use std::os::unix::ffi::OsStrExt;

    pub struct FsStat {
        pub fstype: String,
        pub from: String,
        pub on: String,
        pub capacity: u64,
        pub free: u64,
    }

    pub fn statfs(path: &Path) -> Option<FsStat> {
        let c = CString::new(path.as_os_str().as_bytes()).ok()?;
        let mut s: libc::statfs = unsafe { std::mem::zeroed() };
        if unsafe { libc::statfs(c.as_ptr(), &mut s) } != 0 {
            return None;
        }
        let cs = |p: *const libc::c_char| unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned();
        let bs = s.f_bsize as u64;
        Some(FsStat {
            fstype: cs(s.f_fstypename.as_ptr()),
            from: cs(s.f_mntfromname.as_ptr()),
            on: cs(s.f_mntonname.as_ptr()),
            capacity: s.f_blocks * bs,
            free: s.f_bavail * bs,
        })
    }

    pub struct DiskutilInfo {
        pub uuid: Option<String>,
        pub fs: Option<String>,
        pub name: Option<String>,
        /// The system says the volume lives on a disk inside the computer.
        pub internal: bool,
    }

    pub fn diskutil(path: &Path) -> Option<DiskutilInfo> {
        let out = std::process::Command::new("/usr/sbin/diskutil")
            .args(["info", "-plist"])
            .arg(path)
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let v = plist::Value::from_reader(std::io::Cursor::new(out.stdout)).ok()?;
        let d = v.as_dictionary()?;
        let s = |k: &str| d.get(k).and_then(|x| x.as_string()).map(|x| x.to_string());
        Some(DiskutilInfo {
            uuid: s("VolumeUUID"),
            fs: s("FilesystemType"),
            name: s("VolumeName"),
            internal: ["Internal", "OSInternalMedia"]
                .iter()
                .any(|k| d.get(*k).and_then(|x| x.as_boolean()).unwrap_or(false)),
        })
    }

    pub fn is_network_fs(t: &str) -> bool {
        matches!(t, "smbfs" | "afpfs" | "nfs" | "webdav" | "cifs" | "ftp")
    }

    /// `//user@server/share` -> `net:smbfs://server/share` (sem usuário, estável).
    pub fn network_id(fstype: &str, from: &str) -> String {
        let mut f = from.trim().to_string();
        if let Some(rest) = f.strip_prefix("//") {
            let rest = rest.split_once('@').map(|(_, r)| r).unwrap_or(rest);
            f = format!("//{rest}");
        } else if let Some((scheme, rest)) = f.split_once("://") {
            let rest = rest.split_once('@').map(|(_, r)| r).unwrap_or(rest);
            f = format!("{scheme}://{rest}");
        }
        format!("net:{fstype}:{}", f.to_lowercase())
    }

    pub fn build(path: &Path) -> Option<MountedVolume> {
        let st = statfs(path)?;
        let fallback_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Macintosh HD".into());
        let (uuid, filesystem, name);
        if is_network_fs(&st.fstype) {
            uuid = network_id(&st.fstype, &st.from);
            filesystem = st.fstype.clone();
            name = fallback_name;
        } else {
            let info = diskutil(path)?;
            // Sem VolumeUUID e fora de rede: não dá para identificar com segurança.
            uuid = info.uuid?;
            filesystem = info.fs.unwrap_or(st.fstype.clone()).to_lowercase();
            name = info.name.filter(|n| !n.is_empty()).unwrap_or(fallback_name);
        }
        Some(MountedVolume {
            uuid,
            name,
            mount_path: path.to_string_lossy().into_owned(),
            filesystem,
            capacity_bytes: st.capacity,
            free_bytes: st.free,
            cataloged_disk_id: None,
        })
    }
}

#[cfg(target_os = "macos")]
fn platform_volumes() -> Vec<MountedVolume> {
    let mut out = Vec::new();
    if let Some(mut root) = mac::build(Path::new("/")) {
        if root.name.is_empty() || root.name == "/" {
            root.name = "Macintosh HD".into();
        }
        out.push(root);
    }
    let Ok(rd) = std::fs::read_dir("/Volumes") else {
        return out;
    };
    for e in rd.flatten() {
        let Ok(canon) = std::fs::canonicalize(e.path()) else { continue };
        // O link "Macintosh HD" -> "/" e pastas que não são ponto de montagem ficam de fora.
        if canon == Path::new("/") {
            continue;
        }
        let Some(st) = mac::statfs(&canon) else { continue };
        if Path::new(&st.on) != canon {
            continue;
        }
        if matches!(st.fstype.as_str(), "autofs" | "devfs") || st.from.starts_with("com.apple.TimeMachine") {
            continue;
        }
        if let Some(v) = mac::build(&canon) {
            if !out.iter().any(|o| o.uuid == v.uuid) {
                out.push(v);
            }
        }
    }
    out
}

#[cfg(target_os = "macos")]
fn platform_identify(path: &Path) -> Result<MountedVolume> {
    let canon = std::fs::canonicalize(path)?;
    // Usa o ponto de montagem real, mesmo que o usuário tenha passado uma subpasta.
    let on = mac::statfs(&canon)
        .map(|s| s.on)
        .ok_or_else(|| Error::msg("Não foi possível ler as informações do disco."))?;
    mac::build(Path::new(&on))
        .ok_or_else(|| Error::msg("Não foi possível identificar este disco (sem identificador único)."))
}

// ----------------------------------------------------------------- Windows

/// ETAPA 2: Windows ainda não implementado. Plano: enumerar letras com
/// `GetLogicalDrives`, número de série do volume com `GetVolumeInformationW`,
/// capacidade com `GetDiskFreeSpaceExW` e compartilhamentos via `WNetGetConnectionW`.
/// Por enquanto devolve lista vazia (o app abre, mas não lê discos).
#[cfg(windows)]
fn platform_volumes() -> Vec<MountedVolume> {
    Vec::new()
}

#[cfg(windows)]
fn platform_identify(_path: &Path) -> Result<MountedVolume> {
    Err(Error::msg("A leitura de discos no Windows ainda não está disponível."))
}

// ------------------------------------------------------- outros (Linux etc.)

#[cfg(not(any(target_os = "macos", windows)))]
fn platform_volumes() -> Vec<MountedVolume> {
    Vec::new()
}

#[cfg(not(any(target_os = "macos", windows)))]
fn platform_identify(_path: &Path) -> Result<MountedVolume> {
    Err(Error::msg("Este sistema ainda não é suportado."))
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn network_id_is_stable_and_has_no_user() {
        let a = mac::network_id("smbfs", "//otavio@NAS._smb._tcp.local/Projetos");
        let b = mac::network_id("smbfs", "//outro@nas._smb._tcp.local/projetos");
        assert_eq!(a, b);
        assert!(!a.contains('@'));
    }

    #[test]
    fn lists_internal_disk() {
        let dir = tempfile::tempdir().unwrap();
        let cat = Catalog::open(dir.path().join("c.sqlite")).unwrap();
        let v = list_mounted_volumes(&cat).unwrap();
        assert!(v.iter().any(|x| x.mount_path == "/" && !x.uuid.is_empty()), "{v:?}");
    }
}
