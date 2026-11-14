//! App Tauri do HD Fantasma: comandos finos que chamam o motor (`hdf_core`).
//! Contrato: docs/CONTRATO.md. Erros voltam como texto em pt-BR.

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

/// ADR 0015: a janela só pode mostrar a própria interface; qualquer endereço externo é bloqueado.
fn allow_navigation(url: &tauri::Url) -> bool {
    match url.scheme() {
        "tauri" | "ipc" | "asset" => true,
        "http" | "https" => {
            let host = url.host_str().unwrap_or("");
            if host == "tauri.localhost" || host == "ipc.localhost" || host == "asset.localhost" {
                return true;
            }
            cfg!(debug_assertions) && host == "localhost" && url.port() == Some(1420)
        }
        _ => false,
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let catalog_path = hdf_core::db::default_catalog_path().expect("pasta Documentos não encontrada");
    // Cria a pasta e o esquema já na abertura do app.
    Catalog::open(&catalog_path).expect("não foi possível abrir o catálogo");
    tauri::Builder::default()
        .setup(|app| {
            use tauri::WebviewWindowBuilder;
            // Janela principal criada aqui para travar a navegação (ADR 0015).
            let cfg = app.config().app.windows[0].clone();
            WebviewWindowBuilder::from_config(app.handle(), &cfg)?
                .on_navigation(allow_navigation)
                .build()?;
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
        ])
        .run(tauri::generate_context!())
        .expect("erro ao iniciar o HD Fantasma");
}
