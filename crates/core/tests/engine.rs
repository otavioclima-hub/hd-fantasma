use hdf_core::*;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

fn write(root: &Path, rel: &str, bytes: usize) {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, vec![b'x'; bytes]).unwrap();
}

fn make_tree(root: &Path) {
    write(root, "Projetos/2025/CLIENTE_v3_final.mov", 100);
    write(root, "Projetos/2025/notas.txt", 10);
    write(root, "Projetos/Vídeo Institucional.mp4", 50);
    write(root, "Projetos/pacote.zip", 20);
    write(root, "Backup/CLIENTE_v3_final.mov", 100);
    write(root, "Backup/outro.mov", 7);
    write(root, ".DS_Store", 1);
    write(root, "Projetos/._lixo", 1);
    write(root, "Thumbs.db", 1);
}

fn vol(root: &Path, uuid: &str) -> MountedVolume {
    MountedVolume {
        uuid: uuid.into(),
        name: "TESTE".into(),
        mount_path: root.to_string_lossy().into_owned(),
        filesystem: "apfs".into(),
        capacity_bytes: 1000,
        free_bytes: 500,
        cataloged_disk_id: None,
    }
}

struct Fake {
    calls: AtomicUsize,
}
impl DetailExtractor for Fake {
    fn wants(&self, item: &Item) -> bool {
        matches!(item.kind, Kind::Video | Kind::Archive)
    }
    fn extract(&self, path: &Path) -> Details {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if name.ends_with(".zip") {
            Details {
                video: None,
                archive_entries: vec![
                    ArchiveEntry { path: "cenas/take1.mov".into(), size_bytes: 300, modified_at: None, is_dir: false },
                    ArchiveEntry { path: "cenas/take2.mov".into(), size_bytes: 200, modified_at: None, is_dir: false },
                    ArchiveEntry { path: "leia-me.txt".into(), size_bytes: 5, modified_at: None, is_dir: false },
                ],
            }
        } else {
            Details {
                video: Some(VideoInfo {
                    duration_sec: Some(12.5),
                    width: Some(1920),
                    height: Some(1080),
                    codec: Some("prores".into()),
                    fps: Some(24.0),
                }),
                archive_entries: vec![],
            }
        }
    }
}

fn opts(mode: ReadMode, hidden: bool) -> ReadOptions {
    ReadOptions { mode, include_hidden: hidden }
}

fn scan(cat: &mut Catalog, v: &MountedVolume, mode: ReadMode, hidden: bool, ex: &dyn DetailExtractor) -> Disk {
    let cancel = AtomicBool::new(false);
    read_disk_as(cat, v, &opts(mode, hidden), ex, &cancel, &mut |_| {}).unwrap()
}

fn q(text: &str) -> SearchQuery {
    SearchQuery { text: text.into(), ..Default::default() }
}

fn setup() -> (tempfile::TempDir, tempfile::TempDir, Catalog, MountedVolume) {
    let data = tempfile::tempdir().unwrap();
    let store = tempfile::tempdir().unwrap();
    make_tree(data.path());
    let cat = Catalog::open(store.path().join("c.sqlite")).unwrap();
    let v = vol(data.path(), "uuid-1");
    (data, store, cat, v)
}

#[test]
fn scan_counts_sizes_and_ignores_system_files() {
    let (_d, _s, mut cat, v) = setup();
    let disk = scan(&mut cat, &v, ReadMode::Full, false, &NoopExtractor);
    // Pastas: Projetos, 2025, Backup = 3; arquivos: 6.
    assert_eq!(disk.item_count, 9);
    assert_eq!(disk.total_bytes, 100 + 10 + 50 + 20 + 100 + 7);
    assert!(disk.present);

    let root = cat.list_children(disk.id, None).unwrap();
    let names: Vec<_> = root.iter().map(|i| i.name.as_str()).collect();
    assert_eq!(names, ["Backup", "Projetos"]);
    let projetos = &root[1];
    assert_eq!(projetos.size_bytes, 100 + 10 + 50 + 20);
    assert_eq!(projetos.child_count, 3);
    let kids = cat.list_children(disk.id, Some(projetos.id)).unwrap();
    // pastas primeiro, depois nome
    assert_eq!(kids[0].name, "2025");
    assert_eq!(kids[0].kind, Kind::Folder);
    assert_eq!(kids[1].name, "pacote.zip");
    assert_eq!(kids[1].kind, Kind::Archive);
    assert_eq!(kids[2].name, "Vídeo Institucional.mp4");
    assert_eq!(kids[2].path, "Projetos/Vídeo Institucional.mp4");
    assert_eq!(kids[2].ext.as_deref(), Some("mp4"));
    assert!(kids[2].modified_at.as_deref().unwrap().ends_with('Z'));

    // Ocultos incluídos quando pedido (releitura completa).
    let disk = scan(&mut cat, &v, ReadMode::Full, true, &NoopExtractor);
    assert_eq!(disk.item_count, 9 + 3);
}

