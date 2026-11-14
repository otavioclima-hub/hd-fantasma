//! As 7 ferramentas do MCP: definições (nome, descrição, esquema) e execução.

use crate::server::Server;
use crate::util::*;
use hdf_core::{Catalog, Disk, Item, ReadMode, ReadOptions, SearchQuery};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

type R = Result<Value, String>;

const NAMES: [&str; 7] = [
    "listar_discos",
    "buscar_arquivos",
    "detalhes_item",
    "listar_pasta",
    "achar_duplicados",
    "catalogar_disco",
    "discos_plugados",
];

pub fn exists(name: &str) -> bool {
    NAMES.contains(&name)
}

pub fn tool_list() -> Value {
    let disks_prop = json!({
        "type": "array", "items": { "type": ["string", "integer"] }, "maxItems": 100,
        "description": "Nomes (ou ids) dos discos. Vazio ou ausente = todos os discos."
    });
    json!([
        {
            "name": "listar_discos",
            "description": "Lista todos os discos do catálogo do HD Fantasma (HDs, SSDs, pen drives, cartões já lidos), com id, nome, se está presente (plugado agora) ou ausente, capacidade, espaço usado, quantidade de itens e data da última leitura. Comece por aqui para saber quais discos existem e quais ids usar nas outras ferramentas.",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false },
            "annotations": { "readOnlyHint": true, "openWorldHint": false }
        },
        {
            "name": "buscar_arquivos",
            "description": "Procura arquivos e pastas em TODOS os discos do catálogo, mesmo os desplugados. O texto casa com parte do nome, sem diferenciar maiúsculas nem acentos (várias palavras = todas precisam aparecer). Filtros opcionais: tipo, tamanho, data de modificação e discos. Cada resultado traz nome, disco, caminho completo dentro do disco, tipo, tamanho, data e se o disco está presente. Se 'total' for maior que o que veio, use 'deslocamento' para ver mais.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "texto": { "type": "string", "maxLength": 200, "description": "Parte do nome. Vazio = só filtros." },
                    "tipos": { "type": "array", "maxItems": 7, "items": { "type": "string", "enum": ["pasta", "video", "imagem", "audio", "documento", "compactado", "outro"] }, "description": "Só estes tipos (pelo formato do arquivo). Vazio = todos." },
                    "tamanhoMin": { "type": ["integer", "string"], "description": "Tamanho mínimo: bytes ou texto com unidade, ex.: \"2GB\", \"500 MB\"." },
                    "tamanhoMax": { "type": ["integer", "string"], "description": "Tamanho máximo: bytes ou texto com unidade." },
                    "modificadoDepois": { "type": "string", "description": "Modificado nesta data ou depois (AAAA-MM-DD)." },
                    "modificadoAntes": { "type": "string", "description": "Modificado nesta data ou antes (AAAA-MM-DD)." },
                    "discos": disks_prop,
                    "limite": { "type": "integer", "minimum": 1, "maximum": 500, "default": 50, "description": "Quantos resultados trazer (padrão 50, máximo 500)." },
                    "deslocamento": { "type": "integer", "minimum": 0, "default": 0, "description": "Quantos resultados pular (para paginar)." }
                },
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": true, "openWorldHint": false }
        },
        {
            "name": "detalhes_item",
            "description": "Mostra todos os dados de um arquivo ou pasta do catálogo: nome, caminho, tipo, tamanho, datas de criação e modificação, disco, e para pastas quantos itens há dentro (diretos e no total). Informe o 'id' (vindo de buscar_arquivos ou listar_pasta) OU 'disco' + 'caminho'.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": { "type": "integer", "minimum": 1, "description": "Id do item no catálogo." },
                    "disco": { "type": ["string", "integer"], "description": "Nome ou id do disco (use junto com 'caminho')." },
                    "caminho": { "type": "string", "maxLength": 4096, "description": "Caminho dentro do disco, com barras, ex.: \"Projetos/2025/video.mov\"." }
                },
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": true, "openWorldHint": false }
        },
        {
            "name": "listar_pasta",
            "description": "Lista o conteúdo de uma pasta de um disco do catálogo (pastas primeiro, depois arquivos por nome), funcionando também com o disco desplugado. Sem 'caminho' mostra a raiz do disco.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "disco": { "type": ["string", "integer"], "description": "Nome ou id do disco." },
                    "caminho": { "type": "string", "maxLength": 4096, "description": "Caminho da pasta dentro do disco. Vazio = raiz." },
                    "limite": { "type": "integer", "minimum": 1, "maximum": 500, "default": 100, "description": "Quantos itens trazer (padrão 100, máximo 500)." },
                    "deslocamento": { "type": "integer", "minimum": 0, "default": 0, "description": "Quantos itens pular." }
                },
                "required": ["disco"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": true, "openWorldHint": false }
        },
        {
            "name": "achar_duplicados",
            "description": "Acha arquivos duplicados (mesmo nome e mesmo tamanho) entre os discos do catálogo, dos maiores para os menores, mostrando onde está cada cópia e quanto espaço dá para liberar. Só arquivos (não pastas). Não compara o conteúdo.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "discos": disks_prop,
                    "tamanhoMinimo": { "type": ["integer", "string"], "description": "Ignora arquivos menores que isto. Padrão \"100MB\". Bytes ou texto com unidade." },
                    "limiteGrupos": { "type": "integer", "minimum": 1, "maximum": 200, "default": 20, "description": "Quantos grupos de duplicados trazer (padrão 20, máximo 200)." }
                },
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": true, "openWorldHint": false }
        },
        {
            "name": "catalogar_disco",
            "description": "Lê um disco que está plugado agora e grava (ou atualiza) tudo o que há nele no catálogo: nomes, tipos, tamanhos, datas e pastas. Não abre arquivos, não escreve no disco e não usa a internet. Pode demorar minutos em discos grandes. Informe o caminho de montagem (ex.: \"/Volumes/MEU_HD\") ou o nome do disco; só aceita a raiz de um disco montado. Veja discos_plugados para saber o que pode ser lido.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "disco": { "type": "string", "maxLength": 1024, "description": "Caminho de montagem do disco (\"/Volumes/NOME\") ou o nome do disco plugado." },
                    "incluirOcultos": { "type": "boolean", "default": false, "description": "Incluir arquivos de sistema (.DS_Store etc.). Padrão: não." }
                },
                "required": ["disco"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false }
        },
        {
            "name": "discos_plugados",
            "description": "Lista os discos plugados no computador agora (inclusive o interno) e diz se cada um já está no catálogo. Serve para escolher o que passar para catalogar_disco.",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false },
            "annotations": { "readOnlyHint": true, "openWorldHint": false }
        }
    ])
}

