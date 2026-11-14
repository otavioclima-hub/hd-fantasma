//! Regression tests for the 2026-11-14 security audit (docs/seguranca/).
//! A `.hdfantasma` file can come from someone else, so the import must reject anything odd
//! quickly and leave the catalog untouched; and the app must never delete or overwrite a file
//! that is not an export the person chose.

use hdf_core::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::time::Duration;

fn write(root: &Path, rel: &str, bytes: usize) {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, vec![b'x'; bytes]).unwrap();
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

fn scan(cat: &mut Catalog, v: &MountedVolume) -> Disk {
    let cancel = AtomicBool::new(false);
    let o = ReadOptions { mode: ReadMode::Full, include_hidden: false };
    read_disk_as(cat, v, &o, &cancel, &mut |_| {}).unwrap()
}

/// A valid export of a tiny disk (`a/b.txt`, `a/c.txt`), then tampered with `sql`.
fn tampered_export(dir: &Path, sql: &str) -> PathBuf {
    let data = tempfile::tempdir().unwrap();
    write(data.path(), "a/b.txt", 4);
    write(data.path(), "a/c.txt", 5);
    let mut cat = Catalog::open(dir.join("origem.sqlite")).unwrap();
    let disk = scan(&mut cat, &vol(data.path(), "uuid-hostil"));
    let file = dir.join("hostil.hdfantasma");
    cat.export_disk(disk.id, &file).unwrap();
    let c = rusqlite::Connection::open(&file).unwrap();
    c.execute_batch(sql).unwrap();
    file
}

/// A catalog that already holds one clean disk.
fn victim(dir: &Path) -> (tempfile::TempDir, Catalog) {
    let data = tempfile::tempdir().unwrap();
    write(data.path(), "fotos/casamento.mov", 9);
    let mut cat = Catalog::open(dir.join("vitima.sqlite")).unwrap();
    scan(&mut cat, &vol(data.path(), "uuid-limpo"));
    (data, cat)
}

/// Imports on another thread; fails the test if it does not return in a few seconds.
fn import_bounded(dir: &Path, file: &Path) -> Result<Disk> {
    let (tx, rx) = mpsc::channel();
    let db = dir.join("vitima.sqlite");
    let file = file.to_path_buf();
    std::thread::spawn(move || {
        let mut cat = Catalog::open(db).unwrap();
        let _ = tx.send(cat.import_disk(&file));
    });
    rx.recv_timeout(Duration::from_secs(10)).expect("import_disk travou (laço sem fim)")
}

fn assert_rejected_and_untouched(sql: &str) {
    let s = tempfile::tempdir().unwrap();
    let file = tampered_export(s.path(), sql);
    let (_d, cat) = victim(s.path());
    drop(cat);
    assert!(import_bounded(s.path(), &file).is_err(), "deveria recusar: {sql}");
    let cat = Catalog::open(s.path().join("vitima.sqlite")).unwrap();
    assert_eq!(cat.list_disks(&[]).unwrap().len(), 1, "catálogo mudou: {sql}");
    let all = cat.search(&SearchQuery::default()).unwrap();
    assert_eq!(all.total, 2, "busca mudou: {sql}");
    let one = cat.search(&SearchQuery { text: "casamento".into(), ..Default::default() }).unwrap();
    assert_eq!(one.total, 1);
}

#[test]
fn import_rejects_folder_that_is_its_own_parent() {
    assert_rejected_and_untouched("UPDATE items SET parent_id=id WHERE name='a'");
}

#[test]
fn import_rejects_parent_cycle() {
    assert_rejected_and_untouched(
        "UPDATE items SET parent_id=(SELECT id FROM items WHERE name='b.txt') WHERE name='a';",
    );
}

#[test]
fn import_rejects_missing_parent() {
    assert_rejected_and_untouched("UPDATE items SET parent_id=999999 WHERE name='b.txt'");
}

