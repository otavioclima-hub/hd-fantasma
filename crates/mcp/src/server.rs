//! Protocolo MCP sobre JSON-RPC 2.0 (mensagens de uma linha) + estado do servidor.

use crate::tools;
use hdf_core::{Catalog, MountedVolume};
use serde_json::{Value, json};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::time::{Duration, Instant};

const SUPPORTED: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];
pub const NO_CATALOG: &str = "Nenhum catálogo ainda: abra o HD Fantasma e leia um disco.";
const VOLUMES_TTL: Duration = Duration::from_secs(5);

pub struct Server {
    pub(crate) catalog_path: PathBuf,
    pub(crate) catalog: Option<Catalog>,
    pub(crate) vol_cache: Option<(Instant, Vec<MountedVolume>)>,
}

impl Server {
    pub fn new(catalog_path: PathBuf) -> Server {
        Server { catalog_path, catalog: None, vol_cache: None }
    }

    /// Catálogo aberto. `create = false`: se o arquivo não existe, devolve a mensagem "nenhum catálogo".
    pub(crate) fn catalog(&mut self, create: bool) -> Result<&mut Catalog, String> {
        if self.catalog.is_none() {
            if !create && !self.catalog_path.exists() {
                return Err(NO_CATALOG.into());
            }
            self.catalog = Some(Catalog::open(&self.catalog_path).map_err(|e| e.to_string())?);
        }
        Ok(self.catalog.as_mut().expect("catalogo aberto acima"))
    }

    /// Discos montados agora (com cache curto: o macOS gasta ~50 ms por disco para identificá-los).
    pub(crate) fn volumes(&mut self, fresh: bool) -> Result<Vec<MountedVolume>, String> {
        if !fresh {
            if let Some((t, v)) = &self.vol_cache {
                if t.elapsed() < VOLUMES_TTL {
                    return Ok(v.clone());
                }
            }
        }
        let vols = if self.catalog_path.exists() || self.catalog.is_some() {
            let cat = self.catalog(false)?;
            hdf_core::volumes::list_mounted_volumes(cat).map_err(|e| e.to_string())?
        } else {
            volumes_without_catalog()?
        };
        self.vol_cache = Some((Instant::now(), vols.clone()));
        Ok(vols)
    }

    /// Processa uma linha. `None` = nada a responder (notificação).
    pub fn handle_message(&mut self, line: &str) -> Option<String> {
        let msg: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => return Some(error_reply(Value::Null, -32700, "JSON inválido.")),
        };
        if !msg.is_object() {
            return Some(error_reply(Value::Null, -32600, "Requisição inválida."));
        }
        let id = msg.get("id").cloned();
        let method = msg.get("method").and_then(|m| m.as_str());
        let (Some(id), Some(method)) = (id, method) else {
            return None; // notificação (initialized, cancelled...) ou resposta do cliente
        };
        let params = msg.get("params").cloned().unwrap_or(Value::Null);
        let reply = match method {
            "initialize" => {
                let want = params.get("protocolVersion").and_then(|v| v.as_str()).unwrap_or("");
                let version = SUPPORTED.iter().find(|v| **v == want).unwrap_or(&SUPPORTED[0]);
                ok_reply(
                    id,
                    json!({
                        "protocolVersion": version,
                        "capabilities": { "tools": { "listChanged": false } },
                        "serverInfo": { "name": "hd-fantasma", "title": "HD Fantasma", "version": env!("CARGO_PKG_VERSION") },
                        "instructions": "Consulta o catálogo de discos do HD Fantasma (HDs, SSDs, pen drives já lidos). Funciona mesmo com o disco desplugado: use buscar_arquivos para achar arquivos e listar_discos para ver quais discos existem."
                    }),
                )
            }
            "ping" => ok_reply(id, json!({})),
            "tools/list" => ok_reply(id, json!({ "tools": tools::tool_list() })),
            "tools/call" => {
                let name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
                let args = params.get("arguments").cloned().unwrap_or(Value::Null);
                if !args.is_object() && !args.is_null() {
                    error_reply(id, -32602, "'arguments' precisa ser um objeto.")
                } else if !tools::exists(name) {
                    error_reply(id, -32602, &format!("Ferramenta desconhecida: {name}"))
                } else {
                    let r = catch_unwind(AssertUnwindSafe(|| self.call(name, &args)))
                        .unwrap_or_else(|_| Err("Erro interno inesperado.".into()));
                    ok_reply(id, tool_result(r))
                }
            }
            "resources/list" => ok_reply(id, json!({ "resources": [] })),
            "prompts/list" => ok_reply(id, json!({ "prompts": [] })),
            _ => error_reply(id, -32601, &format!("Método não suportado: {method}")),
        };
        Some(reply)
    }
}

fn tool_result(r: Result<Value, String>) -> Value {
    match r {
        Ok(v) => json!({ "content": [{ "type": "text", "text": v.to_string() }], "isError": false }),
        Err(m) => json!({ "content": [{ "type": "text", "text": m }], "isError": true }),
    }
}

fn ok_reply(id: Value, result: Value) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string()
}

fn error_reply(id: Value, code: i32, message: &str) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }).to_string()
}

/// Sem catálogo ainda: lista os volumes usando um catálogo descartável (não cria nada no lugar real).
fn volumes_without_catalog() -> Result<Vec<MountedVolume>, String> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("hdf-mcp-{}-{nanos}", std::process::id()));
    let r = (|| {
        let cat = Catalog::open(dir.join("vazio.sqlite")).map_err(|e| e.to_string())?;
        hdf_core::volumes::list_mounted_volumes(&cat).map_err(|e| e.to_string())
    })();
    let _ = std::fs::remove_dir_all(&dir);
    r
}
