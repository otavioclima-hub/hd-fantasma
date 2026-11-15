//! Botão "Conectar ao Claude" (ADR 0017): liga/desliga o MCP `hd-fantasma` no Claude Desktop
//! (editando o JSON de configuração com cuidado) e no Claude Code (pelo comando `claude mcp`).
//! Nada aqui usa a internet (ADR 0015). Todos os caminhos entram por `Ctx`, para os testes
//! rodarem em pasta temporária sem encostar na configuração real da pessoa.

use serde::Serialize;
use serde_json::{Map, Value, json};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub const SERVER_NAME: &str = "hd-fantasma";
const CLI_TIMEOUT: Duration = Duration::from_secs(15);
const MSG_BAD_JSON: &str = "O arquivo de configuração do Claude está com erro; não mexi nele.";
const MSG_WRITE: &str = "Não consegui gravar a configuração do Claude.";
#[cfg(not(windows))]
const MSG_NOT_INSTALLED: &str = "Antes de conectar, arraste o HD Fantasma para a pasta Aplicativos e abra ele de lá.";
#[cfg(windows)]
const MSG_NOT_INSTALLED: &str = "Antes de conectar, instale o HD Fantasma pelo instalador e abra ele pelo menu Iniciar.";

/// The Claude apps will run whatever sits at the registered path, every time they start. So the
/// path must be a real install: not the mounted installer image or any other removable disk
/// (another disk with the same name could later put a different program there), and not the
/// temporary copy macOS runs when an app is opened straight from Downloads.
fn stable_install(mcp: &Path) -> Result<(), String> {
    let p = mcp.to_string_lossy();
    if p.starts_with("/Volumes/") || p.contains("/AppTranslocation/") {
        return Err(MSG_NOT_INSTALLED.into());
    }
    // Windows: not a temporary folder (opened straight from a zip) and not a removable disk.
    #[cfg(windows)]
    {
        let low = p.to_lowercase().replace('/', "\\");
        let temp = std::env::temp_dir().to_string_lossy().to_lowercase();
        if (!temp.is_empty() && low.starts_with(temp.trim_end_matches('\\'))) || low.contains("\\temp\\")
            || !hdf_core::volumes::is_internal_volume(mcp)
        {
            return Err(MSG_NOT_INSTALLED.into());
        }
    }
    Ok(())
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DesktopStatus {
    pub installed: bool,
    pub connected: bool,
    pub config_path: String,
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodeStatus {
    pub cli_found: bool,
    pub connected: bool,
    pub command: String,
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeStatus {
    pub mcp_path: String,
    pub desktop: DesktopStatus,
    pub code: CodeStatus,
}

/// Onde as coisas ficam. No app real vem do sistema; nos testes, de uma pasta temporária.
pub struct Ctx {
    pub home: PathBuf,
    pub appdata: Option<PathBuf>,
    /// Windows `%LOCALAPPDATA%` (Claude Desktop installed from the Microsoft Store keeps its
    /// configuration under `Packages` there).
    pub localappdata: Option<PathBuf>,
    pub mcp: PathBuf,
}

impl Ctx {
    pub fn from_system() -> Result<Ctx, String> {
        let exe = std::env::current_exe().map_err(|_| "Não encontrei onde o app está instalado.".to_string())?;
        let dir = exe.parent().ok_or("Não encontrei onde o app está instalado.")?;
        let name = if cfg!(windows) { "hd-fantasma-mcp.exe" } else { "hd-fantasma-mcp" };
        Ok(Ctx {
            home: dirs::home_dir().unwrap_or_default(),
            appdata: std::env::var_os("APPDATA").map(PathBuf::from),
            localappdata: std::env::var_os("LOCALAPPDATA").map(PathBuf::from),
            mcp: dir.join(name),
        })
    }

    fn desktop_dir(&self) -> PathBuf {
        if cfg!(windows) {
            // Microsoft Store / MSIX install: the app sees a private copy of AppData\\Roaming.
            if let Some(d) = self.store_desktop_dir() {
                return d;
            }
            self.appdata.clone().unwrap_or_else(|| self.home.join("AppData/Roaming")).join("Claude")
        } else if cfg!(target_os = "macos") {
            self.home.join("Library/Application Support/Claude")
        } else {
            self.home.join(".config/Claude")
        }
    }

    /// `%LOCALAPPDATA%\\Packages\\Claude_<id>\\LocalCache\\Roaming\\Claude`, when it exists.
    fn store_desktop_dir(&self) -> Option<PathBuf> {
        let pk = self.localappdata.clone().unwrap_or_else(|| self.home.join("AppData/Local")).join("Packages");
        let mut found: Vec<PathBuf> = std::fs::read_dir(pk)
            .ok()?
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().to_lowercase().starts_with("claude_"))
            .map(|e| e.path().join("LocalCache").join("Roaming").join("Claude"))
            .filter(|p| p.is_dir())
            .collect();
        found.sort();
        found.into_iter().next()
    }

    pub fn desktop_config(&self) -> PathBuf {
        self.desktop_dir().join("claude_desktop_config.json")
    }

    fn mcp_str(&self) -> String {
        self.mcp.to_string_lossy().into_owned()
    }
}

// ---------------------------------------------------------------- Claude Desktop (JSON)

fn read_config(path: &Path) -> Result<Value, String> {
    match std::fs::read_to_string(path) {
        Ok(s) if s.trim().is_empty() => Ok(json!({ "mcpServers": {} })),
        Ok(s) => match serde_json::from_str::<Value>(&s) {
            Ok(v) if v.is_object() => Ok(v),
            _ => Err(MSG_BAD_JSON.into()),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(json!({ "mcpServers": {} })),
        Err(_) => Err("Não consegui ler a configuração do Claude.".into()),
    }
}

/// Grava em arquivo temporário na mesma pasta e troca de nome (atômico), guardando antes uma cópia.
fn write_config(path: &Path, v: &Value) -> Result<(), String> {
    let dir = path.parent().ok_or(MSG_WRITE)?;
    let mut text = serde_json::to_string_pretty(v).map_err(|_| MSG_WRITE.to_string())?;
    text.push('\n');
    if path.exists() {
        let mut bak = path.as_os_str().to_owned();
        bak.push(".bak-hdfantasma");
        let old = std::fs::read(path).map_err(|_| MSG_WRITE.to_string())?;
        write_new_then_rename(dir, path, &old, &PathBuf::from(bak)).map_err(|_| MSG_WRITE.to_string())?;
    }
    write_new_then_rename(dir, path, text.as_bytes(), path).map_err(|_| MSG_WRITE.to_string())
}

/// Writes `bytes` to a brand-new file with an unpredictable name in `dir` (never following a
/// link someone left there), with the permissions of `like`, then renames it over `dest`.
fn write_new_then_rename(dir: &Path, like: &Path, bytes: &[u8], dest: &Path) -> std::io::Result<()> {
    use std::io::Write;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = dir.join(format!(".claude_desktop_config.json.hdf-tmp-{}-{nanos}", std::process::id()));
    let res = (|| -> std::io::Result<()> {
        // create_new: fails if anything (file or link) already has this name.
        let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        // This file can hold keys of other MCP servers: keep whatever protection it had, and
        // make a brand-new one readable only by its owner.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(like).map(|m| m.permissions().mode() & 0o777).unwrap_or(0o600);
            f.set_permissions(std::fs::Permissions::from_mode(mode))?;
        }
        #[cfg(not(unix))]
        let _ = like; // Windows: the new file inherits the folder's protection (the user's profile).
        f.write_all(bytes)?;
        f.sync_all()?;
        std::fs::rename(&tmp, dest)
    })();
    if res.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    res
}

fn desktop_command(v: &Value) -> Option<&str> {
    v.get("mcpServers")?.get(SERVER_NAME)?.get("command")?.as_str()
}

pub fn desktop_connected(config: &Path, mcp: &str) -> bool {
    read_config(config).map(|v| desktop_command(&v) == Some(mcp)).unwrap_or(false)
}

/// Acrescenta/atualiza só `mcpServers.hd-fantasma`. Já conectado: não reescreve nada.
pub fn desktop_connect(config: &Path, mcp: &str) -> Result<(), String> {
    let mut v = read_config(config)?;
    if desktop_command(&v) == Some(mcp) {
        return Ok(());
    }
    let root = v.as_object_mut().ok_or(MSG_BAD_JSON)?;
    let servers = root.entry("mcpServers").or_insert_with(|| Value::Object(Map::new()));
    let servers = servers.as_object_mut().ok_or(MSG_BAD_JSON)?;
    servers.insert(SERVER_NAME.into(), json!({ "command": mcp }));
    write_config(config, &v)
}

/// Tira só a chave `hd-fantasma`. Sem ela (ou sem arquivo): não faz nada.
pub fn desktop_disconnect(config: &Path) -> Result<(), String> {
    if !config.exists() {
        return Ok(());
    }
    let mut v = read_config(config)?;
    let removed = v
        .get_mut("mcpServers")
        .and_then(|s| s.as_object_mut())
        .map(|s| s.shift_remove(SERVER_NAME).is_some())
        .unwrap_or(false);
    if removed { write_config(config, &v) } else { Ok(()) }
}

// ---------------------------------------------------------------- Claude Code (CLI)

fn version_key(name: &str) -> (u64, u64, u64) {
    let mut it = name.trim_start_matches('v').split('.').map(|p| p.parse::<u64>().unwrap_or(0));
    (it.next().unwrap_or(0), it.next().unwrap_or(0), it.next().unwrap_or(0))
}

/// Acha o executável `claude sem depender do PATH (app de janela tem um PATH mínimo).
pub fn find_claude(ctx: &Ctx) -> Option<PathBuf> {
    let h = &ctx.home;
    let mut cands: Vec<PathBuf> = Vec::new();
    if cfg!(windows) {
        // Native installer, then npm.
        cands.push(h.join(".local").join("bin").join("claude.exe"));
        let ad = ctx.appdata.clone().unwrap_or_else(|| h.join("AppData/Roaming"));
        cands.push(ad.join("npm").join("claude.cmd"));
    } else {
        cands.push(h.join(".claude/local/claude"));
        cands.push(h.join(".local/bin/claude"));
        cands.push(PathBuf::from("/opt/homebrew/bin/claude"));
        cands.push(PathBuf::from("/usr/local/bin/claude"));
        if let Ok(rd) = std::fs::read_dir(h.join(".nvm/versions/node")) {
            let mut vs: Vec<String> = rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
            vs.sort_by_key(|n| std::cmp::Reverse(version_key(n)));
            for v in vs {
                cands.push(h.join(".nvm/versions/node").join(v).join("bin/claude"));
            }
        }
    }
    cands.into_iter().find(|p| p.is_file() && trusted_program(p, &ctx.home))
}

/// Only run a `claude` that belongs to this person (or to the system) and that nobody else
/// can change. Otherwise another account on the Mac could swap it.
#[cfg(unix)]
fn trusted_program(p: &Path, home: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    let Ok(target) = std::fs::canonicalize(p) else { return false };
    let Ok(m) = std::fs::metadata(&target) else { return false };
    let me = std::fs::metadata(home).map(|h| h.uid()).unwrap_or(u32::MAX);
    (m.uid() == me || m.uid() == 0) && m.mode() & 0o022 == 0
}

#[cfg(not(unix))]
fn trusted_program(_p: &Path, _home: &Path) -> bool {
    true
}

struct Out {
    ok: bool,
    stdout: String,
}

/// Roda o `claude` com argumentos separados (sem shell), PATH ampliado e limite de tempo.
fn run_claude(claude: &Path, home: &Path, args: &[&str], timeout: Duration) -> Result<Out, String> {
    let mut paths: Vec<PathBuf> = Vec::new();
    if let Some(d) = claude.parent() {
        paths.push(d.to_path_buf());
    }
    if let Some(p) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&p));
    }
    if !cfg!(windows) {
        for d in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"] {
            paths.push(PathBuf::from(d));
        }
    }
    let mut cmd = Command::new(claude);
    cmd.args(args).current_dir(if home.is_dir() { home } else { Path::new(".") });
    if let Ok(p) = std::env::join_paths(paths) {
        cmd.env("PATH", p);
    }
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // sem janela de console
    }
    let mut child = cmd.spawn().map_err(|_| "Não consegui executar o Claude Code.".to_string())?;
    // Output is collected on side threads that report back through a channel, so a program
    // that leaves something running with our pipes open can never stretch the time limit.
    let drain = |mut s: Box<dyn Read + Send>| {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut b = Vec::new();
            let _ = s.read_to_end(&mut b);
            let _ = tx.send(String::from_utf8_lossy(&b).into_owned());
        });
        rx
    };
    let out_t = child.stdout.take().map(|s| drain(Box::new(s)));
    let err_t = child.stderr.take().map(|s| drain(Box::new(s)));
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if start.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("O Claude Code demorou demais para responder.".into());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(40)),
            Err(_) => return Err("Não consegui executar o Claude Code.".into()),
        }
    };
    let left = timeout.saturating_sub(start.elapsed()).max(Duration::from_millis(200));
    let stdout = out_t.and_then(|r| r.recv_timeout(left).ok()).unwrap_or_default();
    drop(err_t);
    Ok(Out { ok: status.success(), stdout })
}

