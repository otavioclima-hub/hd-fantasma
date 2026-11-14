//! Cria um catálogo de demonstração a partir de uma pasta (para testar o MCP à mão).
//! Uso: cargo run -p hd-fantasma-mcp --example demo_catalog -- <pasta> <catalogo.sqlite>
use hdf_core::*;
use std::sync::atomic::AtomicBool;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let (folder, db) = match (a.first(), a.get(1)) {
        (Some(f), Some(d)) => (f, d),
        _ => {
            eprintln!("uso: demo_catalog <pasta> <catalogo.sqlite>");
            std::process::exit(2);
        }
    };
    let mut cat = Catalog::open(db).expect("abrir catálogo");
    let vol = MountedVolume {
        uuid: "demo".into(),
        name: "DEMO".into(),
        mount_path: folder.clone(),
        filesystem: "apfs".into(),
        capacity_bytes: 0,
        free_bytes: 0,
        cataloged_disk_id: None,
    };
    let opts = ReadOptions { mode: ReadMode::Full, include_hidden: false };
    let d = read_disk_as(&mut cat, &vol, &opts, &AtomicBool::new(false), &mut |_| {}).expect("ler");
    println!("{} itens", d.item_count);
}
