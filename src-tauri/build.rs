// Every command the window may call is listed here and allowed one by one in
// capabilities/default.json (audit 2026-11-14): a command that is not listed cannot be called.
const COMMANDS: &[&str] = &[
    "list_disks",
    "list_mounted_volumes",
    "read_disk",
    "cancel_read",
    "list_children",
    "get_item",
    "search",
    "find_duplicates",
    "rename_disk",
    "delete_disk",
    "export_disk",
    "import_disk",
    "reveal_item",
    "get_settings",
    "set_settings",
    "claude_status",
    "claude_connect",
    "claude_disconnect",
];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("failed to run tauri-build");
}