// ---------------------------------------------------------------- formatação

/// Disco presente/ausente por id, para legendar resultados.
struct DiskIndex(HashMap<i64, (String, bool, Option<String>)>);

impl DiskIndex {
    fn new(disks: &[Disk]) -> Self {
        DiskIndex(disks.iter().map(|d| (d.id, (d.name.clone(), d.present, d.mount_path.clone()))).collect())
    }
    fn name(&self, id: i64) -> &str {
        self.0.get(&id).map(|d| d.0.as_str()).unwrap_or("?")
    }
    fn present(&self, id: i64) -> bool {
        self.0.get(&id).map(|d| d.1).unwrap_or(false)
    }
}

fn size_fields(m: &mut serde_json::Map<String, Value>, key: &str, text_key: &str, bytes: u64) {
    m.insert(key.into(), json!(bytes));
    m.insert(text_key.into(), json!(human_size(bytes)));
}

fn disk_json(d: &Disk) -> Value {
    let used = d.capacity_bytes.saturating_sub(d.free_bytes);
    let mut m = serde_json::Map::new();
    m.insert("id".into(), json!(d.id));
    m.insert("nome".into(), json!(d.name));
    m.insert("presente".into(), json!(d.present));
    if let Some(p) = &d.mount_path {
        m.insert("caminhoMontagem".into(), json!(p));
    }
    m.insert("sistemaArquivos".into(), json!(d.filesystem));
    size_fields(&mut m, "capacidadeBytes", "capacidade", d.capacity_bytes);
    size_fields(&mut m, "usadoBytes", "usado", used);
    size_fields(&mut m, "livreBytes", "livre", d.free_bytes);
    size_fields(&mut m, "catalogadoBytes", "catalogado", d.total_bytes);
    m.insert("itens".into(), json!(d.item_count));
    m.insert("ultimaLeitura".into(), json!(d.last_read_at));
    m.insert("uuid".into(), json!(d.uuid));
    Value::Object(m)
}

