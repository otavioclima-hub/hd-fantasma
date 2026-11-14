//! Motor do HD Fantasma: catálogo (SQLite + FTS5), Leitura de discos, busca,
//! duplicados e exportação/importação. Sem dependência de Tauri.

pub mod db;
pub mod error;
pub mod model;
pub mod scan;
pub mod search;
pub mod volumes;

pub use db::Catalog;
pub use error::{Error, Result};
pub use model::*;
pub use scan::{ArchiveEntry, DetailExtractor, Details, NoopExtractor, read_disk, read_disk_as};