#[test]
fn search_is_partial_and_accent_insensitive() {
    let (_d, _s, mut cat, v) = setup();
    scan(&mut cat, &v, ReadMode::Full, false, &NoopExtractor);
    let n = |t: &str| cat.search(&q(t)).unwrap();
    assert_eq!(n("video").total, 1); // "Vídeo Institucional"
    assert_eq!(n("VÍDEO").items[0].name, "Vídeo Institucional.mp4");
    assert_eq!(n("final").total, 2);
    assert_eq!(n("v3 final").total, 2);
    assert_eq!(n("instituc").total, 1);
    assert_eq!(n("v3").total, 2); // termo curto (< 3 letras) cai no LIKE
    assert_eq!(n("zz9").total, 0);
    assert_eq!(n("").total, 9);
    // %, _ e aspas não quebram nada
    assert_eq!(n("100%").total, 0);
    assert_eq!(n("\"").total, 0);

    let mut f = q("");
    f.kinds = vec![Kind::Video];
    assert_eq!(cat.search(&f).unwrap().total, 4);
    f.min_bytes = Some(60);
    assert_eq!(cat.search(&f).unwrap().total, 2);
    let mut f = q("final");
    f.max_bytes = Some(10);
    assert_eq!(cat.search(&f).unwrap().total, 0);
    let mut f = q("");
    f.modified_after = Some("2000-01-01".into());
    f.modified_before = Some("2000-12-31".into());
    assert_eq!(cat.search(&f).unwrap().total, 0);
    f.modified_before = Some("2999-01-01".into());
    assert_eq!(cat.search(&f).unwrap().total, 9);
    let mut f = q("");
    f.limit = 2;
    f.offset = 1;
    let r = cat.search(&f).unwrap();
    assert_eq!((r.items.len(), r.total), (2, 9));
    let mut f = q("final");
    f.disk_ids = vec![9999];
    assert_eq!(cat.search(&f).unwrap().total, 0);
}

