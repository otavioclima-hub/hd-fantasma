//! Discos montados agora (candidatos à Leitura).
//!
//! macOS: entradas de /Volumes mais "/" (disco interno). UUID/formatação/nome vêm do
//! `diskutil info -plist`; tamanhos vêm do `statfs`. Compartilhamentos de rede
//! (smbfs/afpfs/nfs/webdav) não têm VolumeUUID: o id estável é montado a partir de
//! servidor + compartilhamento.
//! Windows: uma entrada por letra de unidade (ver o módulo `win`).

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
    #[cfg(windows)]
    {
        win::is_internal(mount_path)
    }
    #[cfg(not(any(target_os = "macos", windows)))]
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

/// Windows: one entry per drive letter (fixed, removable, optical with a disc, mapped network
/// drive). The id is the volume serial number, which is written on the disk itself (the same
/// on every computer); network drives use server + share. Nothing here reads files: only the
/// volume's own information, and a size/bus query on the device (no data is read).
#[cfg(windows)]
mod win {
    use super::*;
    use std::ffi::c_void;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};

    const DRIVE_REMOVABLE: u32 = 2;
    const DRIVE_FIXED: u32 = 3;
    const DRIVE_REMOTE: u32 = 4;
    const DRIVE_CDROM: u32 = 5;
    const SEM_FAILCRITICALERRORS: u32 = 0x0001;
    const SEM_NOOPENFILEERRORBOX: u32 = 0x8000;
    const FILE_SHARE_READ: u32 = 0x1;
    const FILE_SHARE_WRITE: u32 = 0x2;
    const OPEN_EXISTING: u32 = 3;
    const IOCTL_STORAGE_QUERY_PROPERTY: u32 = 0x002D_1400;
    // STORAGE_BUS_TYPE values that mean "outside the computer".
    const BUS_1394: u32 = 0x4;
    const BUS_USB: u32 = 0x7;
    const BUS_SD: u32 = 0xC;
    const BUS_MMC: u32 = 0xD;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetLogicalDrives() -> u32;
        fn GetDriveTypeW(root: *const u16) -> u32;
        fn GetVolumeInformationW(
            root: *const u16,
            name: *mut u16,
            name_len: u32,
            serial: *mut u32,
            max_component: *mut u32,
            fs_flags: *mut u32,
            fs_name: *mut u16,
            fs_name_len: u32,
        ) -> i32;
        fn GetDiskFreeSpaceExW(dir: *const u16, free_to_caller: *mut u64, total: *mut u64, total_free: *mut u64) -> i32;
        fn GetVolumePathNameW(file: *const u16, out: *mut u16, out_len: u32) -> i32;
        fn SetThreadErrorMode(new_mode: u32, old_mode: *mut u32) -> i32;
        fn CreateFileW(
            name: *const u16,
            access: u32,
            share: u32,
            security: *const c_void,
            disposition: u32,
            flags: u32,
            template: *mut c_void,
        ) -> *mut c_void;
        fn DeviceIoControl(
            dev: *mut c_void,
            code: u32,
            inbuf: *const c_void,
            insize: u32,
            outbuf: *mut c_void,
            outsize: u32,
            returned: *mut u32,
            overlapped: *mut c_void,
        ) -> i32;
        fn CloseHandle(h: *mut c_void) -> i32;
    }

    #[link(name = "mpr")]
    unsafe extern "system" {
        fn WNetGetConnectionW(local: *const u16, remote: *mut u16, len: *mut u32) -> u32;
    }

    fn wide(s: &str) -> Vec<u16> {
        std::ffi::OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
    }

    fn from_wide(buf: &[u16]) -> String {
        let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        std::ffi::OsString::from_wide(&buf[..n]).to_string_lossy().into_owned()
    }

    /// No "insert a disk" system dialogs for empty card readers / DVD drives on this thread.
    struct QuietErrors(u32);
    impl QuietErrors {
        fn new() -> QuietErrors {
            let mut old = 0u32;
            unsafe { SetThreadErrorMode(SEM_FAILCRITICALERRORS | SEM_NOOPENFILEERRORBOX, &mut old) };
            QuietErrors(old)
        }
    }
    impl Drop for QuietErrors {
        fn drop(&mut self) {
            unsafe { SetThreadErrorMode(self.0, std::ptr::null_mut()) };
        }
    }

    /// `C:\`, `\\server\share\` ... -> the root of the volume that holds `path`.
    pub fn volume_root(path: &Path) -> Option<String> {
        let p = path.to_string_lossy();
        let p = p.strip_prefix(r"\\?\UNC\").map(|r| format!(r"\\{r}")).unwrap_or_else(|| {
            p.strip_prefix(r"\\?\").unwrap_or(&p).to_string()
        });
        let w = wide(&p);
        let mut out = vec![0u16; 1024];
        if unsafe { GetVolumePathNameW(w.as_ptr(), out.as_mut_ptr(), out.len() as u32) } == 0 {
            return None;
        }
        let mut r = from_wide(&out);
        if !r.ends_with('\\') {
            r.push('\\');
        }
        Some(r)
    }

    pub fn drive_type(root: &str) -> u32 {
        let w = wide(root);
        unsafe { GetDriveTypeW(w.as_ptr()) }
    }

    /// Mapped drive letter -> `\\server\share` (from the local mapping table; no traffic).
    fn remote_name(root: &str) -> Option<String> {
        if root.starts_with(r"\\") {
            return Some(root.trim_end_matches('\\').to_string());
        }
        let local = wide(root.trim_end_matches('\\'));
        let mut buf = vec![0u16; 1024];
        let mut len = buf.len() as u32;
        if unsafe { WNetGetConnectionW(local.as_ptr(), buf.as_mut_ptr(), &mut len) } != 0 {
            return None;
        }
        Some(from_wide(&buf))
    }

    /// The disk is attached from outside (USB, FireWire, SD card) or is removable media.
    /// Asks the device driver for its description; nothing on the disk is read.
    fn external_device(root: &str) -> Option<bool> {
        let letter = root.chars().next().filter(|c| c.is_ascii_alphabetic() && root.get(1..2) == Some(":"))?;
        let dev = wide(&format!(r"\\.\{letter}:"));
        let h = unsafe {
            CreateFileW(dev.as_ptr(), 0, FILE_SHARE_READ | FILE_SHARE_WRITE, std::ptr::null(), OPEN_EXISTING, 0, std::ptr::null_mut())
        };
        if h.is_null() || h as isize == -1 {
            return None;
        }
        // STORAGE_PROPERTY_QUERY { StorageDeviceProperty, PropertyStandardQuery }
        let query: [u32; 3] = [0, 0, 0];
        let mut out = [0u8; 1024];
        let mut got = 0u32;
        let ok = unsafe {
            DeviceIoControl(
                h,
                IOCTL_STORAGE_QUERY_PROPERTY,
                query.as_ptr() as *const c_void,
                std::mem::size_of_val(&query) as u32,
                out.as_mut_ptr() as *mut c_void,
                out.len() as u32,
                &mut got,
                std::ptr::null_mut(),
            )
        };
        unsafe { CloseHandle(h) };
        if ok == 0 || got < 32 {
            return None;
        }
        // STORAGE_DEVICE_DESCRIPTOR: RemovableMedia at byte 10, BusType (u32) at byte 28.
        let removable = out[10] != 0;
        let bus = u32::from_le_bytes([out[28], out[29], out[30], out[31]]);
        Some(removable || matches!(bus, BUS_USB | BUS_1394 | BUS_SD | BUS_MMC))
    }

    fn system_root() -> String {
        let d = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
        format!("{}\\", d.trim_end_matches('\\'))
    }

    pub fn is_internal(path: &Path) -> bool {
        let _q = QuietErrors::new();
        let Some(root) = volume_root(path) else { return true };
        if root.eq_ignore_ascii_case(&system_root()) {
            return true;
        }
        match drive_type(&root) {
            DRIVE_REMOTE => false,
            DRIVE_REMOVABLE | DRIVE_CDROM => false,
            // Unknown: "internal" (safer).
            _ => !external_device(&root).unwrap_or(false),
        }
    }

    pub fn build(root: &str) -> Option<MountedVolume> {
        let w = wide(root);
        let mut name = vec![0u16; 261];
        let mut fs = vec![0u16; 261];
        let (mut serial, mut maxc, mut flags) = (0u32, 0u32, 0u32);
        let ok = unsafe {
            GetVolumeInformationW(
                w.as_ptr(),
                name.as_mut_ptr(),
                name.len() as u32,
                &mut serial,
                &mut maxc,
                &mut flags,
                fs.as_mut_ptr(),
                fs.len() as u32,
            )
        };
        if ok == 0 {
            return None; // no media, or not ready
        }
        let (mut free, mut total, mut total_free) = (0u64, 0u64, 0u64);
        unsafe { GetDiskFreeSpaceExW(w.as_ptr(), &mut free, &mut total, &mut total_free) };
        let label = from_wide(&name);
        let letter = root.trim_end_matches('\\').to_string();
        let remote = drive_type(root) == DRIVE_REMOTE;
        let (uuid, filesystem) = if remote {
            let r = remote_name(root).unwrap_or_else(|| letter.clone());
            (format!("net:smb:{}", r.to_lowercase()), "smb".to_string())
        } else {
            (format!("win:{serial:08X}"), from_wide(&fs).to_lowercase())
        };
        let shown = if !label.trim().is_empty() {
            label
        } else if remote {
            remote_name(root)
                .and_then(|r| r.rsplit('\\').next().map(str::to_string))
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| format!("Rede ({letter})"))
        } else if root.eq_ignore_ascii_case(&system_root()) {
            format!("Disco Local ({letter})")
        } else {
            format!("Disco ({letter})")
        };
        Some(MountedVolume {
            uuid,
            name: shown,
            mount_path: root.to_string(),
            filesystem,
            capacity_bytes: total,
            free_bytes: total_free,
            cataloged_disk_id: None,
        })
    }

    pub fn list() -> Vec<MountedVolume> {
        let _q = QuietErrors::new();
        let mask = unsafe { GetLogicalDrives() };
        let mut out: Vec<MountedVolume> = Vec::new();
        for i in 0..26u32 {
            if mask & (1 << i) == 0 {
                continue;
            }
            let root = format!("{}:\\", (b'A' + i as u8) as char);
            if !matches!(drive_type(&root), DRIVE_FIXED | DRIVE_REMOVABLE | DRIVE_REMOTE | DRIVE_CDROM) {
                continue;
            }
            if let Some(v) = build(&root) {
                if !out.iter().any(|o| o.uuid == v.uuid) {
                    out.push(v);
                }
            }
        }
        out
    }

    pub fn identify(path: &Path) -> Option<MountedVolume> {
        let _q = QuietErrors::new();
        build(&volume_root(path)?)
    }
}

#[cfg(windows)]
fn platform_volumes() -> Vec<MountedVolume> {
    win::list()
}

#[cfg(windows)]
fn platform_identify(path: &Path) -> Result<MountedVolume> {
    win::identify(path).ok_or_else(|| Error::msg("Não foi possível identificar este disco."))
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

#[cfg(all(test, windows))]
mod win_tests {
    use super::*;

    #[test]
    fn lists_the_system_drive_as_internal() {
        let dir = tempfile::tempdir().unwrap();
        let cat = Catalog::open(dir.path().join("c.sqlite")).unwrap();
        let v = list_mounted_volumes(&cat).unwrap();
        let sys = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
        let c = v.iter().find(|x| x.mount_path.eq_ignore_ascii_case(&format!("{sys}\\"))).expect("system drive");
        assert!(c.uuid.starts_with("win:"), "{c:?}");
        assert!(c.capacity_bytes > 0);
        assert!(is_internal_volume(Path::new(&c.mount_path)));
        // A folder inside the drive identifies the same volume.
        let inside = identify(&dir.path()).unwrap();
        assert!(!inside.uuid.is_empty());
    }
}
