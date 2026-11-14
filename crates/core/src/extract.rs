//! Extrator real de detalhes: dados de vídeo (ffprobe) e conteúdo compactado (.zip/.rar).
//!
//! Regras: nunca entra em pânico, nunca extrai nada para o disco (só lista) e qualquer
//! falha vira "sem detalhes" — a Leitura continua. Decisões em `docs/adr/0010-*.md`.

use crate::model::{Item, Kind, VideoInfo};
use crate::scan::{ArchiveEntry, DetailExtractor, Details};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Máximo de entradas guardadas por arquivo compactado (evita estourar a memória/banco).
pub const MAX_ARCHIVE_ENTRIES: usize = 50_000;
/// Tempo máximo de um ffprobe antes de ser encerrado.
pub const FFPROBE_TIMEOUT: Duration = Duration::from_secs(15);

/// Extrator usado pelo app. Sem ffprobe, só os compactados são lidos.
pub struct RealExtractor {
    pub ffprobe: Option<PathBuf>,
}

impl RealExtractor {
    pub fn new(ffprobe: Option<PathBuf>) -> Self {
        Self { ffprobe }
    }
}

fn ext_lower(path: &Path) -> Option<String> {
    path.extension().map(|e| e.to_string_lossy().to_lowercase())
}

impl DetailExtractor for RealExtractor {
    fn wants(&self, item: &Item) -> bool {
        if item.inside_archive {
            return false;
        }
        match item.kind {
            Kind::Video => self.ffprobe.is_some(),
            Kind::Archive => matches!(item.ext.as_deref(), Some("zip") | Some("rar")),
            _ => false,
        }
    }

    fn extract(&self, path: &Path) -> Details {
        let ext = ext_lower(path);
        match ext.as_deref() {
            Some("zip") => Details { video: None, archive_entries: list_zip(path) },
            Some("rar") => Details { video: None, archive_entries: list_rar(path) },
            _ => Details {
                video: self.ffprobe.as_deref().and_then(|f| probe_video(f, path)),
                archive_entries: Vec::new(),
            },
        }
    }
}

// ---------------------------------------------------------------- ffprobe

/// Procura o ffprobe: 1) ao lado do executável do app (sidecar do Tauri) ou na pasta de
/// recursos, 2) no PATH, 3) em lugares comuns. Não baixa nada.
pub fn find_ffprobe(resource_dir: Option<&Path>) -> Option<PathBuf> {
    let name = if cfg!(windows) { "ffprobe.exe" } else { "ffprobe" };
    let is_file = |p: &Path| p.is_file();

    // 1) sidecar: o Tauri copia `binaries/ffprobe-<triple>` como `ffprobe` ao lado do executável.
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() {
            dirs.push(d.to_path_buf());
        }
    }
    if let Some(r) = resource_dir {
        dirs.push(r.to_path_buf());
        dirs.push(r.join("binaries"));
    }
    for d in &dirs {
        let c = d.join(name);
        if is_file(&c) {
            return Some(c);
        }
    }

    // 2) PATH
    if let Some(path) = std::env::var_os("PATH") {
        for d in std::env::split_paths(&path) {
            let c = d.join(name);
            if is_file(&c) {
                return Some(c);
            }
        }
    }

    // 3) lugares comuns (apps abertos pelo Finder não herdam o PATH do terminal).
    let mut common: Vec<PathBuf> = vec![
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/usr/bin"),
    ];
    if let Some(home) = dirs::home_dir() {
        common.push(home.join("bin"));
    }
    common.into_iter().map(|d| d.join(name)).find(|c| is_file(c))
}

#[cfg(windows)]
fn hide_window(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
}
#[cfg(not(windows))]
fn hide_window(_cmd: &mut Command) {}