#[test]
fn import_rejects_wrong_column_types() {
    for sql in [
        "UPDATE items SET size_bytes='abc' WHERE name='b.txt'",
        "UPDATE items SET child_count='abc' WHERE name='a'",
        "UPDATE items SET size_bytes=-5 WHERE name='b.txt'",
        "UPDATE items SET name=x'ff00fe' WHERE name='b.txt'",
        "UPDATE items SET path=x'ff00' WHERE name='b.txt'",
        "UPDATE items SET kind=x'01' WHERE name='b.txt'",
        "UPDATE items SET id=-7 WHERE name='c.txt'",
        "UPDATE disks SET capacity_bytes='muito'",
        "UPDATE disks SET name=x'00ff'",
    ] {
        assert_rejected_and_untouched(sql);
    }
}

#[test]
fn import_rejects_paths_that_leave_the_disk() {
    for sql in [
        "UPDATE items SET path='/etc/passwd' WHERE name='b.txt'",
        "UPDATE items SET path='../fora.txt' WHERE name='b.txt'",
        "UPDATE items SET path='a/../../fora.txt' WHERE name='b.txt'",
        "UPDATE items SET path='a/..' WHERE name='b.txt'",
        "UPDATE items SET name='x/y' WHERE name='b.txt'",
    ] {
        assert_rejected_and_untouched(sql);
    }
}

#[test]
fn import_never_writes_next_to_the_file() {
    let s = tempfile::tempdir().unwrap();
    // WAL mode is the case where SQLite would normally create -wal/-shm files beside it.
    let file = tampered_export(s.path(), "PRAGMA journal_mode=WAL; UPDATE disks SET name='W';");
    let before: Vec<_> = sorted_names(s.path());
    let (_d, cat) = victim(s.path());
    drop(cat);
    let bytes = fs::read(&file).unwrap();
    let imported = import_bounded(s.path(), &file).unwrap();
    assert_eq!(imported.uuid, "uuid-hostil");
    assert_eq!(fs::read(&file).unwrap(), bytes, "o arquivo importado foi alterado");
    let after: Vec<_> = sorted_names(s.path())
        .into_iter()
        .filter(|n| !n.starts_with("vitima.sqlite"))
        .collect();
    assert_eq!(after, before, "a importação criou arquivos ao lado da exportação");
}

