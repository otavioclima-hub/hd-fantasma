use hdf_core::*;
use std::fs;
use std::path::Path;
use std::sync::atomic::AtomicBool;

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

fn opts(mode: ReadMode, hidden: bool) -> ReadOptions {
    ReadOptions { mode, include_hidden: hidden }
}

fn scan(cat: &mut Catalog, v: &MountedVolume, mode: ReadMode, hidden: bool) -> Disk {
    let cancel = AtomicBool::new(false);
    read_disk_as(cat, v, &opts(mode, hidden), &cancel, &mut |_| {}).unwrap()
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
    let disk = scan(&mut cat, &v, ReadMode::Full, false);
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
    // Projetos: 3 filhos diretos (2025, pacote.zip, Vídeo…) + 2 arquivos dentro de 2025 = 5
    assert_eq!(projetos.descendant_count, 5);
    assert_eq!(root[0].descendant_count, 2, "Backup: 2 arquivos");
    assert_eq!(cat.list_children(disk.id, Some(projetos.id)).unwrap()[0].descendant_count, 2, "2025");
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
    let disk = scan(&mut cat, &v, ReadMode::Full, true);
    assert_eq!(disk.item_count, 9 + 3);
}

#[test]
fn search_is_partial_and_accent_insensitive() {
    let (_d, _s, mut cat, v) = setup();
    scan(&mut cat, &v, ReadMode::Full, false);
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
fn quick_and_full_modes_both_rebuild_without_duplicates() {
    let (d, _s, mut cat, v) = setup();
    let disk = scan(&mut cat, &v, ReadMode::Full, false);
    let before = cat.search(&q("")).unwrap().total;
    let disk2 = scan(&mut cat, &v, ReadMode::Quick, false);
    assert_eq!(disk2.id, disk.id);
    assert_eq!(cat.search(&q("")).unwrap().total, before, "sem linhas duplicadas ou perdidas");

    write(d.path(), "Backup/outro.mov", 9);
    write(d.path(), "Backup/novo.mov", 3);
    fs::remove_file(d.path().join("Projetos/2025/notas.txt")).unwrap();
    let disk3 = scan(&mut cat, &v, ReadMode::Quick, false);
    assert_eq!(cat.search(&q("notas")).unwrap().total, 0);
    assert_eq!(cat.search(&q("novo")).unwrap().total, 1);
    assert_eq!(disk3.item_count, 9 + 1 - 1);
    assert_eq!(cat.search(&q("outro")).unwrap().items[0].size_bytes, 9);

    scan(&mut cat, &v, ReadMode::Full, false);
    assert_eq!(cat.list_disks(&[]).unwrap().len(), 1);
}

#[test]
fn duplicates_by_name_and_size() {
    let (_d, _s, mut cat, v) = setup();
    let disk = scan(&mut cat, &v, ReadMode::Full, false);
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
    let disk = scan(&mut cat, &v, ReadMode::Full, false);
    let file = s.path().join("teste.hdfantasma");
    cat.export_disk(disk.id, &file).unwrap();

    // Importa num catálogo que já tem outro disco (ids não podem colidir).
    let other_data = tempfile::tempdir().unwrap();
    write(other_data.path(), "a/b.txt", 4);
    let mut cat2 = Catalog::open(s.path().join("c2.sqlite")).unwrap();
    scan(&mut cat2, &vol(other_data.path(), "uuid-2"), ReadMode::Full, false);
    let imported = cat2.import_disk(&file).unwrap();
    assert_eq!(imported.uuid, "uuid-1");
    assert_eq!(imported.item_count, disk.item_count);
    assert!(!imported.present);
    assert_eq!(cat2.list_disks(&[]).unwrap().len(), 2);
    assert_eq!(cat2.search(&q("video")).unwrap().total, 1);
    let root = cat2.list_children(imported.id, None).unwrap();
    assert_eq!(root.len(), 2);
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
    let disk = scan(&mut cat, &v, ReadMode::Full, false);
    let cancel = AtomicBool::new(true);
    let r = read_disk_as(&mut cat, &v, &opts(ReadMode::Full, false), &cancel, &mut |_| {});
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
fn deleting_the_biggest_disk_keeps_search_working_for_the_others() {
    let (_d, _s, mut cat, v) = setup();
    let big = scan(&mut cat, &v, ReadMode::Full, false);
    let small_root = tempfile::tempdir().unwrap();
    write(small_root.path(), "Sobra/Vídeo pequeno.mov", 5);
    let small = scan(&mut cat, &vol(small_root.path(), "uuid-2"), ReadMode::Full, false);
    assert!(cat.search(&q("final")).unwrap().total > 0);

    // The big disk is most of the catalog: the search index is rebuilt from what stays.
    cat.delete_disk(big.id).unwrap();
    assert_eq!(cat.search(&q("final")).unwrap().total, 0);
    assert_eq!(cat.search(&q("video peq")).unwrap().total, 1, "accent-free search still works");
    assert_eq!(cat.search(&q("")).unwrap().total, 2);

    // Reading the disk again and deleting the small one (minority path) also work.
    let big = scan(&mut cat, &v, ReadMode::Full, false);
    let again = scan(&mut cat, &v, ReadMode::Full, false);
    assert_eq!(again.id, big.id);
    assert_eq!(cat.search(&q("final")).unwrap().total, 2);
    cat.delete_disk(small.id).unwrap();
    assert_eq!(cat.search(&q("pequeno")).unwrap().total, 0);
    assert_eq!(cat.search(&q("final")).unwrap().total, 2);
}

#[test]
fn progress_is_emitted_and_ends_with_done() {
    let (_d, _s, mut cat, v) = setup();
    let cancel = AtomicBool::new(false);
    let mut phases = vec![];
    read_disk_as(&mut cat, &v, &opts(ReadMode::Full, false), &cancel, &mut |p| {
        phases.push(p.phase)
    })
    .unwrap();
    assert_eq!(phases.first(), Some(&Phase::Listing));
    assert!(phases.contains(&Phase::Saving));
    assert_eq!(phases.last(), Some(&Phase::Done));
}

#[test]
fn progress_reports_bytes() {
    let (_d, _s, mut cat, v) = setup();
    let cancel = AtomicBool::new(false);
    let mut last_listing_bytes = 0;
    let mut done_bytes = 0;
    let mut phases = vec![];
    read_disk_as(&mut cat, &v, &opts(ReadMode::Full, false), &cancel, &mut |p| {
        // vol(): capacidade 1000 - livre 500 = 500 esperados
        assert_eq!(p.bytes_expected, 500);
        assert!(!p.in_cloud);
        match p.phase {
            Phase::Listing => last_listing_bytes = p.bytes_seen,
            Phase::Done => done_bytes = p.bytes_seen,
            _ => {}
        }
        phases.push(p.phase);
    })
    .unwrap();
    assert!(done_bytes > 0, "bytesSeen soma o espaço alocado dos arquivos");
    assert!(done_bytes >= last_listing_bytes);
    assert_eq!(phases.first(), Some(&Phase::Listing));
    assert_eq!(phases.last(), Some(&Phase::Done));
    let j = serde_json::to_value(ReadProgress::new("u".into(), Phase::Done, 1, None)).unwrap();
    for k in ["bytesSeen", "bytesExpected", "inCloud"] {
        assert!(j.get(k).is_some(), "campo {k} no JSON");
    }
    for k in ["detailsDone", "detailsTotal"] {
        assert!(j.get(k).is_none(), "campo {k} não existe mais");
    }
}

/// Catálogo da versão 1 (com dados de vídeo e conteúdo compactado) abre e é migrado.
#[test]
fn v1_catalog_is_migrated() {
    let store = tempfile::tempdir().unwrap();
    let dbp = store.path().join("old.sqlite");
    {
        let c = rusqlite::Connection::open(&dbp).unwrap();
        c.execute_batch(
            "CREATE TABLE schema_version (version INTEGER NOT NULL);
             INSERT INTO schema_version VALUES (1);
             CREATE TABLE disks (id INTEGER PRIMARY KEY, uuid TEXT NOT NULL UNIQUE, name TEXT NOT NULL,
               filesystem TEXT NOT NULL DEFAULT '', capacity_bytes INTEGER NOT NULL DEFAULT 0,
               free_bytes INTEGER NOT NULL DEFAULT 0, item_count INTEGER NOT NULL DEFAULT 0,
               total_bytes INTEGER NOT NULL DEFAULT 0, last_read_at TEXT NOT NULL);
             CREATE TABLE items (id INTEGER PRIMARY KEY, disk_id INTEGER NOT NULL REFERENCES disks(id) ON DELETE CASCADE,
               parent_id INTEGER, name TEXT NOT NULL, path TEXT NOT NULL, kind TEXT NOT NULL, ext TEXT,
               size_bytes INTEGER NOT NULL DEFAULT 0, created_at TEXT, modified_at TEXT,
               inside_archive INTEGER NOT NULL DEFAULT 0, child_count INTEGER NOT NULL DEFAULT 0,
               video_duration REAL, video_width INTEGER, video_height INTEGER, video_codec TEXT, video_fps REAL);
             CREATE INDEX idx_items_parent ON items(disk_id, parent_id);
             CREATE INDEX idx_items_dup ON items(name, size_bytes);
             CREATE VIRTUAL TABLE items_fts USING fts5(name_norm, tokenize='trigram');
             CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             INSERT INTO disks VALUES (1,'u1','OLD','apfs',1000,500,3,300,'2025-01-01T00:00:00Z');
             INSERT INTO items(id,disk_id,parent_id,name,path,kind,ext,size_bytes,inside_archive,child_count,video_codec)
               VALUES (1,1,NULL,'pacote.zip','pacote.zip','archive','zip',100,0,1,NULL),
                      (2,1,1,'dentro.mov','pacote.zip/dentro.mov','video','mov',200,1,0,NULL),
                      (3,1,NULL,'filme.mov','filme.mov','video','mov',200,0,0,'prores');
             INSERT INTO items_fts(rowid,name_norm) VALUES (1,'pacote.zip'),(2,'dentro.mov'),(3,'filme.mov');",
        )
        .unwrap();
    }
    let cat = Catalog::open(&dbp).unwrap();
    assert_eq!(cat.search(&q("")).unwrap().total, 2, "conteúdo compactado sumiu");
    assert_eq!(cat.search(&q("dentro")).unwrap().total, 0);
    assert_eq!(cat.search(&q("filme")).unwrap().total, 1);
    let d = &cat.list_disks(&[]).unwrap()[0];
    assert_eq!((d.item_count, d.total_bytes), (2, 300));
    let c = rusqlite::Connection::open(&dbp).unwrap();
    let cols: i64 = c
        .query_row("SELECT COUNT(*) FROM pragma_table_info('items') WHERE name LIKE 'video_%' OR name='inside_archive'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(cols, 0);
    let v: i64 = c.query_row("SELECT MAX(version) FROM schema_version", [], |r| r.get(0)).unwrap();
    assert_eq!(v, hdf_core::db::SCHEMA_VERSION);
    // v3: a coluna nova existe e foi calculada (pacote.zip tem 1 filho no catálogo antigo, que sumiu).
    let dc: i64 = c.query_row("SELECT COUNT(*) FROM pragma_table_info('items') WHERE name='descendant_count'", [], |r| r.get(0)).unwrap();
    assert_eq!(dc, 1);
}

/// Catálogo v2 (sem `descendant_count`) ganha a coluna e o valor certo ao abrir.
#[test]
fn v2_catalog_gets_descendant_counts() {
    let store = tempfile::tempdir().unwrap();
    let dbp = store.path().join("v2.sqlite");
    {
        let c = rusqlite::Connection::open(&dbp).unwrap();
        c.execute_batch(
            "CREATE TABLE schema_version (version INTEGER NOT NULL);
             INSERT INTO schema_version VALUES (2);
             CREATE TABLE disks (id INTEGER PRIMARY KEY, uuid TEXT NOT NULL UNIQUE, name TEXT NOT NULL,
               filesystem TEXT NOT NULL DEFAULT '', capacity_bytes INTEGER NOT NULL DEFAULT 0,
               free_bytes INTEGER NOT NULL DEFAULT 0, item_count INTEGER NOT NULL DEFAULT 0,
               total_bytes INTEGER NOT NULL DEFAULT 0, last_read_at TEXT NOT NULL);
             CREATE TABLE items (id INTEGER PRIMARY KEY, disk_id INTEGER NOT NULL REFERENCES disks(id) ON DELETE CASCADE,
               parent_id INTEGER, name TEXT NOT NULL, path TEXT NOT NULL, kind TEXT NOT NULL, ext TEXT,
               size_bytes INTEGER NOT NULL DEFAULT 0, created_at TEXT, modified_at TEXT,
               child_count INTEGER NOT NULL DEFAULT 0);
             CREATE VIRTUAL TABLE items_fts USING fts5(name_norm, tokenize='trigram');
             CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             INSERT INTO disks VALUES (1,'u1','OLD','apfs',1000,500,4,3,'2025-01-01T00:00:00Z');
             INSERT INTO items(id,disk_id,parent_id,name,path,kind,ext,size_bytes,child_count) VALUES
               (1,1,NULL,'A','A','folder',NULL,3,2),
               (2,1,1,'B','A/B','folder',NULL,2,1),
               (3,1,2,'f.txt','A/B/f.txt','document','txt',2,0),
               (4,1,1,'g.txt','A/g.txt','document','txt',1,0);",
        )
        .unwrap();
    }
    let cat = Catalog::open(&dbp).unwrap();
    let root = cat.list_children(1, None).unwrap();
    assert_eq!(root[0].descendant_count, 3, "B, f.txt e g.txt");
    let b = cat.list_children(1, Some(1)).unwrap();
    assert_eq!(b[0].name, "B");
    assert_eq!(b[0].descendant_count, 1);
    assert_eq!(b[1].descendant_count, 0, "arquivo");
}

#[test]
fn settings_roundtrip_and_json_shape() {
    let store = tempfile::tempdir().unwrap();
    let cat = Catalog::open(store.path().join("c.sqlite")).unwrap();
    assert_eq!(cat.get_settings().unwrap(), Settings::default());
    cat.set_settings(&Settings { language: Language::En, include_hidden: true, view_mode: ViewMode::List, show_inspector: false }).unwrap();
    assert_eq!(cat.get_settings().unwrap().language, Language::En);
    let json = serde_json::to_value(cat.get_settings().unwrap()).unwrap();
    assert_eq!(json["includeHidden"], true);
    assert_eq!(json["language"], "en");
    assert_eq!(json["viewMode"], "list");
    assert_eq!(json["showInspector"], false);
    // Configuração gravada por versão antiga (sem os campos novos) continua valendo.
    let old: Settings = serde_json::from_str(r#"{"language":"en","includeHidden":true}"#).unwrap();
    assert_eq!(old.language, Language::En);
    assert_eq!((old.view_mode, old.show_inspector), (ViewMode::Columns, true));
    // SearchQuery aceita JSON do contrato (camelCase)
    let sq: SearchQuery = serde_json::from_str(
        r#"{"text":"a","kinds":["video"],"minBytes":null,"maxBytes":5,"modifiedAfter":null,"modifiedBefore":null,"diskIds":[],"limit":500,"offset":0}"#,
    )
    .unwrap();
    assert_eq!(sq.kinds, vec![Kind::Video]);
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
        // Files store no path (ADR 0018): the catalog rebuilds it from the parent folder.
        .prepare(
            "SELECT CASE WHEN i.path IS NOT NULL THEN i.path WHEN p.path IS NULL THEN i.name ELSE p.path || '/' || i.name END,
                    i.kind, i.size_bytes, i.child_count, p.path, i.parent_id
             FROM items i LEFT JOIN items p ON p.id=i.parent_id",
        )
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
    let disk = scan(&mut cat, &v, ReadMode::Full, false);
    assert_eq!(disk.item_count as usize, expected.len());
    assert_eq!(disk.total_bytes, total);
    let (rows, items, fts) = catalog_rows(&dbp);
    assert_eq!(rows, expected, "catálogo difere do percurso de referência");
    assert_eq!(items, fts, "índice de busca completo");

    // Busca funciona depois da leitura, e depois da atualização rápida.
    let n1 = cat.search(&q("f3-")).unwrap().total;
    assert!(n1 > 1000);
    let disk2 = scan(&mut cat, &v, ReadMode::Quick, false);
    assert_eq!(disk2.item_count, disk.item_count);
    let (rows2, items2, fts2) = catalog_rows(&dbp);
    assert_eq!(rows2, expected);
    assert_eq!((items2, fts2), (items, fts));
    assert_eq!(cat.search(&q("f3-")).unwrap().total, n1);

    // Com 1 thread e com muitas o resultado é o mesmo.
    for n in [1, 40] {
        let cancel = AtomicBool::new(false);
        let disk3 = read_disk_threads(&mut cat, &v, &opts(ReadMode::Full, false), &cancel, &mut |_| {}, n)
            .unwrap();
        assert_eq!(disk3.item_count, disk.item_count);
        assert_eq!(catalog_rows(&dbp).0, expected);
    }
}

/// ADR 0014: pastas de nuvem que ficam sem arquivo somem; a irmã com arquivo fica.
#[test]
fn empty_cloud_folders_are_pruned() {
    let data = tempfile::tempdir().unwrap();
    let store = tempfile::tempdir().unwrap();
    let root = data.path();
    write(root, "Library/CloudStorage/X/real/arquivo.txt", 10);
    fs::create_dir_all(root.join("Library/CloudStorage/X/empty/sub")).unwrap();
    fs::create_dir_all(root.join("Outra/vazia")).unwrap(); // fora da nuvem: continua
    let mut cat = Catalog::open(store.path().join("c.sqlite")).unwrap();
    let v = vol(root, "uuid-cloud");
    let disk = scan(&mut cat, &v, ReadMode::Full, false);
    let paths = catalog_rows(&store.path().join("c.sqlite")).0;
    assert!(paths.contains_key("Library/CloudStorage/X/real/arquivo.txt"));
    assert!(paths.contains_key("Library/CloudStorage/X/real"));
    assert!(!paths.contains_key("Library/CloudStorage/X/empty"), "pasta de nuvem vazia podada");
    assert!(!paths.contains_key("Library/CloudStorage/X/empty/sub"));
    assert!(paths.contains_key("Outra/vazia"), "pasta vazia fora da nuvem fica");
    // 1 (Library) + CloudStorage + X + real + arquivo + Outra + vazia = 7
    assert_eq!(disk.item_count, 7);
    let cs = cat.list_children(disk.id, None).unwrap();
    let lib = cs.iter().find(|i| i.name == "Library").unwrap();
    let kids = cat.list_children(disk.id, Some(lib.id)).unwrap();
    let cloud = kids.iter().find(|i| i.name == "CloudStorage").unwrap();
    let xs = cat.list_children(disk.id, Some(cloud.id)).unwrap();
    assert_eq!(xs[0].child_count, 1, "X ficou só com a pasta real");
}
