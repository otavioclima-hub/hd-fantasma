//! App Tauri do HD Fantasma: comandos finos que chamam o motor (`hdf_core`).
//! Contrato: docs/CONTRATO.md. Erros voltam como texto em pt-BR.

mod claude;

use hdf_core::*;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter, State};

struct AppState {
    catalog_path: PathBuf,
    cancel: Arc<AtomicBool>,
    reading: Arc<AtomicBool>,
}

type CmdResult<T> = std::result::Result<T, String>;

fn err(e: hdf_core::Error) -> String {
    e.to_string()
}

fn open(state: &AppState) -> CmdResult<Catalog> {
    Catalog::open(&state.catalog_path).map_err(err)
}

/// Roda trabalho de banco/disco fora da thread da janela.
async fn blocking<T, F>(f: F) -> CmdResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> CmdResult<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|_| "Erro interno do aplicativo.".to_string())?
}

#[tauri::command]
async fn list_disks(state: State<'_, AppState>) -> CmdResult<Vec<Disk>> {
    let path = state.catalog_path.clone();
    blocking(move || {
        let cat = Catalog::open(path).map_err(err)?;
        let mounted = volumes::list_mounted_volumes(&cat).map_err(err)?;
        cat.list_disks(&mounted).map_err(err)
    })
    .await
}

#[tauri::command]
async fn list_mounted_volumes(state: State<'_, AppState>) -> CmdResult<Vec<MountedVolume>> {
    let path = state.catalog_path.clone();
    blocking(move || {
        let cat = Catalog::open(path).map_err(err)?;
        volumes::list_mounted_volumes(&cat).map_err(err)
    })
    .await
}

#[tauri::command]
async fn read_disk(
    app: AppHandle,
    state: State<'_, AppState>,
    mount_path: String,
    options: ReadOptions,
) -> CmdResult<Disk> {
    if state.reading.swap(true, Ordering::SeqCst) {
        return Err("Já existe uma leitura em andamento.".into());
    }
    state.cancel.store(false, Ordering::SeqCst);
    let (path, cancel, reading) = (state.catalog_path.clone(), state.cancel.clone(), state.reading.clone());
    let result = blocking(move || {
        let mut cat = Catalog::open(path).map_err(err)?;
        let vol = volumes::identify(std::path::Path::new(&mount_path)).map_err(err)?;
        let uuid = vol.uuid.clone();
        let emitter = app.clone();
        let r = read_disk_as(&mut cat, &vol, &options, &cancel, &mut |p| {
            let _ = emitter.emit("read-progress", p);
        });
        match r {
            Ok(d) => Ok(d),
            Err(e) => {
                let _ = app.emit(
                    "read-progress",
                    ReadProgress::new(uuid, Phase::Error, 0, Some(e.to_string())),
                );
                Err(err(e))
            }
        }
    })
    .await;
    reading.store(false, Ordering::SeqCst);
    result
}

#[tauri::command]
fn cancel_read(state: State<'_, AppState>) {
    state.cancel.store(true, Ordering::SeqCst);
}

#[tauri::command]
async fn list_children(state: State<'_, AppState>, disk_id: i64, parent_id: Option<i64>) -> CmdResult<Vec<Item>> {
    let s = open(&state)?;
    blocking(move || s.list_children(disk_id, parent_id).map_err(err)).await
}

#[tauri::command]
async fn get_item(state: State<'_, AppState>, id: i64) -> CmdResult<Item> {
    let s = open(&state)?;
    blocking(move || s.get_item(id).map_err(err)).await
}

#[tauri::command]
async fn search(state: State<'_, AppState>, query: SearchQuery) -> CmdResult<SearchResult> {
    let s = open(&state)?;
    blocking(move || s.search(&query).map_err(err)).await
}

#[tauri::command]
async fn find_duplicates(
    state: State<'_, AppState>,
    disk_ids: Vec<i64>,
    min_bytes: u64,
) -> CmdResult<Vec<DuplicateGroup>> {
    let s = open(&state)?;
    blocking(move || s.find_duplicates(&disk_ids, min_bytes).map_err(err)).await
}

#[tauri::command]
async fn rename_disk(state: State<'_, AppState>, disk_id: i64, name: String) -> CmdResult<()> {
    let s = open(&state)?;
    blocking(move || s.rename_disk(disk_id, &name).map_err(err)).await
}

#[tauri::command]
async fn delete_disk(state: State<'_, AppState>, disk_id: i64) -> CmdResult<()> {
    let mut s = open(&state)?;
    blocking(move || s.delete_disk(disk_id).map_err(err)).await
}

#[tauri::command]
async fn export_disk(state: State<'_, AppState>, disk_id: i64, dest_path: String) -> CmdResult<()> {
    let s = open(&state)?;
    blocking(move || s.export_disk(disk_id, dest_path).map_err(err)).await
}

#[tauri::command]
async fn import_disk(state: State<'_, AppState>, src_path: String) -> CmdResult<Disk> {
    let mut s = open(&state)?;
    blocking(move || s.import_disk(src_path).map_err(err)).await
}