fn sorted_names(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> =
        fs::read_dir(dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    v.sort();
    v
}

#[test]
fn export_only_replaces_an_export_file() {
    let s = tempfile::tempdir().unwrap();
    let (_d, cat) = victim(s.path());
    let disk = cat.list_disks(&[]).unwrap().remove(0);

    // Any other file stays exactly as it was.
    let precious = s.path().join("contrato.pdf");
    fs::write(&precious, b"importante").unwrap();
    assert!(cat.export_disk(disk.id, &precious).is_err());
    assert_eq!(fs::read(&precious).unwrap(), b"importante");

    // The live catalog is refused, even through a link ending in .hdfantasma.
    assert!(cat.export_disk(disk.id, s.path().join("vitima.sqlite")).is_err());
    #[cfg(unix)]
    {
        let link = s.path().join("atalho.hdfantasma");
        std::os::unix::fs::symlink(s.path().join("vitima.sqlite"), &link).unwrap();
        assert!(cat.export_disk(disk.id, &link).is_err());
    }
    assert_eq!(cat.search(&SearchQuery::default()).unwrap().total, 2);

    // A folder is never removed.
    let folder = s.path().join("pasta.hdfantasma");
    fs::create_dir(&folder).unwrap();
    fs::write(folder.join("dentro.txt"), b"x").unwrap();
    assert!(cat.export_disk(disk.id, &folder).is_err());
    assert!(folder.join("dentro.txt").exists());

    // The normal case still works, including replacing an older export.
    let ok = s.path().join("disco.HDFantasma");
    cat.export_disk(disk.id, &ok).unwrap();
    cat.export_disk(disk.id, &ok).unwrap();
    assert!(ok.is_file());
}

#[test]
fn reveal_path_stays_inside_the_disk() {
    let s = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    write(data.path(), "a/b.txt", 4);
    write(outside.path(), "segredo.txt", 4);
    let db = s.path().join("c.sqlite");
    let mut cat = Catalog::open(&db).unwrap();
    let v = vol(data.path(), "uuid-1");
    scan(&mut cat, &v);
    let item = cat.search(&SearchQuery { text: "b.txt".into(), ..Default::default() }).unwrap().items.remove(0);
    assert!(cat.resolve_item_path(item.id, &[v.clone()]).unwrap().ends_with("a/b.txt"));

    // A catalog edited by hand (or by another program) must not point the reveal elsewhere.
    let raw = rusqlite::Connection::open(&db).unwrap();
    for bad in [
        outside.path().join("segredo.txt").to_string_lossy().into_owned(),
        format!("../{}/segredo.txt", outside.path().file_name().unwrap().to_string_lossy()),
    ] {
        raw.execute("UPDATE items SET path=?1 WHERE id=?2", rusqlite::params![bad, item.id]).unwrap();
        assert!(cat.resolve_item_path(item.id, &[v.clone()]).is_err(), "aceitou {bad}");
    }
}

// ---------------------------------------------------------------- audit run-2 (2026-11-14)

#[test]
fn import_rejects_views_and_triggers() {
    // A view can return clean rows while being checked and other rows while being copied.
    assert_rejected_and_untouched(
        "ALTER TABLE items RENAME TO items_real;
         CREATE VIEW items AS SELECT * FROM items_real;",
    );
    assert_rejected_and_untouched(
        "CREATE TRIGGER t AFTER INSERT ON disks BEGIN SELECT 1; END;",
    );
}

#[test]
fn import_rejects_ids_near_the_integer_limit() {
    assert_rejected_and_untouched(
        "UPDATE items_fts SET rowid=9223372036854775807 WHERE rowid=(SELECT id FROM items WHERE name='c.txt');
         UPDATE items SET id=9223372036854775807 WHERE name='c.txt';",
    );
}

#[test]
fn import_rejects_huge_names() {
    assert_rejected_and_untouched("UPDATE items SET name=printf('%.2000c','x') WHERE name='b.txt'");
}

#[test]
fn import_rebuilds_search_text_from_names() {
    let s = tempfile::tempdir().unwrap();
    // A file can carry any search text; it must not make an item match words it does not have.
    let file = tampered_export(s.path(), "UPDATE items_fts SET name_norm='senha banco contrato'");
    let (_d, cat) = victim(s.path());
    drop(cat);
    import_bounded(s.path(), &file).unwrap();
    let cat = Catalog::open(s.path().join("vitima.sqlite")).unwrap();
    let q = |t: &str| cat.search(&SearchQuery { text: t.into(), ..Default::default() }).unwrap().total;
    assert_eq!(q("contrato"), 0);
    assert_eq!(q("b.txt"), 1);
}

#[test]
fn deep_folder_chain_imports_in_linear_time() {
    let s = tempfile::tempdir().unwrap();
    let n = 20_000;
    let file = tampered_export(
        s.path(),
        &format!(
            "DELETE FROM items_fts; DELETE FROM items;
             WITH RECURSIVE c(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM c WHERE i<{n})
             INSERT INTO items(id,disk_id,parent_id,name,path,kind,ext,size_bytes,created_at,modified_at,child_count,descendant_count)
             SELECT i, (SELECT id FROM disks), CASE WHEN i=1 THEN NULL ELSE i-1 END, 'd', 'd', 'folder', NULL, 0, NULL, NULL, 1, 0 FROM c;"
        ),
    );
    let (_d, cat) = victim(s.path());
    drop(cat);
    let t0 = std::time::Instant::now();
    let d = import_bounded(s.path(), &file).unwrap();
    assert!(t0.elapsed() < Duration::from_secs(8), "lento demais: {:?}", t0.elapsed());
    let cat = Catalog::open(s.path().join("vitima.sqlite")).unwrap();
    let top = cat.list_children(d.id, None).unwrap();
    assert_eq!(top[0].descendant_count, (n - 1) as u64);
}

#[test]
fn catalog_from_a_newer_version_is_refused_untouched() {
    let s = tempfile::tempdir().unwrap();
    let db = s.path().join("c.sqlite");
    drop(Catalog::open(&db).unwrap());
    rusqlite::Connection::open(&db).unwrap().execute("UPDATE schema_version SET version=99", []).unwrap();
    let e = Catalog::open(&db).err().expect("devia recusar").to_string();
    assert_eq!(e, hdf_core::db::MSG_NEWER_CATALOG);
    let v: i64 = rusqlite::Connection::open(&db).unwrap().query_row("SELECT version FROM schema_version", [], |r| r.get(0)).unwrap();
    assert_eq!(v, 99);
}
