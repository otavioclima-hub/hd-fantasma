//! Catalog format v4 (ADR 0018): smaller storage, same answers.

use hdf_core::*;
use std::path::Path;

fn q(text: &str) -> SearchQuery {
    SearchQuery { text: text.into(), ..Default::default() }
}

/// A catalog exactly as v3 wrote it: full path on every item, ISO dates, positional FTS.
fn write_v3(path: &Path) {
    let c = rusqlite::Connection::open(path).unwrap();
    c.execute_batch(
        "CREATE TABLE schema_version (version INTEGER NOT NULL);
         INSERT INTO schema_version VALUES (3);
         CREATE TABLE disks (id INTEGER PRIMARY KEY, uuid TEXT NOT NULL UNIQUE, name TEXT NOT NULL,
           filesystem TEXT NOT NULL DEFAULT '', capacity_bytes INTEGER NOT NULL DEFAULT 0,
           free_bytes INTEGER NOT NULL DEFAULT 0, item_count INTEGER NOT NULL DEFAULT 0,
           total_bytes INTEGER NOT NULL DEFAULT 0, last_read_at TEXT NOT NULL);
         CREATE TABLE items (id INTEGER PRIMARY KEY, disk_id INTEGER NOT NULL REFERENCES disks(id) ON DELETE CASCADE,
           parent_id INTEGER, name TEXT NOT NULL, path TEXT NOT NULL, kind TEXT NOT NULL, ext TEXT,
           size_bytes INTEGER NOT NULL DEFAULT 0, created_at TEXT, modified_at TEXT,
           child_count INTEGER NOT NULL DEFAULT 0, descendant_count INTEGER NOT NULL DEFAULT 0);
         CREATE INDEX idx_items_parent ON items(disk_id, parent_id);
         CREATE INDEX idx_items_dup ON items(name, size_bytes);
         CREATE VIRTUAL TABLE items_fts USING fts5(name_norm, tokenize='trigram');
         CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
         INSERT INTO disks VALUES (1,'u1','VELHO','apfs',1000,500,5,350,'2025-01-01T00:00:00Z');
         INSERT INTO items(id,disk_id,parent_id,name,path,kind,ext,size_bytes,child_count,descendant_count,created_at,modified_at) VALUES
           (1,1,NULL,'Projetos','Projetos','folder',NULL,300,2,3,'2024-01-01T10:00:00Z','2024-02-01T10:00:00Z'),
           (2,1,1,'Vídeo Final [v2].mov','Projetos/Vídeo Final [v2].mov','video','mov',200,0,0,'2024-03-01T12:30:00Z','2024-03-05T23:59:59Z'),
           (3,1,1,'2025','Projetos/2025','folder',NULL,100,1,1,NULL,'2025-01-01T00:00:00Z'),
           (4,1,3,'50%_off*.png','Projetos/2025/50%_off*.png','image','png',100,0,0,NULL,'2025-06-10T08:00:00Z'),
           (5,1,NULL,'leia-me.txt','leia-me.txt','document','txt',50,0,0,NULL,NULL);
         INSERT INTO items_fts(rowid,name_norm) VALUES
           (1,'projetos'),(2,'video final [v2].mov'),(3,'2025'),(4,'50%_off*.png'),(5,'leia-me.txt');",
    )
    .unwrap();
}

#[test]
fn v3_catalog_is_converted_and_answers_the_same() {
    let store = tempfile::tempdir().unwrap();
    let dbp = store.path().join("v3.sqlite");
    write_v3(&dbp);
    let cat = Catalog::open(&dbp).unwrap();

    // Paths and dates come back exactly as before.
    let it = cat.get_item(2).unwrap();
    assert_eq!(it.path, "Projetos/Vídeo Final [v2].mov");
    assert_eq!(it.created_at.as_deref(), Some("2024-03-01T12:30:00Z"));
    assert_eq!(it.modified_at.as_deref(), Some("2024-03-05T23:59:59Z"));
    assert_eq!(cat.get_item(4).unwrap().path, "Projetos/2025/50%_off*.png");
    assert_eq!(cat.get_item(5).unwrap().path, "leia-me.txt");
    assert_eq!(cat.get_item(5).unwrap().modified_at, None);
    assert_eq!(cat.get_item(3).unwrap().path, "Projetos/2025");

    // Stored compactly: files keep no path, dates are integers.
    let c = rusqlite::Connection::open(&dbp).unwrap();
    let file_paths: i64 = c.query_row("SELECT COUNT(*) FROM items WHERE kind!='folder' AND path IS NOT NULL", [], |r| r.get(0)).unwrap();
    assert_eq!(file_paths, 0);
    let text_dates: i64 = c
        .query_row("SELECT COUNT(*) FROM items WHERE typeof(modified_at) NOT IN ('integer','null')", [], |r| r.get(0))
        .unwrap();
    assert_eq!(text_dates, 0);
    let auto_vacuum: i64 = c.query_row("PRAGMA auto_vacuum", [], |r| r.get(0)).unwrap();
    assert_eq!(auto_vacuum, 2, "incremental auto-vacuum");

    // Search: accents, brackets, % _ *, several words, short words.
    let names = |t: &str| -> Vec<String> {
        let mut v: Vec<String> = cat.search(&q(t)).unwrap().items.into_iter().map(|i| i.name).collect();
        v.sort();
        v
    };
    assert_eq!(names("video"), vec!["Vídeo Final [v2].mov"]);
    assert_eq!(names("FINAL mov"), vec!["Vídeo Final [v2].mov"]);
    assert_eq!(names("[v2]"), vec!["Vídeo Final [v2].mov"]);
    assert_eq!(names("50%"), vec!["50%_off*.png"]);
    assert_eq!(names("_off*"), vec!["50%_off*.png"]);
    assert_eq!(names("f*"), vec!["50%_off*.png"]);
    assert_eq!(names("v?"), Vec::<String>::new(), "? is a character, not a wildcard");
    assert_eq!(names("le"), vec!["leia-me.txt"]);
    assert_eq!(names("nada disso"), Vec::<String>::new());

    // Date filters: whole days, inclusive at both ends.
    let by_date = |after: Option<&str>, before: Option<&str>| -> Vec<i64> {
        let mut v: Vec<i64> = cat
            .search(&SearchQuery {
                modified_after: after.map(Into::into),
                modified_before: before.map(Into::into),
                ..Default::default()
            })
            .unwrap()
            .items
            .into_iter()
            .map(|i| i.id)
            .collect();
        v.sort();
        v
    };
    assert_eq!(by_date(Some("2024-03-05"), Some("2024-03-05")), vec![2]);
    assert_eq!(by_date(Some("2025-01-01"), None), vec![3, 4]);
    assert_eq!(by_date(None, Some("2024-02-01")), vec![1]);

    // Browsing a folder still shows full paths.
    let kids = cat.list_children(1, Some(1)).unwrap();
    assert_eq!(kids.iter().map(|k| k.path.as_str()).collect::<Vec<_>>(), vec!["Projetos/2025", "Projetos/Vídeo Final [v2].mov"]);
}