#[tauri::command]
async fn reveal_item(state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    let s = open(&state)?;
    blocking(move || {
        let mounted = volumes::list_mounted_volumes(&s).map_err(err)?;
        let path = s.resolve_item_path(id, &mounted).map_err(err)?;
        reveal(&path)
    })
    .await
}

#[cfg(target_os = "macos")]
fn reveal(path: &std::path::Path) -> CmdResult<()> {
    std::process::Command::new("open")
        .arg("-R")
        .arg(path)
        .status()
        .map_err(|_| "Não foi possível abrir o Finder.".to_string())
        .and_then(|s| if s.success() { Ok(()) } else { Err("Não foi possível mostrar o item no Finder.".into()) })
}

#[cfg(windows)]
fn reveal(path: &std::path::Path) -> CmdResult<()> {
    std::process::Command::new("explorer")
        .arg(format!("/select,{}", path.display()))
        .spawn()
        .map(|_| ())
        .map_err(|_| "Não foi possível abrir o Explorer.".to_string())
}

#[cfg(not(any(target_os = "macos", windows)))]
fn reveal(_path: &std::path::Path) -> CmdResult<()> {
    Err("Mostrar no gerenciador de arquivos ainda não é suportado neste sistema.".into())
}

#[tauri::command]
async fn get_settings(state: State<'_, AppState>) -> CmdResult<Settings> {
    let s = open(&state)?;
    blocking(move || s.get_settings().map_err(err)).await
}

#[tauri::command]
async fn set_settings(state: State<'_, AppState>, settings: Settings) -> CmdResult<Settings> {
    let s = open(&state)?;
    blocking(move || s.set_settings(&settings).map_err(err)).await
}

#[tauri::command]
async fn claude_status() -> CmdResult<claude::ClaudeStatus> {
    blocking(|| Ok(claude::status(&claude::Ctx::from_system()?))).await
}

/// `target` opcional: "desktop" ou "code"; sem ele, conecta em todos os que existirem.
#[tauri::command]
async fn claude_connect(target: Option<String>) -> CmdResult<claude::ClaudeStatus> {
    blocking(move || claude::connect(&claude::Ctx::from_system()?, target.as_deref())).await
}

#[tauri::command]
async fn claude_disconnect(target: Option<String>) -> CmdResult<claude::ClaudeStatus> {
    blocking(move || claude::disconnect(&claude::Ctx::from_system()?, target.as_deref())).await
}

/// ADR 0015: a janela só pode mostrar a própria interface; qualquer endereço externo é bloqueado.
fn allow_navigation(url: &tauri::Url) -> bool {
    let host = url.host_str().unwrap_or("");
    let plain = url.username().is_empty() && url.password().is_none();
    match url.scheme() {
        // macOS/Linux: the app's own pages are tauri://localhost
        "tauri" => plain && host == "localhost" && url.port().is_none(),
        // Windows: http://tauri.localhost (no port)
        "http" if cfg!(windows) => plain && host == "tauri.localhost" && url.port().is_none(),
        // Development only: the local Vite server.
        "http" => cfg!(debug_assertions) && plain && host == "localhost" && url.port() == Some(1420),
        _ => false,
    }
}

#[cfg(test)]
mod nav_tests {
    use super::allow_navigation;

    fn ok(s: &str) -> bool {
        allow_navigation(&tauri::Url::parse(s).unwrap())
    }

    #[test]
    fn only_the_app_itself() {
        if !cfg!(windows) {
            assert!(ok("tauri://localhost/"));
            assert!(ok("tauri://localhost/index.html#x"));
        }
        for bad in [
            "https://example.com/",
            "http://tauri.localhost:8080/",
            "tauri://localhost.evil.com/",
            "tauri://user@localhost/",
            "asset://localhost/x",
            "ipc://localhost/x",
            "file:///etc/passwd",
            "data:text/html,hi",
            "javascript:alert(1)",
            "http://localhost:3000/",
        ] {
            assert!(!ok(bad), "{bad}");
        }
    }
    #[test]
    fn damaged_catalog_is_kept_aside_and_replaced() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("catalogo.sqlite");
        std::fs::write(&p, b"isto nao e um banco de dados, so texto qualquer com tamanho suficiente").unwrap();
        let n = super::prepare_catalog(&p).expect("devia avisar");
        assert!(!n.fatal, "{}", n.text);
        let kept: Vec<_> = std::fs::read_dir(d.path())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with("catalogo-danificado-"))
            .collect();
        assert_eq!(kept.len(), 1, "{kept:?}");
        assert!(std::fs::read(d.path().join(&kept[0])).unwrap().starts_with(b"isto nao e"));
        assert!(hdf_core::Catalog::open(&p).is_ok());
        assert!(super::prepare_catalog(&p).is_none());
    }

    #[test]
    fn newer_catalog_is_not_moved() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("catalogo.sqlite");
        drop(hdf_core::Catalog::open(&p).unwrap());
        hdf_core::rusqlite::Connection::open(&p).unwrap().execute("UPDATE schema_version SET version=99", []).unwrap();
        let n = super::prepare_catalog(&p).unwrap();
        assert!(n.fatal);
        assert_eq!(std::fs::read_dir(d.path()).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().contains("danificado")).count(), 0);
    }
}