/// (existe, comando registrado, se der para ler)
fn code_lookup(claude: &Path, home: &Path, timeout: Duration) -> Result<(bool, Option<String>), String> {
    let o = run_claude(claude, home, &["mcp", "get", SERVER_NAME], timeout)?;
    if !o.ok {
        return Ok((false, None));
    }
    let cmd = o.stdout.lines().find_map(|l| l.trim().strip_prefix("Command:").map(|c| c.trim().to_string()));
    Ok((true, cmd))
}

fn code_connected_with(claude: &Path, ctx: &Ctx, timeout: Duration) -> bool {
    match code_lookup(claude, &ctx.home, timeout) {
        Ok((true, Some(c))) => c == ctx.mcp_str(),
        Ok((true, None)) => true, // existe, mas a saída não deixou ler o caminho
        _ => false,
    }
}

fn code_connect_with(claude: &Path, ctx: &Ctx, timeout: Duration) -> Result<(), String> {
    let mcp = ctx.mcp_str();
    match code_lookup(claude, &ctx.home, timeout)? {
        (true, Some(c)) if c == mcp => return Ok(()),
        (true, None) => return Ok(()),
        (true, Some(_)) => {
            let r = run_claude(claude, &ctx.home, &["mcp", "remove", "--scope", "user", SERVER_NAME], timeout)?;
            if !r.ok {
                return Err("O Claude Code não conseguiu trocar a ligação antiga.".into());
            }
        }
        _ => {}
    }
    let r = run_claude(claude, &ctx.home, &["mcp", "add", "--scope", "user", SERVER_NAME, "--", &mcp], timeout)?;
    if r.ok { Ok(()) } else { Err("O Claude Code não conseguiu adicionar a ligação.".into()) }
}