#[test]
fn quick_mode_keeps_details_for_unchanged_items() {
    let (d, _s, mut cat, v) = setup();
    let ex = Fake { calls: AtomicUsize::new(0) };
    let disk = scan(&mut cat, &v, ReadMode::Full, false, &ex);
    // 3 .mov + 1 .mp4 + 1 .zip
    assert_eq!(ex.calls.load(Ordering::SeqCst), 5);
    let mov = cat.search(&q("outro")).unwrap().items.remove(0);
    assert_eq!(mov.video.as_ref().unwrap().codec.as_deref(), Some("prores"));
    // conteúdo compactado: 3 arquivos + pasta "cenas"
    let zip = cat.search(&q("pacote")).unwrap().items.remove(0);
    assert_eq!(zip.child_count, 2); // cenas/ + leia-me.txt
    let inside = cat.list_children(disk.id, Some(zip.id)).unwrap();
    assert_eq!(inside[0].name, "cenas");
    assert_eq!(inside[0].size_bytes, 500);
    assert!(inside[0].inside_archive);
    assert_eq!(inside[0].path, "Projetos/pacote.zip/cenas");
    // item dentro do zip é pesquisável
    assert_eq!(cat.search(&q("take1")).unwrap().total, 1);
    let before_total = cat.search(&q("")).unwrap().total;

    // Atualização rápida sem mudanças: nenhuma extração nova, detalhes preservados.
    let disk2 = scan(&mut cat, &v, ReadMode::Quick, false, &ex);
    assert_eq!(ex.calls.load(Ordering::SeqCst), 5, "não deveria reextrair");
    assert_eq!(disk2.id, disk.id);
    assert_eq!(cat.search(&q("")).unwrap().total, before_total, "sem linhas duplicadas ou perdidas");
    let mov = cat.search(&q("outro")).unwrap().items.remove(0);
    assert_eq!(mov.video.as_ref().unwrap().width, Some(1920));
    assert_eq!(cat.search(&q("take1")).unwrap().total, 1);
    let zip = cat.search(&q("pacote")).unwrap().items.remove(0);
    assert_eq!(zip.child_count, 2);
    assert_eq!(cat.list_children(disk.id, Some(zip.id)).unwrap().len(), 2);

    // Muda um arquivo e adiciona outro: só os novos/alterados são reprocessados.
    write(d.path(), "Backup/outro.mov", 9);
    write(d.path(), "Backup/novo.mov", 3);
    fs::remove_file(d.path().join("Projetos/2025/notas.txt")).unwrap();
    let disk3 = scan(&mut cat, &v, ReadMode::Quick, false, &ex);
    assert_eq!(ex.calls.load(Ordering::SeqCst), 7);
    assert_eq!(cat.search(&q("notas")).unwrap().total, 0);
    assert_eq!(cat.search(&q("novo")).unwrap().total, 1);
    assert_eq!(disk3.item_count, 9 + 1 - 1);

    // Releitura completa refaz tudo.
    scan(&mut cat, &v, ReadMode::Full, false, &ex);
    assert_eq!(ex.calls.load(Ordering::SeqCst), 7 + 6);
    assert_eq!(cat.list_disks(&[]).unwrap().len(), 1);
}

#[test]
fn duplicates_by_name_and_size() {
    let (_d, _s, mut cat, v) = setup();
    let disk = scan(&mut cat, &v, ReadMode::Full, false, &NoopExtractor);
    let g = cat.find_duplicates(&[], 0).unwrap();
    assert_eq!(g.len(), 1);
    assert_eq!(g[0].name, "CLIENTE_v3_final.mov");
    assert_eq!(g[0].size_bytes, 100);
    assert_eq!(g[0].items.len(), 2);
    assert!(cat.find_duplicates(&[], 101).unwrap().is_empty());
    assert_eq!(cat.find_duplicates(&[disk.id], 1).unwrap().len(), 1);
    assert!(cat.find_duplicates(&[disk.id + 5], 1).unwrap().is_empty());
}

#[test]
fn export_then_import_roundtrip() {
    let (_d, s, mut cat, v) = setup();
    let ex = Fake { calls: AtomicUsize::new(0) };
    let disk = scan(&mut cat, &v, ReadMode::Full, false, &ex);
    let file = s.path().join("teste.hdfantasma");
    cat.export_disk(disk.id, &file).unwrap();

    // Importa num catálogo que já tem outro disco (ids não podem colidir).
    let other_data = tempfile::tempdir().unwrap();
    write(other_data.path(), "a/b.txt", 4);
    let mut cat2 = Catalog::open(s.path().join("c2.sqlite")).unwrap();
    scan(&mut cat2, &vol(other_data.path(), "uuid-2"), ReadMode::Full, false, &NoopExtractor);
    let imported = cat2.import_disk(&file).unwrap();
    assert_eq!(imported.uuid, "uuid-1");
    assert_eq!(imported.item_count, disk.item_count);
    assert!(!imported.present);
    assert_eq!(cat2.list_disks(&[]).unwrap().len(), 2);
    assert_eq!(cat2.search(&q("take1")).unwrap().total, 1);
    assert_eq!(cat2.search(&q("video")).unwrap().total, 1);
    let root = cat2.list_children(imported.id, None).unwrap();
    assert_eq!(root.len(), 2);
    let outro = cat2.search(&q("outro")).unwrap().items.remove(0);
    assert_eq!(outro.video.unwrap().codec.as_deref(), Some("prores"));
    let zip = cat2.search(&q("pacote")).unwrap().items.remove(0);
    assert_eq!(cat2.list_children(imported.id, Some(zip.id)).unwrap()[0].name, "cenas");
    // pai de cada item existe (ids remapeados direito)
    for it in cat2.search(&SearchQuery { limit: 100, ..Default::default() }).unwrap().items {
        if let Some(p) = it.parent_id {
            let parent = cat2.get_item(p).unwrap();
            assert_eq!(parent.disk_id, it.disk_id);
        }
    }

    // Importar de novo substitui (mesmo uuid), sem duplicar.
    let again = cat2.import_disk(&file).unwrap();
    assert_eq!(again.id, imported.id);
    assert_eq!(cat2.list_disks(&[]).unwrap().len(), 2);
    assert_eq!(cat2.search(&q("final")).unwrap().total, 2);

    // Arquivo inválido
    let bad = s.path().join("ruim.hdfantasma");
    fs::write(&bad, b"nao e sqlite").unwrap();
    assert!(cat2.import_disk(&bad).is_err());
}