/// What to tell the person at startup when the catalog could not be opened normally.
struct StartupNotice {
    text: String,
    /// The app cannot work: close it after the message.
    fatal: bool,
}

/// True only when the file itself is broken (not a database, or damaged). A busy or locked
/// catalog is never treated as broken.
fn is_damaged(e: &hdf_core::Error) -> bool {
    match e {
        hdf_core::Error::Db(d) => matches!(
            d.sqlite_error_code(),
            Some(hdf_core::rusqlite::ErrorCode::NotADatabase) | Some(hdf_core::rusqlite::ErrorCode::DatabaseCorrupt)
        ),
        hdf_core::Error::Msg(m) => m == hdf_core::db::MSG_DAMAGED_CATALOG,
        _ => false,
    }
}

/// Opens (creates, migrates) the catalog before the window exists. Never deletes it: a damaged
/// file is renamed and kept next to the new one, so nothing is lost and the app still opens.
fn prepare_catalog(path: &std::path::Path) -> Option<StartupNotice> {
    let mut last = None;
    for _ in 0..20 {
        match Catalog::open(path) {
            Ok(_) => return None,
            // Another HD Fantasma process (the Claude connection) may be opening it right now.
            Err(hdf_core::Error::Db(e)) if e.sqlite_error_code() == Some(hdf_core::rusqlite::ErrorCode::DatabaseBusy) => {
                last = Some(hdf_core::Error::Db(e));
                std::thread::sleep(std::time::Duration::from_millis(250));
            }
            Err(e) => {
                last = Some(e);
                break;
            }
        }
    }
    let e = last?;
    if let hdf_core::Error::Msg(m) = &e {
        if m == hdf_core::db::MSG_NEWER_CATALOG {
            return Some(StartupNotice { text: m.clone(), fatal: true });
        }
    }
    if !is_damaged(&e) {
        return Some(StartupNotice { text: format!("Não foi possível abrir o catálogo: {e}"), fatal: true });
    }
    let stamp = hdf_core::db::now_iso().replace([':', 'T'], "-");
    let stamp: String = stamp.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').take(19).collect();
    let kept = format!("catalogo-danificado-{stamp}.sqlite");
    let aside = path.with_file_name(&kept);
    if std::fs::rename(path, &aside).is_err() {
        return Some(StartupNotice { text: format!("O catálogo está danificado e não pude guardá-lo de lado: {e}"), fatal: true });
    }
    for side in ["-wal", "-shm"] {
        let mut from = path.as_os_str().to_owned();
        from.push(side);
        let mut to = aside.as_os_str().to_owned();
        to.push(side);
        let _ = std::fs::rename(from, to);
    }
    match Catalog::open(path) {
        Ok(_) => Some(StartupNotice {
            text: format!(
                "O catálogo não pôde ser aberto ({e}). Ele foi guardado como \"{kept}\", na mesma pasta, e um catálogo novo e vazio foi criado. Para ter os discos de volta, leia-os de novo ou importe exportações."
            ),
            fatal: false,
        }),
        Err(e2) => Some(StartupNotice { text: format!("Não foi possível criar um catálogo novo: {e2}"), fatal: true }),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let catalog_path = hdf_core::db::default_catalog_path().expect("pasta Documentos não encontrada");
    // Cria a pasta e o esquema já na abertura do app; se der errado, avisa na janela.
    let notice = prepare_catalog(&catalog_path);
    tauri::Builder::default()
        .setup(move |app| {
            use tauri::WebviewWindowBuilder;
            // Janela principal criada aqui para travar a navegação (ADR 0015).
            let cfg = app.config().app.windows[0].clone();
            WebviewWindowBuilder::from_config(app.handle(), &cfg)?
                .on_navigation(allow_navigation)
                .build()?;
            if let Some(n) = notice {
                use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
                let handle = app.handle().clone();
                app.dialog()
                    .message(n.text)
                    .title("HD Fantasma")
                    .kind(if n.fatal { MessageDialogKind::Error } else { MessageDialogKind::Warning })
                    .show(move |_| {
                        if n.fatal {
                            handle.exit(1);
                        }
                    });
            }
            Ok(())
        })
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            catalog_path,
            cancel: Arc::new(AtomicBool::new(false)),
            reading: Arc::new(AtomicBool::new(false)),
        })
        .invoke_handler(tauri::generate_handler![
            list_disks,
            list_mounted_volumes,
            read_disk,
            cancel_read,
            list_children,
            get_item,
            search,
            find_duplicates,
            rename_disk,
            delete_disk,
            export_disk,
            import_disk,
            reveal_item,
            get_settings,
            set_settings,
            claude_status,
            claude_connect,
            claude_disconnect,
        ])
        .run(tauri::generate_context!())
        .expect("erro ao iniciar o HD Fantasma");
}