fn item_json(it: &Item, idx: &DiskIndex, detailed: bool) -> Value {
    let mut m = serde_json::Map::new();
    m.insert("id".into(), json!(it.id));
    m.insert("nome".into(), json!(it.name));
    m.insert("disco".into(), json!(idx.name(it.disk_id)));
    m.insert("discoId".into(), json!(it.disk_id));
    m.insert("caminho".into(), json!(it.path));
    m.insert("tipo".into(), json!(kind_pt(it.kind)));
    if let Some(e) = &it.ext {
        m.insert("extensao".into(), json!(e));
    }
    size_fields(&mut m, "tamanhoBytes", "tamanho", it.size_bytes);
    m.insert("modificadoEm".into(), json!(it.modified_at));
    m.insert("presente".into(), json!(idx.present(it.disk_id)));
    if detailed {
        m.insert("criadoEm".into(), json!(it.created_at));
        m.insert("pastaPaiId".into(), json!(it.parent_id));
    }
    if it.kind == hdf_core::Kind::Folder {
        m.insert("itensDiretos".into(), json!(it.child_count));
        m.insert("itensNoTotal".into(), json!(it.descendant_count));
    }
    Value::Object(m)
}

// ---------------------------------------------------------------- resolução

fn norm_eq(a: &str, b: &str) -> bool {
    hdf_core::db::normalize(a) == hdf_core::db::normalize(b)
}

fn disk_list_hint(disks: &[Disk]) -> String {
    if disks.is_empty() {
        return "O catálogo não tem nenhum disco ainda.".into();
    }
    let names: Vec<String> = disks.iter().take(20).map(|d| format!("\"{}\" (id {})", d.name, d.id)).collect();
    format!("Discos do catálogo: {}.", names.join(", "))
}

fn resolve_disk(v: &Value, disks: &[Disk]) -> Result<i64, String> {
    match v {
        Value::Number(n) => {
            let id = n.as_i64().ok_or("Id de disco inválido.")?;
            disks.iter().find(|d| d.id == id).map(|d| d.id).ok_or_else(|| {
                format!("Não existe disco com id {id}. {}", disk_list_hint(disks))
            })
        }
        Value::String(s) => {
            let s = s.trim();
            if s.is_empty() || s.chars().count() > MAX_TEXT {
                return Err("Nome de disco inválido.".into());
            }
            let exact: Vec<&Disk> = disks.iter().filter(|d| norm_eq(&d.name, s)).collect();
            match exact.len() {
                1 => return Ok(exact[0].id),
                n if n > 1 => {
                    return Err(format!("Mais de um disco se chama \"{s}\": use o id. {}", disk_list_hint(disks)));
                }
                _ => {}
            }
            let ns = hdf_core::db::normalize(s);
            let part: Vec<&Disk> = disks.iter().filter(|d| hdf_core::db::normalize(&d.name).contains(&ns)).collect();
            if part.len() == 1 {
                return Ok(part[0].id);
            }
            if let Ok(id) = s.parse::<i64>() {
                if let Some(d) = disks.iter().find(|d| d.id == id) {
                    return Ok(d.id);
                }
            }
            if part.len() > 1 {
                return Err(format!("\"{s}\" combina com mais de um disco. {}", disk_list_hint(disks)));
            }
            Err(format!("Não achei o disco \"{s}\". {}", disk_list_hint(disks)))
        }
        _ => Err("Disco: informe o nome (texto) ou o id (número).".into()),
    }
}