#[test]
fn a_v3_export_imports_into_a_v4_catalog() {
    let store = tempfile::tempdir().unwrap();
    let old = store.path().join("velho.hdfantasma");
    write_v3(&old);
    let mut cat = Catalog::open(store.path().join("novo.sqlite")).unwrap();
    let d = cat.import_disk(&old).unwrap();
    assert_eq!(d.name, "VELHO");
    let hits = cat.search(&q("final")).unwrap().items;
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].path, "Projetos/Vídeo Final [v2].mov");
    assert_eq!(hits[0].modified_at.as_deref(), Some("2024-03-05T23:59:59Z"));

    // And a v4 export of it round-trips.
    let out = store.path().join("de-novo.hdfantasma");
    cat.export_disk(d.id, &out).unwrap();
    let mut cat2 = Catalog::open(store.path().join("outro.sqlite")).unwrap();
    let d2 = cat2.import_disk(&out).unwrap();
    let hits = cat2.search(&q("50%")).unwrap().items;
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].path, "Projetos/2025/50%_off*.png");
    assert_eq!(cat2.get_disk(d2.id).unwrap().item_count, 5);
}

#[test]
fn a_v4_export_with_a_folder_without_path_is_refused() {
    let store = tempfile::tempdir().unwrap();
    let mut cat = Catalog::open(store.path().join("c.sqlite")).unwrap();
    let bad = store.path().join("ruim.hdfantasma");
    {
        let e = Catalog::open(&bad).unwrap();
        drop(e);
        let c = rusqlite::Connection::open(&bad).unwrap();
        c.execute_batch(
            "INSERT INTO disks(id,uuid,name,last_read_at) VALUES (1,'x','X','2025-01-01T00:00:00Z');
             INSERT INTO items(id,disk_id,parent_id,name,path,kind) VALUES (1,1,NULL,'Pasta',NULL,'folder'),
                                                                    (2,1,1,'a.txt',NULL,'document');",
        )
        .unwrap();
    }
    assert!(cat.import_disk(&bad).is_err());
    let bad2 = store.path().join("ruim2.hdfantasma");
    {
        drop(Catalog::open(&bad2).unwrap());
        let c = rusqlite::Connection::open(&bad2).unwrap();
        c.execute_batch(
            "INSERT INTO disks(id,uuid,name,last_read_at) VALUES (1,'y','Y','2025-01-01T00:00:00Z');
             INSERT INTO items(id,disk_id,parent_id,name,path,kind,modified_at) VALUES (1,1,NULL,'a.txt',NULL,'document','ontem');",
        )
        .unwrap();
    }
    assert!(cat.import_disk(&bad2).is_err(), "text date in a v4 export");
}

#[test]
fn a_fresh_catalog_is_v4_with_incremental_vacuum() {
    let store = tempfile::tempdir().unwrap();
    let p = store.path().join("c.sqlite");
    drop(Catalog::open(&p).unwrap());
    let c = rusqlite::Connection::open(&p).unwrap();
    let av: i64 = c.query_row("PRAGMA auto_vacuum", [], |r| r.get(0)).unwrap();
    assert_eq!(av, 2);
    let sql: String = c.query_row("SELECT sql FROM sqlite_master WHERE name='items_fts'", [], |r| r.get(0)).unwrap();
    assert!(sql.contains("detail=none"), "{sql}");
}