fn code_disconnect_with(claude: &Path, ctx: &Ctx, timeout: Duration) -> Result<(), String> {
    if !code_lookup(claude, &ctx.home, timeout)?.0 {
        return Ok(());
    }
    let r = run_claude(claude, &ctx.home, &["mcp", "remove", "--scope", "user", SERVER_NAME], timeout)?;
    if r.ok { Ok(()) } else { Err("O Claude Code não conseguiu remover a ligação.".into()) }
}

// ---------------------------------------------------------------- API usada pelos comandos

pub fn status(ctx: &Ctx) -> ClaudeStatus {
    status_with(ctx, CLI_TIMEOUT)
}

fn status_with(ctx: &Ctx, timeout: Duration) -> ClaudeStatus {
    let mcp = ctx.mcp_str();
    let cfg = ctx.desktop_config();
    let claude = find_claude(ctx);
    ClaudeStatus {
        desktop: DesktopStatus {
            installed: ctx.desktop_dir().is_dir(),
            connected: desktop_connected(&cfg, &mcp),
            config_path: cfg.to_string_lossy().into_owned(),
        },
        code: CodeStatus {
            cli_found: claude.is_some(),
            connected: claude.as_deref().map(|c| code_connected_with(c, ctx, timeout)).unwrap_or(false),
            // PowerShell may drop a bare `--`, and the path has no options, so Windows goes without it.
            command: if cfg!(windows) {
                format!("claude mcp add --scope user {SERVER_NAME} \"{mcp}\"")
            } else {
                format!("claude mcp add --scope user {SERVER_NAME} -- \"{mcp}\"")
            },
        },
        mcp_path: mcp,
    }
}