#[test]
fn cancel_rolls_back_and_delete_cleans_search() {
    let (_d, _s, mut cat, v) = setup();
    let disk = scan(&mut cat, &v, ReadMode::Full, false, &NoopExtractor);
    let cancel = AtomicBool::new(true);
    let r = read_disk_as(&mut cat, &v, &opts(ReadMode::Full, false), &NoopExtractor, &cancel, &mut |_| {});
    assert!(matches!(r, Err(Error::Cancelled)));
    // catálogo intacto
    assert_eq!(cat.search(&q("")).unwrap().total, 9);

    cat.rename_disk(disk.id, "Meu HD").unwrap();
    assert_eq!(cat.get_disk(disk.id).unwrap().name, "Meu HD");
    let mounted = [MountedVolume { cataloged_disk_id: Some(disk.id), ..v.clone() }];
    let ds = cat.list_disks(&mounted).unwrap();
    assert!(ds[0].present && ds[0].mount_path.is_some());

    cat.delete_disk(disk.id).unwrap();
    assert!(cat.list_disks(&[]).unwrap().is_empty());
    assert_eq!(cat.search(&q("final")).unwrap().total, 0);
}

#[test]
fn progress_is_emitted_and_ends_with_done() {
    let (_d, _s, mut cat, v) = setup();
    let cancel = AtomicBool::new(false);
    let mut phases = vec![];
    read_disk_as(&mut cat, &v, &opts(ReadMode::Full, false), &NoopExtractor, &cancel, &mut |p| {
        phases.push(p.phase)
    })
    .unwrap();
    assert_eq!(phases.first(), Some(&Phase::Listing));
    assert!(phases.contains(&Phase::Saving));
    assert_eq!(phases.last(), Some(&Phase::Done));
}

#[test]
fn settings_roundtrip_and_json_shape() {
    let store = tempfile::tempdir().unwrap();
    let cat = Catalog::open(store.path().join("c.sqlite")).unwrap();
    assert_eq!(cat.get_settings().unwrap(), Settings::default());
    cat.set_settings(&Settings { language: Language::En, include_hidden: true }).unwrap();
    assert_eq!(cat.get_settings().unwrap().language, Language::En);
    let json = serde_json::to_value(cat.get_settings().unwrap()).unwrap();
    assert_eq!(json["includeHidden"], true);
    assert_eq!(json["language"], "en");
    // SearchQuery aceita JSON do contrato (camelCase)
    let sq: SearchQuery = serde_json::from_str(
        r#"{"text":"a","kinds":["video"],"minBytes":null,"maxBytes":5,"modifiedAfter":null,"modifiedBefore":null,"diskIds":[],"limit":500,"offset":0}"#,
    )
    .unwrap();
    assert_eq!(sq.kinds, vec![Kind::Video]);
}

/// Extrator lento que pede cancelamento no meio: a Leitura inteira volta atrás.
#[test]
fn cancel_during_details_rolls_back() {
    struct Cancelling<'a>(&'a AtomicBool);
    impl DetailExtractor for Cancelling<'_> {
        fn wants(&self, item: &Item) -> bool {
            item.kind == Kind::Video
        }
        fn extract(&self, _p: &Path) -> Details {
            self.0.store(true, Ordering::SeqCst);
            Details::default()
        }
    }
    let (_d, _s, mut cat, v) = setup();
    let cancel = AtomicBool::new(false);
    let r = read_disk_as(&mut cat, &v, &opts(ReadMode::Full, false), &Cancelling(&cancel), &cancel, &mut |_| {});
    assert!(matches!(r, Err(Error::Cancelled)));
    assert!(cat.list_disks(&[]).unwrap().is_empty() || cat.list_disks(&[]).unwrap()[0].item_count == 0);
}

