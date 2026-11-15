//! Checks the v4 conversion on a real catalog (development tool, not shipped).
//! Usage: cargo run --release -p hdf-core --example v4_check -- <untouched v3 copy> <copy to convert>
use hdf_core::*;
use rusqlite::Connection;
use std::time::Instant;

fn mb(p: &str) -> u64 {
    std::fs::metadata(p).map(|m| m.len() / 1_048_576).unwrap_or(0)
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (old_p, new_p) = (&a[1], &a[2]);
    println!("before: {} MB", mb(new_p));
    let t = Instant::now();
    let cat = Catalog::open(new_p).expect("open/convert");
    println!("conversion: {:.1} s", t.elapsed().as_secs_f64());
    drop(cat);
    println!("after: {} MB (wal {} MB)", mb(new_p), mb(&format!("{new_p}-wal")));
    let cat = Catalog::open(new_p).unwrap();
    let old = Connection::open(old_p).unwrap();

    // Every item: same path and dates as v3 stored them (compared in bulk, in id order).
    let t = Instant::now();
    let mut st = old.prepare("SELECT id, path, created_at, modified_at FROM items ORDER BY id").unwrap();
    let mut rows = st.query([]).unwrap();
    let newc = Connection::open(new_p).unwrap();
    let mut st2 = newc
        .prepare(&format!("SELECT i.id, {} FROM items i {} ORDER BY i.id", hdf_core::db::ITEM_COLS, hdf_core::db::ITEM_JOIN))
        .unwrap();
    let mut rows2 = st2.query([]).unwrap();
    let (mut n, mut bad) = (0u64, 0u64);
    while let Some(r) = rows.next().unwrap() {
        let r2 = rows2.next().unwrap().expect("same count");
        let (id, p, c, m): (i64, String, Option<String>, Option<String>) = (r.get(0).unwrap(), r.get(1).unwrap(), r.get(2).unwrap(), r.get(3).unwrap());
        let (id2, p2, c2, m2): (i64, String, Option<String>, Option<String>) = (r2.get(0).unwrap(), r2.get(5).unwrap(), r2.get(9).unwrap(), r2.get(10).unwrap());
        n += 1;
        if id != id2 || p != p2 || c != c2 || m != m2 {
            bad += 1;
            if bad <= 5 {
                println!("DIFF id {id}: {p:?} {c:?} {m:?}  vs  {id2} {p2:?} {c2:?} {m2:?}");
            }
        }
    }
    assert!(rows2.next().unwrap().is_none(), "extra rows in v4");
    println!("items compared: {n}, different: {bad} ({:.1} s)", t.elapsed().as_secs_f64());

    // Search: same ids as the v3 query (MATCH / LIKE), timing like the app (count + first 500).
    let v3_ids = |text: &str| -> Vec<i64> {
        let norm = hdf_core::db::normalize(text);
        let terms: Vec<&str> = norm.split_whitespace().collect();
        let (long, short): (Vec<&str>, Vec<&str>) = terms.iter().partition(|t| t.chars().count() >= 3);
        let mut w = vec![];
        let mut args: Vec<String> = vec![];
        if !long.is_empty() {
            w.push("f.name_norm MATCH ?".to_string());
            args.push(long.iter().map(|t| format!("\"{}\"", t.replace('"', "\"\""))).collect::<Vec<_>>().join(" AND "));
        }
        for s in short {
            w.push("f.name_norm LIKE ? ESCAPE '\\'".to_string());
            args.push(format!("%{}%", s.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")));
        }
        let sql = format!("SELECT f.rowid FROM items_fts f WHERE {} ORDER BY 1", w.join(" AND "));
        let mut st = old.prepare(&sql).unwrap();
        st.query_map(rusqlite::params_from_iter(args.iter()), |r| r.get(0)).unwrap().map(|x| x.unwrap()).collect()
    };
    let mut all_same = true;
    for text in ["final", "video", "png", "proj", "cliente", "readme", "photos library", "ção", "São Paulo", "50%", "a_b", "[1]", "x", "ab", "IMG 2024 jpg", "claude code", "what?", "file*"] {
        let t = Instant::now();
        let r = cat.search(&SearchQuery { text: text.into(), limit: 500, ..Default::default() }).unwrap();
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        let mut ids: Vec<i64> = cat
            .search(&SearchQuery { text: text.into(), limit: 10_000, ..Default::default() })
            .unwrap()
            .items
            .into_iter()
            .map(|i| i.id)
            .collect();
        ids.sort();
        let want = v3_ids(text);
        let same = r.total as usize == want.len() && (want.len() > 10_000 || ids == want);
        all_same &= same;
        println!("{text:>16}: {:>7} results, {ms:>5.0} ms, same as v3: {same}", r.total);
    }
    println!("ALL SEARCHES SAME: {all_same}");

    let t = Instant::now();
    let d = cat.find_duplicates(&[], 1_048_576).unwrap();
    println!("duplicates >= 1 MB: {} groups, {:.0} ms", d.len(), t.elapsed().as_secs_f64() * 1000.0);
    let t = Instant::now();
    let d = cat.find_duplicates(&[], 0).unwrap();
    println!("duplicates any size: {} groups, {:.0} ms", d.len(), t.elapsed().as_secs_f64() * 1000.0);
    let t = Instant::now();
    let k = cat.list_children(1, None).unwrap();
    println!("root folder: {} items, {:.0} ms", k.len(), t.elapsed().as_secs_f64() * 1000.0);
}