fn resolve_disks(args: &Value, key: &str, disks: &[Disk]) -> Result<Vec<i64>, String> {
    let mut out = Vec::new();
    for v in list_arg(args, key)? {
        let id = resolve_disk(v, disks)?;
        if !out.contains(&id) {
            out.push(id);
        }
    }
    Ok(out)
}

fn path_components(path: &str) -> Result<Vec<&str>, String> {
    if path.chars().count() > MAX_PATH {
        return Err("O caminho é longo demais.".into());
    }
    let mut v = Vec::new();
    for c in path.split('/') {
        match c {
            "" | "." => {}
            ".." => return Err("O caminho não pode conter \"..\".".into()),
            c => v.push(c),
        }
    }
    Ok(v)
}

/// Desce pelo catálogo, pasta por pasta. Lista vazia = raiz (`None`).
fn find_by_path(cat: &Catalog, disk_id: i64, path: &str) -> Result<Option<Item>, String> {
    let mut parent: Option<i64> = None;
    let mut current: Option<Item> = None;
    let mut walked = String::new();
    for comp in path_components(path)? {
        let children = cat.list_children(disk_id, parent).map_err(|e| e.to_string())?;
        let found = children
            .iter()
            .find(|c| c.name == comp)
            .or_else(|| children.iter().find(|c| norm_eq(&c.name, comp)))
            .cloned();
        match found {
            Some(it) => {
                parent = Some(it.id);
                walked = it.path.clone();
                current = Some(it);
            }
            None => {
                let onde = if walked.is_empty() { "na raiz do disco".to_string() } else { format!("em \"{walked}\"") };
                return Err(format!("Não achei \"{comp}\" {onde}."));
            }
        }
    }
    Ok(current)
}

/// Rejects argument names not declared in the tool's schema, so a misspelled
/// field (e.g. `query` instead of `texto`) fails loudly instead of being ignored.
fn check_unknown_args(name: &str, args: &Value) -> Result<(), String> {
    let Some(obj) = args.as_object() else { return Ok(()) };
    let list = tool_list();
    let Some(props) = list
        .as_array()
        .and_then(|ts| ts.iter().find(|t| t["name"] == name))
        .and_then(|t| t["inputSchema"]["properties"].as_object())
    else {
        return Ok(());
    };
    let unknown: Vec<&str> = obj.keys().filter(|k| !props.contains_key(*k)).map(|k| k.as_str()).collect();
    if unknown.is_empty() {
        return Ok(());
    }
    let valid: Vec<&str> = props.keys().map(|k| k.as_str()).collect();
    Err(format!(
        "Campo(s) desconhecido(s): {}. Campos aceitos em {name}: {}.",
        unknown.join(", "),
        if valid.is_empty() { "nenhum".to_string() } else { valid.join(", ") }
    ))
}

// ---------------------------------------------------------------- execução

impl Server {
    pub(crate) fn call(&mut self, name: &str, args: &Value) -> R {
        check_unknown_args(name, args)?;
        match name {
            "listar_discos" => self.listar_discos(),
            "buscar_arquivos" => self.buscar_arquivos(args),
            "detalhes_item" => self.detalhes_item(args),
            "listar_pasta" => self.listar_pasta(args),
            "achar_duplicados" => self.achar_duplicados(args),
            "catalogar_disco" => self.catalogar_disco(args),
            "discos_plugados" => self.discos_plugados(),
            _ => Err(format!("Ferramenta desconhecida: {name}")),
        }
    }

