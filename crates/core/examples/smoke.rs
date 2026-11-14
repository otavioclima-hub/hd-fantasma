//! Teste real: lê uma pasta de verdade para um catálogo temporário e imprime contagens.
//! Uso: cargo run -p hdf-core --example smoke -- <pasta> [termo-de-busca]
use hdf_core::*;
use std::sync::atomic::AtomicBool;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let folder = args.first().expect("passe a pasta a ler");
    let term = args.get(1).cloned().unwrap_or_else(|| "contrato".into());
    let tmp = std::env::temp_dir().join(format!("hdf-smoke-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let mut cat = Catalog::open(tmp.join("catalogo.sqlite")).unwrap();
    let vol = MountedVolume {
        uuid: "smoke".into(),
        name: "SMOKE".into(),
        mount_path: folder.clone(),
        filesystem: "apfs".into(),
        capacity_bytes: 0,
        free_bytes: 0,
        cataloged_disk_id: None,
    };
    let ffprobe = find_ffprobe(None);
    println!("ffprobe: {:?}", ffprobe);
    let extractor = RealExtractor::new(ffprobe);
    let cancel = AtomicBool::new(false);
    let t = std::time::Instant::now();
    let disk = read_disk_as(
        &mut cat,
        &vol,
        &ReadOptions { mode: ReadMode::Full, include_hidden: false },
        &extractor,
        &cancel,
        &mut |p| {
            if p.phase != Phase::Listing {
                println!("  [{:?}] {} itens {}", p.phase, p.items_seen, p.message.unwrap_or_default());
            }
        },
    )
    .unwrap();
    println!("Leitura: {} itens, {} bytes, em {:?}", disk.item_count, disk.total_bytes, t.elapsed());
    let r = cat
        .search(&SearchQuery { text: term.clone(), ..Default::default() })
        .unwrap();
    println!("Busca '{term}': {} resultado(s)", r.total);
    for it in r.items.iter().take(5) {
        println!("  {} ({:?}, {} bytes)", it.path, it.kind, it.size_bytes);
    }
    let roots = cat.list_children(disk.id, None).unwrap();
    println!("Raiz: {:?}", roots.iter().map(|i| &i.name).collect::<Vec<_>>());
    let all = cat.search(&SearchQuery { text: String::new(), ..Default::default() }).unwrap();
    for it in all.items.iter().filter(|i| i.video.is_some() || (i.kind == Kind::Archive && i.child_count > 0)) {
        println!("  {} -> video {:?} filhos {}", it.path, it.video, it.child_count);
        if it.kind == Kind::Archive {
            for c in cat.list_children(disk.id, Some(it.id)).unwrap() {
                println!("      {} ({:?}, {} bytes)", c.path, c.kind, c.size_bytes);
            }
        }
    }
    println!("Duplicados: {} grupo(s)", cat.find_duplicates(&[], 0).unwrap().len());
    println!("Volumes montados: {:?}", volumes::list_mounted_volumes(&cat).unwrap().iter().map(|v| (&v.name, &v.filesystem, &v.uuid)).collect::<Vec<_>>());
    let _ = std::fs::remove_dir_all(&tmp);
}
