//! Tipos do contrato motor <-> telas (docs/CONTRATO.md). JSON em camelCase.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Folder,
    Video,
    Image,
    Audio,
    Document,
    Archive,
    Other,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Folder => "folder",
            Kind::Video => "video",
            Kind::Image => "image",
            Kind::Audio => "audio",
            Kind::Document => "document",
            Kind::Archive => "archive",
            Kind::Other => "other",
        }
    }

    pub fn parse(s: &str) -> Kind {
        match s {
            "folder" => Kind::Folder,
            "video" => Kind::Video,
            "image" => Kind::Image,
            "audio" => Kind::Audio,
            "document" => Kind::Document,
            "archive" => Kind::Archive,
            _ => Kind::Other,
        }
    }

    /// Tipo a partir da extensão (já em minúsculas, sem ponto).
    pub fn from_ext(ext: Option<&str>) -> Kind {
        let Some(e) = ext else { return Kind::Other };
        match e {
            "mov" | "mp4" | "mxf" | "mkv" | "avi" | "m4v" | "r3d" | "braw" | "mts" | "m2ts"
            | "webm" | "wmv" | "mpg" | "mpeg" | "flv" | "ari" | "crm" | "3gp" => Kind::Video,
            "jpg" | "jpeg" | "png" | "gif" | "bmp" | "tif" | "tiff" | "psd" | "psb" | "exr"
            | "dpx" | "heic" | "heif" | "webp" | "svg" | "ai" | "eps" | "raw" | "cr2" | "cr3"
            | "nef" | "arw" | "dng" | "orf" | "rw2" | "raf" | "tga" | "ico" => Kind::Image,
            "wav" | "aif" | "aiff" | "mp3" | "aac" | "flac" | "m4a" | "ogg" | "opus" | "wma"
            | "bwf" => Kind::Audio,
            "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt" | "rtf" | "csv"
            | "md" | "aep" | "prproj" | "drp" | "key" | "pages" | "numbers" | "odt" | "ods"
            | "odp" | "indd" | "fcpxml" => Kind::Document,
            "zip" | "rar" | "7z" | "tar" | "gz" | "tgz" | "bz2" | "xz" => Kind::Archive,
            _ => Kind::Other,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Disk {
    pub id: i64,
    pub uuid: String,
    pub name: String,
    pub filesystem: String,
    pub capacity_bytes: u64,
    pub free_bytes: u64,
    pub item_count: u64,
    pub total_bytes: u64,
    pub last_read_at: String,
    pub present: bool,
    pub mount_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: i64,
    pub disk_id: i64,
    pub parent_id: Option<i64>,
    pub name: String,
    pub path: String,
    pub kind: Kind,
    pub ext: Option<String>,
    pub size_bytes: u64,
    pub created_at: Option<String>,
    pub modified_at: Option<String>,
    pub child_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SearchQuery {
    pub text: String,
    pub kinds: Vec<Kind>,
    pub min_bytes: Option<u64>,
    pub max_bytes: Option<u64>,
    pub modified_after: Option<String>,
    pub modified_before: Option<String>,
    pub disk_ids: Vec<i64>,
    pub limit: u32,
    pub offset: u32,
}

impl Default for SearchQuery {
    fn default() -> Self {
        SearchQuery {
            text: String::new(),
            kinds: vec![],
            min_bytes: None,
            max_bytes: None,
            modified_after: None,
            modified_before: None,
            disk_ids: vec![],
            limit: 500,
            offset: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub items: Vec<Item>,
    pub total: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateGroup {
    pub name: String,
    pub size_bytes: u64,
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MountedVolume {
    pub uuid: String,
    pub name: String,
    pub mount_path: String,
    pub filesystem: String,
    pub capacity_bytes: u64,
    pub free_bytes: u64,
    pub cataloged_disk_id: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReadMode {
    Quick,
    Full,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadOptions {
    pub mode: ReadMode,
    pub include_hidden: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Listing,
    Saving,
    Done,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadProgress {
    pub disk_uuid: String,
    pub phase: Phase,
    pub items_seen: u64,
    pub current_path: Option<String>,
    pub message: Option<String>,
    /// Espaço real ocupado (alocado) somado até agora; arquivos só na nuvem contam 0.
    pub bytes_seen: u64,
    /// Espaço ocupado do volume no início (capacidade − livre); 0 se desconhecido.
    pub bytes_expected: u64,
    /// `current_path` está numa pasta de nuvem (CloudStorage, Mobile Documents).
    pub in_cloud: bool,
}

impl ReadProgress {
    /// Progresso "vazio" (sem bytes), para erros e testes.
    pub fn new(disk_uuid: String, phase: Phase, items_seen: u64, message: Option<String>) -> Self {
        ReadProgress {
            disk_uuid,
            phase,
            items_seen,
            current_path: None,
            message,
            bytes_seen: 0,
            bytes_expected: 0,
            in_cloud: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Pt,
    En,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub language: Language,
    pub include_hidden: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            language: Language::Pt,
            include_hidden: false,
        }
    }
}