/// Vários itens pendentes em paralelo: todos recebem seus detalhes.
#[test]
fn parallel_details_cover_all_items() {
    let data = tempfile::tempdir().unwrap();
    let store = tempfile::tempdir().unwrap();
    for i in 0..40 {
        std::fs::write(data.path().join(format!("c{i}.mov")), vec![0u8; 10 + i]).unwrap();
    }
    let mut cat = Catalog::open(store.path().join("c.sqlite")).unwrap();
    let v = MountedVolume {
        uuid: "par".into(), name: "PAR".into(), mount_path: data.path().to_string_lossy().into(),
        filesystem: "apfs".into(), capacity_bytes: 0, free_bytes: 0, cataloged_disk_id: None,
    };
    let fake = Fake { calls: AtomicUsize::new(0) };
    let disk = scan(&mut cat, &v, ReadMode::Full, false, &fake);
    assert_eq!(fake.calls.load(Ordering::SeqCst), 40);
    let kids = cat.list_children(disk.id, None).unwrap();
    assert_eq!(kids.len(), 40);
    assert!(kids.iter().all(|k| k.video.as_ref().and_then(|x| x.width) == Some(1920)));
}

// ------------------------------------------------------- leitura em paralelo (ADR 0012)

use std::collections::BTreeMap;

/// Árvore de ~6,8 mil arquivos, 6 níveis, com pastas vazias e tamanhos variados.
fn make_big_tree(root: &Path) -> usize {
    fn rec(dir: &Path, depth: usize, n: &mut usize) {
        fs::create_dir_all(dir).unwrap();
        for f in 0..5 {
            *n += 1;
            let ext = ["mov", "txt", "zip", "jpg", "dat"][f];
            fs::write(dir.join(format!("f{f}-{n}.{ext}")), vec![b'x'; (*n * 7) % 300]).unwrap();
        }
        if depth < 6 {
            for c in 0..4 {
                rec(&dir.join(format!("d{depth}_{c}")), depth + 1, n);
            }
        }
        if depth == 3 {
            fs::create_dir_all(dir.join("vazia")).unwrap();
        }
    }
    let mut n = 0;
    rec(root, 1, &mut n);
    n
}

/// Percurso de referência, simples e sequencial: caminho -> (tipo, tamanho recursivo, filhos).
fn reference(dir: &Path, rel: &str, out: &mut BTreeMap<String, (String, u64, u64)>) -> (u64, u64) {
    let (mut size, mut count) = (0, 0);
    for e in fs::read_dir(dir).unwrap() {
        let e = e.unwrap();
        let name = e.file_name().to_string_lossy().into_owned();
        let r = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
        let md = e.metadata().unwrap();
        count += 1;
        if md.is_dir() {
            let (s, c) = reference(&e.path(), &r, out);
            out.insert(r, ("folder".into(), s, c));
            size += s;
        } else {
            let kind = Kind::from_ext(Path::new(&name).extension().map(|x| x.to_string_lossy().to_lowercase()).as_deref());
            out.insert(r, (kind.as_str().into(), md.len(), 0));
            size += md.len();
        }
    }
    (size, count)
}

fn catalog_rows(path: &Path) -> (BTreeMap<String, (String, u64, u64)>, i64, i64) {
    let c = rusqlite::Connection::open(path).unwrap();
    let mut st = c
        .prepare("SELECT i.path, i.kind, i.size_bytes, i.child_count, p.path, i.parent_id FROM items i LEFT JOIN items p ON p.id=i.parent_id")
        .unwrap();
    let mut m = BTreeMap::new();
    let rows = st
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)? as u64,
                r.get::<_, i64>(3)? as u64,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, Option<i64>>(5)?,
            ))
        })
        .unwrap();
    for r in rows {
        let (path, kind, size, cc, parent_path, parent_id) = r.unwrap();
        // pai certo: o caminho do pai é o caminho do filho sem o último pedaço
        let expect = path.rsplit_once('/').map(|x| x.0.to_string());
        assert_eq!(parent_path, expect, "pai errado para {path}");
        assert_eq!(parent_id.is_some(), expect.is_some());
        m.insert(path, (kind, size, cc));
    }
    let items: i64 = c.query_row("SELECT COUNT(*) FROM items", [], |r| r.get(0)).unwrap();
    let fts: i64 = c.query_row("SELECT COUNT(*) FROM items_fts", [], |r| r.get(0)).unwrap();
    (m, items, fts)
}

