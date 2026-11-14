//! Busca, navegação, duplicados e caminho real de um item.

use crate::db::{Catalog, ITEM_COLS, normalize, row_to_item};
use crate::error::{Error, Result};
use crate::model::*;
use rusqlite::types::Value;
use rusqlite::{OptionalExtension, params, params_from_iter};
use std::path::PathBuf;

const MAX_LIMIT: u32 = 10_000;
const MAX_DUP_GROUPS: i64 = 1000;

fn like_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

struct Built {
    from: String,
    wher: String,
    args: Vec<Value>,
}

fn build(q: &SearchQuery) -> Built {
    let mut clauses: Vec<String> = Vec::new();
    let mut args: Vec<Value> = Vec::new();
    let norm = normalize(&q.text);
    let terms: Vec<&str> = norm.split_whitespace().collect();
    let (long, short): (Vec<&str>, Vec<&str>) = terms.iter().partition(|t| t.chars().count() >= 3);
    let use_fts = !terms.is_empty();
    if !long.is_empty() {
        let m = long
            .iter()
            .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" AND ");
        clauses.push("f.name_norm MATCH ?".into());
        args.push(Value::Text(m));
    }
    for t in short {
        clauses.push("f.name_norm LIKE ? ESCAPE '\\'".into());
        args.push(Value::Text(format!("%{}%", like_escape(t))));
    }
    if !q.kinds.is_empty() {
        let ph = vec!["?"; q.kinds.len()].join(",");
        clauses.push(format!("i.kind IN ({ph})"));
        args.extend(q.kinds.iter().map(|k| Value::Text(k.as_str().into())));
    }
    if let Some(v) = q.min_bytes {
        clauses.push("i.size_bytes >= ?".into());
        args.push(Value::Integer(v as i64));
    }
    if let Some(v) = q.max_bytes {
        clauses.push("i.size_bytes <= ?".into());
        args.push(Value::Integer(v as i64));
    }
    if let Some(d) = q.modified_after.as_ref().filter(|s| !s.is_empty()) {
        clauses.push("substr(i.modified_at,1,10) >= ?".into());
        args.push(Value::Text(d.chars().take(10).collect()));
    }
    if let Some(d) = q.modified_before.as_ref().filter(|s| !s.is_empty()) {
        clauses.push("substr(i.modified_at,1,10) <= ?".into());
        args.push(Value::Text(d.chars().take(10).collect()));
    }
    if !q.disk_ids.is_empty() {
        let ph = vec!["?"; q.disk_ids.len()].join(",");
        clauses.push(format!("i.disk_id IN ({ph})"));
        args.extend(q.disk_ids.iter().map(|d| Value::Integer(*d)));
    }
    Built {
        from: if use_fts {
            "items_fts f JOIN items i ON i.id = f.rowid".into()
        } else {
            "items i".into()
        },
        wher: if clauses.is_empty() { String::new() } else { format!("WHERE {}", clauses.join(" AND ")) },
        args,
    }
}

impl Catalog {
    /// Busca com texto (parcial, sem acento, sem maiúsculas) + filtros.
    pub fn search(&self, q: &SearchQuery) -> Result<SearchResult> {
        let b = build(q);
        let total: i64 = self.conn.query_row(
            &format!("SELECT COUNT(*) FROM {} {}", b.from, b.wher),
            params_from_iter(b.args.iter()),
            |r| r.get(0),
        )?;
        let limit = if q.limit == 0 { 500 } else { q.limit.min(MAX_LIMIT) };
        let sql = format!(
            "SELECT {ITEM_COLS} FROM {} {} ORDER BY i.name COLLATE NOCASE, i.id LIMIT {} OFFSET {}",
            b.from, b.wher, limit, q.offset
        );
        let mut st = self.conn.prepare(&sql)?;
        let items = st
            .query_map(params_from_iter(b.args.iter()), row_to_item)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(SearchResult { items, total: total as u64 })
    }