/// `target`: "desktop", "code" ou nada (= todos os que existirem).
pub fn connect(ctx: &Ctx, target: Option<&str>) -> Result<ClaudeStatus, String> {
    connect_with(ctx, target, CLI_TIMEOUT)
}

fn connect_with(ctx: &Ctx, target: Option<&str>, timeout: Duration) -> Result<ClaudeStatus, String> {
    stable_install(&ctx.mcp)?;
    let mut first_err: Option<String> = None;
    if matches!(target, None | Some("desktop")) {
        if ctx.desktop_dir().is_dir() {
            if let Err(e) = desktop_connect(&ctx.desktop_config(), &ctx.mcp_str()) {
                first_err.get_or_insert(e);
            }
        } else if target.is_some() {
            first_err.get_or_insert("O Claude Desktop não está instalado.".into());
        }
    }
    if matches!(target, None | Some("code")) {
        match find_claude(ctx) {
            Some(c) => {
                if let Err(e) = code_connect_with(&c, ctx, timeout) {
                    first_err.get_or_insert(e);
                }
            }
            None if target.is_some() => {
                first_err.get_or_insert("Não encontrei o Claude Code neste computador.".into());
            }
            None => {}
        }
    }
    match first_err {
        Some(e) => Err(e),
        None => Ok(status_with(ctx, timeout)),
    }
}