    /// Discos do catálogo já com presente/ausente.
    fn disks(&mut self) -> Result<Vec<Disk>, String> {
        self.catalog(false)?; // erro claro se não há catálogo
        let vols = self.volumes(false)?;
        self.catalog(false)?.list_disks(&vols).map_err(|e| e.to_string())
    }

    fn listar_discos(&mut self) -> R {
        let disks = self.disks()?;
        let total_items: u64 = disks.iter().map(|d| d.item_count).sum();
        Ok(json!({
            "totalDiscos": disks.len(),
            "totalItens": total_items,
            "discos": disks.iter().map(disk_json).collect::<Vec<_>>(),
        }))
    }

    fn buscar_arquivos(&mut self, args: &Value) -> R {
        let mut q = SearchQuery::default();
        q.text = opt_str(args, "texto", MAX_TEXT)?.unwrap_or_default();
        for k in list_arg(args, "tipos")? {
            let s = k.as_str().ok_or("Cada item de 'tipos' precisa ser texto.")?;
            let kind = parse_kind(s)?;
            if !q.kinds.contains(&kind) {
                q.kinds.push(kind);
            }
        }
        if let Some(v) = arg(args, "tamanhoMin") {
            q.min_bytes = Some(parse_size(v)?);
        }
        if let Some(v) = arg(args, "tamanhoMax") {
            q.max_bytes = Some(parse_size(v)?);
        }
        if let (Some(a), Some(b)) = (q.min_bytes, q.max_bytes) {
            if a > b {
                return Err("'tamanhoMin' é maior que 'tamanhoMax'.".into());
            }
        }
        if let Some(s) = opt_str(args, "modificadoDepois", 40)? {
            q.modified_after = Some(parse_date(&s)?);
        }
        if let Some(s) = opt_str(args, "modificadoAntes", 40)? {
            q.modified_before = Some(parse_date(&s)?);
        }
        let limit = opt_int(args, "limite", 50, 1, 500)? as u32;
        let offset = opt_int(args, "deslocamento", 0, 0, 100_000_000)? as u32;
        q.limit = limit;
        q.offset = offset;

        let disks = self.disks()?;
        q.disk_ids = resolve_disks(args, "discos", &disks)?;
        let idx = DiskIndex::new(&disks);
        let res = self.catalog(false)?.search(&q).map_err(|e| e.to_string())?;
        let shown = res.items.len() as u64;
        let mut out = json!({
            "total": res.total,
            "retornados": shown,
            "deslocamento": offset,
            "resultados": res.items.iter().map(|i| item_json(i, &idx, false)).collect::<Vec<_>>(),
        });
        let next = offset as u64 + shown;
        if next < res.total {
            out["proximoDeslocamento"] = json!(next);
        }
        Ok(out)
    }

    fn detalhes_item(&mut self, args: &Value) -> R {
        let id = match arg(args, "id") {
            Some(v) => Some(v.as_i64().filter(|n| *n >= 1).ok_or("'id' precisa ser um número inteiro positivo.")?),
            None => None,
        };
        let disco = arg(args, "disco");
        let caminho = opt_str(args, "caminho", MAX_PATH)?;
        let disks = self.disks()?;
        let idx = DiskIndex::new(&disks);
        let item = match (id, disco, caminho) {
            (Some(id), _, _) => self.catalog(false)?.get_item(id).map_err(|e| e.to_string())?,
            (None, Some(d), Some(p)) => {
                let disk_id = resolve_disk(d, &disks)?;
                find_by_path(self.catalog(false)?, disk_id, &p)?
                    .ok_or("Informe o caminho do item (sem caminho é a raiz do disco: veja listar_pasta).")?
            }
            _ => return Err("Informe o 'id' do item, ou 'disco' e 'caminho'.".into()),
        };
        let mut v = item_json(&item, &idx, true);
        if let Some(d) = disks.iter().find(|d| d.id == item.disk_id) {
            if let Some(mp) = &d.mount_path {
                v["caminhoNoComputador"] = json!(Path::new(mp).join(&item.path).to_string_lossy());
            }
            v["discoDetalhes"] = disk_json(d);
        }
        Ok(v)
    }