    /// Filhos diretos: pastas primeiro, depois por nome (sem diferenciar maiúsculas).
    pub fn list_children(&self, disk_id: i64, parent_id: Option<i64>) -> Result<Vec<Item>> {
        let mut st = self.conn.prepare_cached(&format!(
            "SELECT {ITEM_COLS} FROM items i WHERE i.disk_id=?1 AND i.parent_id IS ?2
             ORDER BY (i.kind != 'folder'), i.name COLLATE NOCASE, i.id"
        ))?;
        let v = st
            .query_map(params![disk_id, parent_id], row_to_item)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    pub fn get_item(&self, id: i64) -> Result<Item> {
        self.conn
            .query_row(
                &format!("SELECT {ITEM_COLS} FROM items i WHERE i.id=?1"),
                [id],
                row_to_item,
            )
            .optional()?
            .ok_or_else(|| Error::msg("Item não encontrado no catálogo."))
    }

    /// Mesmo nome + mesmo tamanho (ADR 0007). Só arquivos (não pastas), tamanho >= `min_bytes`, nos discos escolhidos (vazio = todos).
    /// Grupos do maior para o menor, no máximo 1000.
    pub fn find_duplicates(&self, disk_ids: &[i64], min_bytes: u64) -> Result<Vec<DuplicateGroup>> {
        let mut args: Vec<Value> = vec![Value::Integer(min_bytes as i64)];
        let disk_clause = if disk_ids.is_empty() {
            String::new()
        } else {
            args.extend(disk_ids.iter().map(|d| Value::Integer(*d)));
            format!("AND disk_id IN ({})", vec!["?"; disk_ids.len()].join(","))
        };
        let sql = format!(
            "SELECT name, size_bytes FROM items
             WHERE kind != 'folder' AND size_bytes >= ?1 {disk_clause}
             GROUP BY name, size_bytes HAVING COUNT(*) > 1
             ORDER BY size_bytes DESC, name COLLATE NOCASE LIMIT {MAX_DUP_GROUPS}"
        );
        let mut st = self.conn.prepare(&sql)?;
        let groups: Vec<(String, i64)> = st
            .query_map(params_from_iter(args.iter()), |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;

        let item_sql = format!(
            "SELECT {ITEM_COLS} FROM items i
             WHERE i.name=?1 AND i.size_bytes=?2 AND i.kind != 'folder' {}
             ORDER BY i.disk_id, i.path",
            if disk_ids.is_empty() {
                String::new()
            } else {
                format!("AND i.disk_id IN ({})", vec!["?"; disk_ids.len()].join(","))
            }
        );
        let mut ist = self.conn.prepare(&item_sql)?;
        let mut out = Vec::with_capacity(groups.len());
        for (name, size) in groups {
            let mut a: Vec<Value> = vec![Value::Text(name.clone()), Value::Integer(size)];
            a.extend(disk_ids.iter().map(|d| Value::Integer(*d)));
            let items = ist
                .query_map(params_from_iter(a.iter()), row_to_item)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            out.push(DuplicateGroup { name, size_bytes: size as u64, items });
        }
        Ok(out)
    }

    /// Caminho real do item no disco presente.
    pub fn resolve_item_path(&self, id: i64, mounted: &[MountedVolume]) -> Result<PathBuf> {
        let item = self.get_item(id)?;
        let disk = self.get_disk(item.disk_id)?;
        let vol = mounted
            .iter()
            .find(|v| v.uuid == disk.uuid)
            .ok_or_else(|| Error::msg("O disco não está plugado."))?;
        // The catalog path must stay inside the disk: only plain names, no `..`, not absolute.
        let rel = std::path::Path::new(&item.path);
        if !rel.components().all(|c| matches!(c, std::path::Component::Normal(_))) {
            return Err(Error::msg("O caminho deste item no catálogo não é válido."));
        }
        let p = PathBuf::from(&vol.mount_path).join(rel);
        if !p.exists() {
            return Err(Error::msg("O arquivo não existe mais no disco (o catálogo pode estar desatualizado)."));
        }
        Ok(p)
    }
}