pub fn disconnect(ctx: &Ctx, target: Option<&str>) -> Result<ClaudeStatus, String> {
    disconnect_with(ctx, target, CLI_TIMEOUT)
}

fn disconnect_with(ctx: &Ctx, target: Option<&str>, timeout: Duration) -> Result<ClaudeStatus, String> {
    let mut first_err: Option<String> = None;
    if matches!(target, None | Some("desktop")) {
        if let Err(e) = desktop_disconnect(&ctx.desktop_config()) {
            first_err.get_or_insert(e);
        }
    }
    if matches!(target, None | Some("code")) {
        if let Some(c) = find_claude(ctx) {
            if let Err(e) = code_disconnect_with(&c, ctx, timeout) {
                first_err.get_or_insert(e);
            }
        }
    }
    match first_err {
        Some(e) => Err(e),
        None => Ok(status_with(ctx, timeout)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    const MCP: &str = "/Applications/HD Fantasma.app/Contents/MacOS/hd-fantasma-mcp";

    fn cfg_in(dir: &TempDir) -> PathBuf {
        dir.path().join("claude_desktop_config.json")
    }

    fn parse(p: &Path) -> Value {
        serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
    }

    #[test]
    fn creates_missing_file() {
        let d = TempDir::new().unwrap();
        let c = cfg_in(&d);
        desktop_connect(&c, MCP).unwrap();
        assert_eq!(parse(&c), json!({ "mcpServers": { "hd-fantasma": { "command": MCP } } }));
        assert!(desktop_connected(&c, MCP));
        assert!(!desktop_connected(&c, "/outro"));
    }

    #[test]
    fn preserves_other_keys_order_and_servers() {
        let d = TempDir::new().unwrap();
        let c = cfg_in(&d);
        std::fs::write(
            &c,
            "{\n  \"zeta\": 1,\n  \"mcpServers\": {\n    \"outro\": { \"command\": \"x\", \"args\": [\"a\"] }\n  },\n  \"alpha\": { \"k\": true }\n}",
        )
        .unwrap();
        desktop_connect(&c, MCP).unwrap();
        let text = std::fs::read_to_string(&c).unwrap();
        let v = parse(&c);
        assert_eq!(v["zeta"], 1);
        assert_eq!(v["alpha"]["k"], true);
        assert_eq!(v["mcpServers"]["outro"], json!({ "command": "x", "args": ["a"] }));
        assert_eq!(v["mcpServers"]["hd-fantasma"]["command"], MCP);
        assert!(text.find("zeta").unwrap() < text.find("mcpServers").unwrap());
        assert!(text.find("mcpServers").unwrap() < text.find("alpha").unwrap());
        assert!(text.contains("\n  \"zeta\": 1,"), "indentação de 2 espaços");
        // backup com o conteúdo antigo
        let bak = std::fs::read_to_string(dir_bak(&c)).unwrap();
        assert!(bak.contains("\"outro\"") && !bak.contains("hd-fantasma"));
        // nenhum temporário sobrando
        let n = std::fs::read_dir(d.path()).unwrap().count();
        assert_eq!(n, 2);
    }

    fn dir_bak(c: &Path) -> PathBuf {
        PathBuf::from(format!("{}.bak-hdfantasma", c.display()))
    }

    #[test]
    fn invalid_json_untouched() {
        let d = TempDir::new().unwrap();
        let c = cfg_in(&d);
        let bad = "{ \"mcpServers\": { oops ";
        std::fs::write(&c, bad).unwrap();
        let e = desktop_connect(&c, MCP).unwrap_err();
        assert_eq!(e, MSG_BAD_JSON);
        assert_eq!(desktop_disconnect(&c).unwrap_err(), MSG_BAD_JSON);
        assert_eq!(std::fs::read_to_string(&c).unwrap(), bad);
        assert!(!dir_bak(&c).exists());
        assert!(!desktop_connected(&c, MCP));
        // raiz que não é objeto também é "com erro"
        std::fs::write(&c, "[1,2]").unwrap();
        assert!(desktop_connect(&c, MCP).is_err());
        assert_eq!(std::fs::read_to_string(&c).unwrap(), "[1,2]");
    }

    #[test]
    fn idempotent_and_overwrites_only_own_entry() {
        let d = TempDir::new().unwrap();
        let c = cfg_in(&d);
        std::fs::write(&c, r#"{"mcpServers":{"hd-fantasma":{"command":"/velho"},"b":{"command":"y"}}}"#).unwrap();
        assert!(!desktop_connected(&c, MCP));
        desktop_connect(&c, MCP).unwrap();
        let first = std::fs::read_to_string(&c).unwrap();
        assert_eq!(parse(&c)["mcpServers"]["b"]["command"], "y");
        assert_eq!(parse(&c)["mcpServers"]["hd-fantasma"]["command"], MCP);
        desktop_connect(&c, MCP).unwrap();
        assert_eq!(std::fs::read_to_string(&c).unwrap(), first);
        // o backup guarda o estado anterior à primeira gravação (a segunda chamada não gravou)
        assert!(std::fs::read_to_string(dir_bak(&c)).unwrap().contains("/velho"));
    }

    #[test]
    fn disconnect_removes_only_our_key() {
        let d = TempDir::new().unwrap();
        let c = cfg_in(&d);
        std::fs::write(&c, r#"{"x":1,"mcpServers":{"hd-fantasma":{"command":"a"},"b":{"command":"y"}}}"#).unwrap();
        desktop_disconnect(&c).unwrap();
        assert_eq!(parse(&c), json!({"x":1,"mcpServers":{"b":{"command":"y"}}}));
        let again = std::fs::read_to_string(&c).unwrap();
        desktop_disconnect(&c).unwrap(); // idempotente
        assert_eq!(std::fs::read_to_string(&c).unwrap(), again);
        // arquivo inexistente: nada acontece
        let none = d.path().join("nao-existe.json");
        desktop_disconnect(&none).unwrap();
        assert!(!none.exists());
    }

    #[test]
    fn refuses_to_register_a_path_on_a_removable_disk() {
        let d = TempDir::new().unwrap();
        std::fs::create_dir_all(d.path().join("Library/Application Support/Claude")).unwrap();
        for bad in [
            "/Volumes/HD Fantasma/HD Fantasma.app/Contents/MacOS/hd-fantasma-mcp",
            "/private/var/folders/ab/T/AppTranslocation/1234/d/HD Fantasma.app/Contents/MacOS/hd-fantasma-mcp",
        ] {
            let ctx = Ctx { home: d.path().to_path_buf(), appdata: None, localappdata: None, mcp: PathBuf::from(bad) };
            assert_eq!(connect_with(&ctx, None, Duration::from_secs(5)).unwrap_err(), MSG_NOT_INSTALLED);
            assert!(!ctx.desktop_config().exists(), "gravou a configuração para {bad}");
            // desconectar continua funcionando de qualquer lugar
            assert!(disconnect_with(&ctx, None, Duration::from_secs(5)).is_ok());
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_finds_the_store_install_config() {
        let d = TempDir::new().unwrap();
        let ctx = Ctx {
            home: d.path().to_path_buf(),
            appdata: Some(d.path().join("Roaming")),
            localappdata: Some(d.path().join("Local")),
            mcp: PathBuf::from(r"C:\Users\x\AppData\Local\HD Fantasma\hd-fantasma-mcp.exe"),
        };
        assert_eq!(ctx.desktop_config(), d.path().join("Roaming").join("Claude").join("claude_desktop_config.json"));
        let store = d.path().join("Local/Packages/Claude_pzs8sxrjxfjjc/LocalCache/Roaming/Claude");
        std::fs::create_dir_all(&store).unwrap();
        assert_eq!(ctx.desktop_config(), store.join("claude_desktop_config.json"));
        assert!(stable_install(&ctx.mcp).is_ok());
        let tmp = std::env::temp_dir().join("x").join("hd-fantasma-mcp.exe");
        assert_eq!(stable_install(&tmp).unwrap_err(), MSG_NOT_INSTALLED);
    }

    #[cfg(unix)]
    #[test]
    fn never_follows_links_left_next_to_the_config() {
        let d = TempDir::new().unwrap();
        let c = cfg_in(&d);
        std::fs::write(&c, r#"{"mcpServers":{}}"#).unwrap();
        let victim = d.path().join("outro-arquivo.txt");
        std::fs::write(&victim, "intacto").unwrap();
        std::os::unix::fs::symlink(&victim, dir_bak(&c)).unwrap();
        desktop_connect(&c, MCP).unwrap();
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "intacto");
        assert!(!std::fs::symlink_metadata(dir_bak(&c)).unwrap().file_type().is_symlink());
        assert_eq!(parse(&c)["mcpServers"]["hd-fantasma"]["command"], MCP);
    }

    #[cfg(unix)]
    #[test]
    fn keeps_config_file_private() {
        use std::os::unix::fs::PermissionsExt;
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        let d = TempDir::new().unwrap();
        let c = cfg_in(&d);
        desktop_connect(&c, MCP).unwrap();
        assert_eq!(mode(&c), 0o600, "arquivo novo só o dono lê");
        std::fs::write(&c, r#"{"mcpServers":{"b":{"command":"y","env":{"KEY":"segredo"}}}}"#).unwrap();
        std::fs::set_permissions(&c, std::fs::Permissions::from_mode(0o640)).unwrap();
        desktop_connect(&c, MCP).unwrap();
        assert_eq!(mode(&c), 0o640, "permissão que já existia é mantida");
    }

    #[cfg(unix)]
    mod cli {
        use super::*;
        use std::os::unix::fs::PermissionsExt;

        fn ctx(d: &TempDir) -> Ctx {
            Ctx { home: d.path().to_path_buf(), appdata: None, localappdata: None, mcp: PathBuf::from(MCP) }
        }

        /// `claude` falso: guarda o "comando registrado" em um arquivo ao lado dele.
        fn fake_claude(d: &TempDir, body: &str) -> PathBuf {
            let dir = d.path().join(".local/bin");
            std::fs::create_dir_all(&dir).unwrap();
            let p = dir.join("claude");
            std::fs::write(&p, body).unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
            p
        }

        const STATEFUL: &str = r#"#!/bin/sh
S="$(dirname "$0")/state"
echo "$@" >> "$(dirname "$0")/calls"
case "$2" in
  get) [ -f "$S" ] || exit 1; printf 'hd-fantasma:\n  Scope: User\n  Type: stdio\n  Command: %s\n  Args:\n' "$(cat "$S")";;
  add) printf '%s' "$7" > "$S";;
  remove) rm -f "$S";;
esac
"#;

        #[test]
        fn not_found() {
            let d = TempDir::new().unwrap();
            let s = status_with(&ctx(&d), Duration::from_secs(5));
            assert!(!s.code.cli_found && !s.code.connected);
            assert_eq!(s.code.command, format!("claude mcp add --scope user hd-fantasma -- \"{MCP}\""));
            assert!(!s.desktop.installed);
            assert!(connect_with(&ctx(&d), Some("code"), Duration::from_secs(5)).is_err());
            assert!(connect_with(&ctx(&d), None, Duration::from_secs(5)).is_ok());
        }

        #[test]
        fn connect_disconnect_and_replace_elsewhere() {
            let d = TempDir::new().unwrap();
            let bin = fake_claude(&d, STATEFUL);
            let state = bin.parent().unwrap().join("state");
            let c = ctx(&d);
            let t = Duration::from_secs(10);
            assert!(status_with(&c, t).code.cli_found);
            assert!(!status_with(&c, t).code.connected);
            let s = connect_with(&c, Some("code"), t).unwrap();
            assert!(s.code.connected);
            assert_eq!(std::fs::read_to_string(&state).unwrap(), MCP);
            // idempotente: não chama add de novo
            let calls1 = std::fs::read_to_string(bin.parent().unwrap().join("calls")).unwrap();
            connect_with(&c, Some("code"), t).unwrap();
            let calls2 = std::fs::read_to_string(bin.parent().unwrap().join("calls")).unwrap();
            assert_eq!(calls2.matches("mcp add").count(), calls1.matches("mcp add").count());
            // apontando para outro lugar: não conta como conectado; conectar troca
            std::fs::write(&state, "/outro/lugar").unwrap();
            assert!(!status_with(&c, t).code.connected);
            connect_with(&c, Some("code"), t).unwrap();
            assert_eq!(std::fs::read_to_string(&state).unwrap(), MCP);
            let s = disconnect_with(&c, Some("code"), t).unwrap();
            assert!(!s.code.connected && !state.exists());
        }

        #[test]
        fn skips_a_claude_others_can_change() {
            let d = TempDir::new().unwrap();
            let p = fake_claude(&d, STATEFUL);
            assert!(find_claude(&ctx(&d)).is_some());
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o777)).unwrap();
            assert!(find_claude(&ctx(&d)).is_none());
        }

        #[test]
        fn leftover_child_does_not_stretch_the_timeout() {
            let d = TempDir::new().unwrap();
            fake_claude(&d, "#!/bin/sh\n(sleep 6) &\nexit 0\n");
            let t0 = Instant::now();
            let _ = status_with(&ctx(&d), Duration::from_millis(500));
            assert!(t0.elapsed() < Duration::from_secs(3), "{:?}", t0.elapsed());
        }

        #[test]
        fn timeout_kills() {
            let d = TempDir::new().unwrap();
            fake_claude(&d, "#!/bin/sh\nexec sleep 5\n");
            let t0 = Instant::now();
            let e = connect_with(&ctx(&d), Some("code"), Duration::from_millis(300)).unwrap_err();
            assert!(e.contains("demorou"));
            assert!(t0.elapsed() < Duration::from_secs(3));
        }

        #[test]
        fn nvm_newest_first() {
            let d = TempDir::new().unwrap();
            for v in ["v9.0.0", "v24.22.1", "v24.3.0"] {
                let b = d.path().join(format!(".nvm/versions/node/{v}/bin"));
                std::fs::create_dir_all(&b).unwrap();
                std::fs::write(b.join("claude"), "").unwrap();
            }
            let p = find_claude(&ctx(&d)).unwrap();
            assert!(p.to_string_lossy().contains("v24.22.1"));
        }
    }
}