/// Roda o ffprobe e devolve os dados do primeiro stream de vídeo. Qualquer falha → `None`.
pub fn probe_video(ffprobe: &Path, file: &Path) -> Option<VideoInfo> {
    let mut cmd = Command::new(ffprobe);
    cmd.args(["-v", "error", "-print_format", "json", "-show_format", "-show_streams", "-i"])
        .arg(file)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    hide_window(&mut cmd);
    let mut child = cmd.spawn().ok()?;
    let mut stdout = child.stdout.take()?;
    // Lê a saída numa thread para o ffprobe nunca travar com o cano cheio.
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf);
        buf
    });

    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) => {
                if start.elapsed() >= FFPROBE_TIMEOUT {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
                std::thread::sleep(Duration::from_millis(15));
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let out = reader.join().ok()?;
    if !status?.success() {
        return None;
    }
    parse_ffprobe(&out)
}

fn num(v: &serde_json::Value) -> Option<f64> {
    match v {
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
    .filter(|x| x.is_finite())
}

/// "24000/1001" -> 23.976 ; "0/0" -> None.
fn parse_rate(s: &str) -> Option<f64> {
    let (a, b) = match s.split_once('/') {
        Some((a, b)) => (a.trim().parse::<f64>().ok()?, b.trim().parse::<f64>().ok()?),
        None => (s.trim().parse::<f64>().ok()?, 1.0),
    };
    if b == 0.0 || a <= 0.0 {
        return None;
    }
    let r = a / b;
    r.is_finite().then(|| (r * 1000.0).round() / 1000.0)
}

/// Interpreta o JSON do ffprobe (separado para ser testável sem o programa).
pub fn parse_ffprobe(json: &[u8]) -> Option<VideoInfo> {
    let root: serde_json::Value = serde_json::from_slice(json).ok()?;
    let streams = root.get("streams")?.as_array()?;
    let is_video = |s: &&serde_json::Value| s.get("codec_type").and_then(|c| c.as_str()) == Some("video");
    let is_cover = |s: &serde_json::Value| {
        s.pointer("/disposition/attached_pic").and_then(|v| v.as_i64()) == Some(1)
    };
    let video = streams
        .iter()
        .filter(is_video)
        .find(|s| !is_cover(s))
        .or_else(|| streams.iter().find(is_video))?;

    let duration = root
        .pointer("/format/duration")
        .and_then(num)
        .or_else(|| video.get("duration").and_then(num))
        .filter(|d| *d >= 0.0);
    let dim = |k: &str| video.get(k).and_then(|v| v.as_u64()).filter(|x| *x > 0).map(|x| x as u32);
    let fps = ["avg_frame_rate", "r_frame_rate"]
        .iter()
        .filter_map(|k| video.get(*k).and_then(|v| v.as_str()))
        .find_map(parse_rate);
    let info = VideoInfo {
        duration_sec: duration,
        width: dim("width"),
        height: dim("height"),
        codec: video.get("codec_name").and_then(|c| c.as_str()).map(|s| s.to_string()),
        fps,
    };
    (!info.is_empty()).then_some(info)
}

// -------------------------------------------------------------------- zip

/// Lista as entradas de um .zip sem extrair nada. Corrompido → o que der para ler (ou vazio).
pub fn list_zip(path: &Path) -> Vec<ArchiveEntry> {
    std::panic::catch_unwind(|| list_zip_inner(path)).unwrap_or_default()
}

fn list_zip_inner(path: &Path) -> Vec<ArchiveEntry> {
    let Ok(file) = std::fs::File::open(path) else { return Vec::new() };
    let Ok(mut zip) = zip::ZipArchive::new(std::io::BufReader::new(file)) else {
        return Vec::new();
    };
    let n = zip.len().min(MAX_ARCHIVE_ENTRIES);
    let mut out = Vec::with_capacity(n.min(4096));
    for i in 0..n {
        // `by_index_raw` só lê o cabeçalho: não descomprime nem pede senha.
        let Ok(f) = zip.by_index_raw(i) else { continue };
        let name = String::from_utf8_lossy(f.name_raw()).replace('\\', "/");
        let name = name.trim_end_matches('/').to_string();
        if name.is_empty() {
            continue;
        }
        let m = f.last_modified().map(|d| {
            format!(
                "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
                d.year(),
                d.month(),
                d.day(),
                d.hour(),
                d.minute(),
                d.second()
            )
        });
        out.push(ArchiveEntry {
            path: name,
            size_bytes: f.size(),
            modified_at: m,
            is_dir: f.is_dir(),
        });
    }
    out
}

// -------------------------------------------------------------------- rar

/// Data/hora do RAR (formato DOS: data nos 16 bits altos, hora nos baixos) -> ISO.
fn dos_to_iso(t: u32) -> Option<String> {
    let (date, time) = ((t >> 16) as u32, t & 0xFFFF);
    let (year, month, day) = (1980 + (date >> 9), (date >> 5) & 0xF, date & 0x1F);
    let (h, mi, s) = (time >> 11, (time >> 5) & 0x3F, (time & 0x1F) * 2);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || h > 23 || mi > 59 || s > 59 {
        return None;
    }
    Some(format!("{year:04}-{month:02}-{day:02}T{h:02}:{mi:02}:{s:02}Z"))
}

/// Lista as entradas de um .rar sem extrair nada. Com senha nos nomes / corrompido → vazio.
pub fn list_rar(path: &Path) -> Vec<ArchiveEntry> {
    std::panic::catch_unwind(|| list_rar_inner(path)).unwrap_or_default()
}

fn list_rar_inner(path: &Path) -> Vec<ArchiveEntry> {
    let mut out = Vec::new();
    let Ok(mut cursor) = unrar_ng::Archive::new(path).open_for_listing() else {
        return out;
    };
    // Cada item lido já é uma entrada; `skip` não descomprime (modo listagem).
    while let Ok(Some(open)) = cursor.read_header() {
        let h = open.entry();
        if !h.is_split_before() && out.len() < MAX_ARCHIVE_ENTRIES {
            let name = h.filename.to_string_lossy().replace('\\', "/");
            let name = name.trim_end_matches('/').to_string();
            if !name.is_empty() {
                out.push(ArchiveEntry {
                    path: name,
                    size_bytes: h.unpacked_size,
                    modified_at: dos_to_iso(h.file_time),
                    is_dir: h.is_directory(),
                });
            }
        }
        if out.len() >= MAX_ARCHIVE_ENTRIES {
            break;
        }
        match open.skip() {
            Ok(c) => cursor = c,
            Err(_) => break,
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn parses_ffprobe_json() {
        let j = br#"{"streams":[{"codec_type":"audio","codec_name":"aac"},
            {"codec_type":"video","codec_name":"h264","width":1920,"height":1080,
             "avg_frame_rate":"24000/1001","r_frame_rate":"24000/1001","duration":"9.5"}],
            "format":{"duration":"10.250000"}}"#;
        let v = parse_ffprobe(j).unwrap();
        assert_eq!(v.duration_sec, Some(10.25));
        assert_eq!((v.width, v.height), (Some(1920), Some(1080)));
        assert_eq!(v.codec.as_deref(), Some("h264"));
        assert_eq!(v.fps, Some(23.976));
        // sem stream de vídeo / lixo -> None, sem pânico
        assert!(parse_ffprobe(br#"{"streams":[{"codec_type":"audio"}]}"#).is_none());
        assert!(parse_ffprobe(b"nao e json").is_none());
        // avg 0/0 cai para r_frame_rate; duração do stream quando o format não tem
        let j = br#"{"streams":[{"codec_type":"video","codec_name":"prores","width":10,"height":5,
            "avg_frame_rate":"0/0","r_frame_rate":"25/1","duration":"2.0"}],"format":{}}"#;
        let v = parse_ffprobe(j).unwrap();
        assert_eq!((v.fps, v.duration_sec), (Some(25.0), Some(2.0)));
    }

    #[test]
    fn dos_dates() {
        // 2020-05-17 13:45:30
        let date = ((2020 - 1980) << 9) | (5 << 5) | 17;
        let time = (13 << 11) | (45 << 5) | (30 / 2);
        assert_eq!(dos_to_iso((date << 16) | time).as_deref(), Some("2020-05-17T13:45:30Z"));
        assert_eq!(dos_to_iso(0), None);
    }

    #[test]
    fn lists_zip_without_extracting() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("teste.zip");
        {
            let mut w = zip::ZipWriter::new(std::fs::File::create(&p).unwrap());
            let opt = zip::write::SimpleFileOptions::default();
            w.add_directory("cenas/", opt).unwrap();
            w.start_file("cenas/take1.mov", opt).unwrap();
            w.write_all(&[0u8; 300]).unwrap();
            w.start_file("leia-me.txt", opt).unwrap();
            w.write_all(b"oi").unwrap();
            w.finish().unwrap();
        }
        let e = list_zip(&p);
        assert_eq!(e.len(), 3);
        assert!(e.iter().any(|x| x.path == "cenas" && x.is_dir));
        let t = e.iter().find(|x| x.path == "cenas/take1.mov").unwrap();
        assert_eq!((t.size_bytes, t.is_dir), (300, false));
        assert!(t.modified_at.is_some());
        // nada foi extraído
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);

        // zip corrompido / arquivo que não existe -> vazio
        let bad = dir.path().join("ruim.zip");
        std::fs::write(&bad, b"isto nao e um zip").unwrap();
        assert!(list_zip(&bad).is_empty());
        assert!(list_zip(&dir.path().join("nao-existe.zip")).is_empty());
    }

    #[test]
    fn lists_rar_fixtures() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let e = list_rar(&dir.join("version.rar"));
        assert_eq!(e.len(), 1, "{e:?}");
        assert_eq!(e[0].path, "VERSION");
        assert!(e[0].size_bytes > 0);
        assert!(e[0].modified_at.is_some());
        let u = list_rar(&dir.join("unicode.rar"));
        assert_eq!(u.len(), 1, "{u:?}");
        // rar com nomes criptografados ou arquivo lixo -> vazio, sem pânico
        let tmp = tempfile::tempdir().unwrap();
        let junk = tmp.path().join("lixo.rar");
        std::fs::write(&junk, b"nada").unwrap();
        assert!(list_rar(&junk).is_empty());
    }

    fn make_test_video(path: &Path) -> bool {
        Command::new("ffmpeg")
            .args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc=duration=1:size=320x240:rate=25"])
            .args(["-c:v", "libx264", "-pix_fmt", "yuv420p"])
            .arg(path)
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    #[test]
    fn probes_real_video_when_ffprobe_available() {
        let Some(ffprobe) = find_ffprobe(None) else {
            eprintln!("ffprobe não encontrado: teste pulado");
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("t.mp4");
        if !make_test_video(&p) {
            eprintln!("ffmpeg não encontrado: teste pulado");
            return;
        }
        let v = probe_video(&ffprobe, &p).expect("deveria ler o vídeo");
        assert_eq!((v.width, v.height), (Some(320), Some(240)));
        assert_eq!(v.codec.as_deref(), Some("h264"));
        assert_eq!(v.fps, Some(25.0));
        assert!((v.duration_sec.unwrap() - 1.0).abs() < 0.1);

        // arquivo que não é vídeo -> None
        let t = dir.path().join("x.mov");
        std::fs::write(&t, b"lixo").unwrap();
        assert!(probe_video(&ffprobe, &t).is_none());
        // ffprobe inexistente -> None
        assert!(probe_video(Path::new("/nao/existe/ffprobe"), &p).is_none());
    }
}