    fn listar_pasta(&mut self, args: &Value) -> R {
        let disco = arg(args, "disco").ok_or("Informe o 'disco' (nome ou id). Veja listar_discos.")?;
        let caminho = opt_str(args, "caminho", MAX_PATH)?.unwrap_or_default();
        let limit = opt_int(args, "limite", 100, 1, 500)? as usize;
        let offset = opt_int(args, "deslocamento", 0, 0, 100_000_000)? as usize;
        let disks = self.disks()?;
        let disk_id = resolve_disk(disco, &disks)?;
        let idx = DiskIndex::new(&disks);
        let cat = self.catalog(false)?;
        let folder = find_by_path(cat, disk_id, &caminho)?;
        if let Some(f) = &folder {
            if f.kind != hdf_core::Kind::Folder {
                return Err(format!("\"{}\" é um arquivo, não uma pasta. Use detalhes_item.", f.path));
            }
        }
        let children = cat.list_children(disk_id, folder.as_ref().map(|f| f.id)).map_err(|e| e.to_string())?;
        let total = children.len();
        let page: Vec<Value> = children
            .iter()
            .skip(offset)
            .take(limit)
            .map(|i| {
                let mut m = serde_json::Map::new();
                m.insert("id".into(), json!(i.id));
                m.insert("nome".into(), json!(i.name));
                m.insert("tipo".into(), json!(kind_pt(i.kind)));
                size_fields(&mut m, "tamanhoBytes", "tamanho", i.size_bytes);
                m.insert("modificadoEm".into(), json!(i.modified_at));
                if i.kind == hdf_core::Kind::Folder {
                    m.insert("itensDiretos".into(), json!(i.child_count));
                }
                Value::Object(m)
            })
            .collect();
        let shown = page.len();
        let mut out = json!({
            "disco": idx.name(disk_id),
            "discoId": disk_id,
            "presente": idx.present(disk_id),
            "pasta": folder.as_ref().map(|f| f.path.clone()).unwrap_or_default(),
            "total": total,
            "retornados": page.len(),
            "deslocamento": offset,
            "itens": page,
        });
        if offset + shown < total {
            out["proximoDeslocamento"] = json!(offset + shown);
        }
        Ok(out)
    }

    fn achar_duplicados(&mut self, args: &Value) -> R {
        let min = match arg(args, "tamanhoMinimo") {
            Some(v) => parse_size(v)?,
            None => 100_000_000,
        };
        let max_groups = opt_int(args, "limiteGrupos", 20, 1, 200)? as usize;
        let disks = self.disks()?;
        let ids = resolve_disks(args, "discos", &disks)?;
        let idx = DiskIndex::new(&disks);
        let groups = self.catalog(false)?.find_duplicates(&ids, min).map_err(|e| e.to_string())?;
        const MAX_PLACES: usize = 20;
        let found = groups.len();
        let wasted_total: u64 = groups.iter().map(|g| g.size_bytes * (g.items.len() as u64 - 1)).sum();
        let list: Vec<Value> = groups
            .iter()
            .take(max_groups)
            .map(|g| {
                let copies = g.items.len() as u64;
                let mut m = serde_json::Map::new();
                m.insert("nome".into(), json!(g.name));
                size_fields(&mut m, "tamanhoBytes", "tamanho", g.size_bytes);
                m.insert("copias".into(), json!(copies));
                size_fields(&mut m, "desperdicioBytes", "desperdicio", g.size_bytes * (copies - 1));
                m.insert(
                    "locais".into(),
                    json!(
                        g.items
                            .iter()
                            .take(MAX_PLACES)
                            .map(|i| json!({
                                "id": i.id, "disco": idx.name(i.disk_id), "caminho": i.path,
                                "presente": idx.present(i.disk_id), "modificadoEm": i.modified_at
                            }))
                            .collect::<Vec<_>>()
                    ),
                );
                if g.items.len() > MAX_PLACES {
                    m.insert("locaisOmitidos".into(), json!(g.items.len() - MAX_PLACES));
                }
                Value::Object(m)
            })
            .collect();
        let mut out = json!({
            "tamanhoMinimo": human_size(min),
            "gruposEncontrados": found,
            "gruposMostrados": list.len(),
            "grupos": list,
        });
        out["espacoDesperdicadoTotal"] = json!(human_size(wasted_total));
        if found >= 1000 {
            out["aviso"] = json!("Há 1000 ou mais grupos; aumente 'tamanhoMinimo' para ver só os maiores.");
        }
        Ok(out)
    }