#[test]
fn parallel_walk_matches_reference_walk() {
    let data = tempfile::tempdir().unwrap();
    let store = tempfile::tempdir().unwrap();
    let files = make_big_tree(data.path());
    assert!(files > 5000);
    let mut expected = BTreeMap::new();
    let (total, _) = reference(data.path(), "", &mut expected);

    let dbp = store.path().join("c.sqlite");
    let mut cat = Catalog::open(&dbp).unwrap();
    let v = vol(data.path(), "big");
    let disk = scan(&mut cat, &v, ReadMode::Full, false, &NoopExtractor);
    assert_eq!(disk.item_count as usize, expected.len());
    assert_eq!(disk.total_bytes, total);
    let (rows, items, fts) = catalog_rows(&dbp);
    assert_eq!(rows, expected, "catálogo difere do percurso de referência");
    assert_eq!(items, fts, "índice de busca completo");

    // Busca funciona depois da leitura, e depois da atualização rápida.
    let n1 = cat.search(&q("f3-")).unwrap().total;
    assert!(n1 > 1000);
    let disk2 = scan(&mut cat, &v, ReadMode::Quick, false, &NoopExtractor);
    assert_eq!(disk2.item_count, disk.item_count);
    let (rows2, items2, fts2) = catalog_rows(&dbp);
    assert_eq!(rows2, expected);
    assert_eq!((items2, fts2), (items, fts));
    assert_eq!(cat.search(&q("f3-")).unwrap().total, n1);

    // Com 1 thread e com muitas o resultado é o mesmo.
    for n in [1, 40] {
        let cancel = AtomicBool::new(false);
        let disk3 = read_disk_threads(&mut cat, &v, &opts(ReadMode::Full, false), &NoopExtractor, &cancel, &mut |_| {}, n)
            .unwrap();
        assert_eq!(disk3.item_count, disk.item_count);
        assert_eq!(catalog_rows(&dbp).0, expected);
    }
}

/// Cancelar no meio da listagem (as threads leitoras já rodando) desfaz tudo.
#[test]
fn cancel_mid_walk_rolls_back() {
    struct CancelAfter<'a> {
        cancel: &'a AtomicBool,
        calls: AtomicUsize,
    }
    impl DetailExtractor for CancelAfter<'_> {
        fn wants(&self, _item: &Item) -> bool {
            if self.calls.fetch_add(1, Ordering::SeqCst) == 800 {
                self.cancel.store(true, Ordering::SeqCst);
            }
            false
        }
        fn extract(&self, _p: &Path) -> Details {
            Details::default()
        }
    }
    let data = tempfile::tempdir().unwrap();
    let store = tempfile::tempdir().unwrap();
    make_big_tree(data.path());
    let dbp = store.path().join("c.sqlite");
    let mut cat = Catalog::open(&dbp).unwrap();
    let v = vol(data.path(), "cancel");
    let cancel = AtomicBool::new(false);
    let ex = CancelAfter { cancel: &cancel, calls: AtomicUsize::new(0) };
    let r = read_disk_as(&mut cat, &v, &opts(ReadMode::Full, false), &ex, &cancel, &mut |_| {});
    assert!(matches!(r, Err(Error::Cancelled)));
    assert!(ex.calls.load(Ordering::SeqCst) > 800);
    assert!(cat.list_disks(&[]).unwrap().is_empty(), "nada ficou gravado");
    let (rows, items, fts) = catalog_rows(&dbp);
    assert!(rows.is_empty() && items == 0 && fts == 0);

    // Depois do cancelamento uma leitura normal funciona.
    let disk = scan(&mut cat, &v, ReadMode::Full, false, &NoopExtractor);
    assert!(disk.item_count > 5000);
}
