//! Mede a velocidade da Leitura numa pasta (catálogo temporário, sem ffprobe).
//! Uso: cargo run --release -p hdf-core --example bench -- <pasta> [segundos-máximos]
//! Se o tempo máximo estourar, cancela a leitura e mostra quantos itens já tinham sido vistos.
use hdf_core::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let folder = args.first().expect("passe a pasta a ler");
    let max_secs: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    // 3º argumento opcional: nº de threads leitoras (padrão: o do motor)
    let threads: Option<usize> = args.get(2).and_then(|s| s.parse().ok());
    let tmp = std::env::temp_dir().join(format!("hdf-bench-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let mut cat = Catalog::open(tmp.join("catalogo.sqlite")).unwrap();
    let vol = MountedVolume {
        uuid: "bench".into(),
        name: "BENCH".into(),
        mount_path: folder.clone(),
        filesystem: "apfs".into(),
        capacity_bytes: 0,
        free_bytes: 0,
        cataloged_disk_id: None,
    };
    let cancel = AtomicBool::new(false);
    let t = Instant::now();
    let mut seen = 0u64;
    let mut last_phase = Phase::Listing;
    let res = std::thread::scope(|s| {
        if max_secs > 0 {
            let cancel = &cancel;
            s.spawn(move || {
                let end = Instant::now() + Duration::from_secs(max_secs);
                while Instant::now() < end {
                    std::thread::sleep(Duration::from_millis(200));
                    if cancel.load(Ordering::Relaxed) {
                        return;
                    }
                }
                cancel.store(true, Ordering::Relaxed);
            });
        }
        let opts = ReadOptions { mode: ReadMode::Full, include_hidden: false };
        let mut on_progress = |p: ReadProgress| {
            seen = p.items_seen;
            if p.phase != last_phase || (p.phase == Phase::Saving && p.message.is_some()) {
                println!("  +{:.1}s fase {:?} ({} itens) {}", t.elapsed().as_secs_f64(), p.phase, p.items_seen, p.message.clone().unwrap_or_default());
            }
            last_phase = p.phase;
        };
        let r = match threads {
            Some(n) => read_disk_threads(&mut cat, &vol, &opts, &NoopExtractor, &cancel, &mut on_progress, n),
            None => read_disk_as(&mut cat, &vol, &opts, &NoopExtractor, &cancel, &mut on_progress),
        };
        // libera o vigia
        cancel.store(true, Ordering::Relaxed);
        r
    });
    let el = t.elapsed().as_secs_f64();
    match res {
        Ok(d) => println!(
            "{folder}: {} itens em {el:.2}s = {:.0} itens/s ({} bytes)",
            d.item_count,
            d.item_count as f64 / el,
            d.total_bytes
        ),
        Err(Error::Cancelled) => println!(
            "{folder}: NÃO TERMINOU em {el:.0}s; itens vistos: {seen} ({:.0} itens/s), fase {last_phase:?}",
            seen as f64 / el
        ),
        Err(e) => println!("{folder}: erro {e}"),
    }
    let _ = std::fs::remove_dir_all(&tmp);
}