    fn discos_plugados(&mut self) -> R {
        let vols = self.volumes(true)?;
        Ok(json!({
            "total": vols.len(),
            "discos": vols.iter().map(|v| {
                let mut m = serde_json::Map::new();
                m.insert("nome".into(), json!(v.name));
                m.insert("caminhoMontagem".into(), json!(v.mount_path));
                m.insert("sistemaArquivos".into(), json!(v.filesystem));
                size_fields(&mut m, "capacidadeBytes", "capacidade", v.capacity_bytes);
                size_fields(&mut m, "livreBytes", "livre", v.free_bytes);
                m.insert("jaCatalogado".into(), json!(v.cataloged_disk_id.is_some()));
                m.insert("discoIdNoCatalogo".into(), json!(v.cataloged_disk_id));
                Value::Object(m)
            }).collect::<Vec<_>>(),
        }))
    }

    fn catalogar_disco(&mut self, args: &Value) -> R {
        let target = opt_str(args, "disco", 1024)?
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .ok_or("Informe o 'disco': caminho de montagem (\"/Volumes/NOME\") ou nome do disco plugado.")?;
        let hidden = match arg(args, "incluirOcultos") {
            None => false,
            Some(Value::Bool(b)) => *b,
            Some(_) => return Err("'incluirOcultos' precisa ser verdadeiro ou falso.".into()),
        };
        let vols = self.volumes(true)?;
        let refuse = || {
            let names: Vec<String> = vols.iter().map(|v| format!("\"{}\" ({})", v.name, v.mount_path)).collect();
            format!(
                "\"{target}\" não é a raiz de um disco montado. Só dá para catalogar um disco inteiro e plugado. Discos disponíveis: {}.",
                if names.is_empty() { "nenhum".to_string() } else { names.join(", ") }
            )
        };
        let vol = if target.starts_with('/') {
            let canon = std::fs::canonicalize(&target).map_err(|_| refuse())?;
            vols.iter()
                .find(|v| std::fs::canonicalize(&v.mount_path).map(|c| c == canon).unwrap_or(false))
                .cloned()
                .ok_or_else(refuse)?
        } else {
            let found: Vec<_> = vols.iter().filter(|v| norm_eq(&v.name, &target)).collect();
            match found.len() {
                1 => found[0].clone(),
                0 => return Err(refuse()),
                _ => {
                    return Err(format!(
                        "Mais de um disco plugado se chama \"{target}\": informe o caminho de montagem."
                    ));
                }
            }
        };
        let t0 = Instant::now();
        let cancel = AtomicBool::new(false);
        let opts = ReadOptions { mode: ReadMode::Full, include_hidden: hidden };
        let disk = {
            let cat = self.catalog(true)?;
            hdf_core::read_disk_as(cat, &vol, &opts, &cancel, &mut |_| {}).map_err(|e| e.to_string())?
        };
        self.vol_cache = None;
        let mut d = disk;
        d.present = true;
        d.mount_path = Some(vol.mount_path.clone());
        Ok(json!({
            "mensagem": format!("Disco \"{}\" catalogado: {} itens em {:.1} s.", d.name, d.item_count, t0.elapsed().as_secs_f64()),
            "disco": disk_json(&d),
        }))
    }
}
