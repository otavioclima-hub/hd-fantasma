//! Testes de ponta a ponta: sobe o executável com um catálogo temporário e conversa por stdio.

use hdf_core::*;
use serde_json::{Value, json};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::AtomicBool;

fn write(root: &Path, rel: &str, bytes: usize) {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, vec![b'x'; bytes]).unwrap();
}

fn scan(cat: &mut Catalog, root: &Path, uuid: &str, name: &str) {
    let v = MountedVolume {
        uuid: uuid.into(),
        name: name.into(),
        mount_path: root.to_string_lossy().into_owned(),
        filesystem: "apfs".into(),
        capacity_bytes: 1_000_000,
        free_bytes: 400_000,
        cataloged_disk_id: None,
    };
    let opts = ReadOptions { mode: ReadMode::Full, include_hidden: false };
    read_disk_as(cat, &v, &opts, &AtomicBool::new(false), &mut |_| {}).unwrap();
}

struct Fixture {
    _dirs: Vec<tempfile::TempDir>,
    catalog: std::path::PathBuf,
}

/// Dois discos: "ARQUIVO" e "BACKUP", com um arquivo grande repetido nos dois.
fn fixture() -> Fixture {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let store = tempfile::tempdir().unwrap();
    write(a.path(), "Projetos/2025/CLIENTE_final.mov", 3000);
    write(a.path(), "Projetos/2025/notas.txt", 10);
    write(a.path(), "Projetos/Vídeo Institucional.mp4", 500);
    write(b.path(), "Copia/CLIENTE_final.mov", 3000);
    write(b.path(), "Copia/musica.wav", 800);
    let catalog = store.path().join("catalogo.sqlite");
    let mut cat = Catalog::open(&catalog).unwrap();
    scan(&mut cat, a.path(), "uuid-a", "ARQUIVO");
    scan(&mut cat, b.path(), "uuid-b", "BACKUP");
    drop(cat);
    Fixture { _dirs: vec![a, b, store], catalog }
}

struct Client {
    child: Child,
    stdin: ChildStdin,
    out: BufReader<ChildStdout>,
    next_id: u64,
}

impl Client {
    fn start(catalog: &Path) -> Client {
        let mut child = Command::new(env!("CARGO_BIN_EXE_hd-fantasma-mcp"))
            .arg("--catalog")
            .arg(catalog)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let out = BufReader::new(child.stdout.take().unwrap());
        let mut c = Client { child, stdin, out, next_id: 1 };
        let r = c.request(
            "initialize",
            json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "teste", "version": "0"}}),
        );
        assert_eq!(r["result"]["serverInfo"]["name"], "hd-fantasma");
        assert_eq!(r["result"]["protocolVersion"], "2025-06-18");
        c.send_raw(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}).to_string());
        c
    }

    fn send_raw(&mut self, line: &str) {
        writeln!(self.stdin, "{line}").unwrap();
        self.stdin.flush().unwrap();
    }

    fn read(&mut self) -> Value {
        let mut l = String::new();
        self.out.read_line(&mut l).unwrap();
        serde_json::from_str(&l).unwrap_or_else(|_| panic!("resposta inválida: {l:?}"))
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send_raw(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string());
        let r = self.read();
        assert_eq!(r["id"], id);
        r
    }

    /// Chama uma ferramenta; devolve (isError, texto).
    fn tool(&mut self, name: &str, args: Value) -> (bool, String) {
        let r = self.request("tools/call", json!({"name": name, "arguments": args}));
        let res = &r["result"];
        (res["isError"].as_bool().unwrap(), res["content"][0]["text"].as_str().unwrap().to_string())
    }

    fn ok(&mut self, name: &str, args: Value) -> Value {
        let (err, text) = self.tool(name, args);
        assert!(!err, "{name} falhou: {text}");
        serde_json::from_str(&text).unwrap()
    }

    fn fail(&mut self, name: &str, args: Value) -> String {
        let (err, text) = self.tool(name, args);
        assert!(err, "{name} devia falhar, mas deu: {text}");
        text
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn protocol_basics() {
    let f = fixture();
    let mut c = Client::start(&f.catalog);
    assert_eq!(c.request("ping", json!({}))["result"], json!({}));
    let list = c.request("tools/list", json!({}));
    let names: Vec<&str> = list["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(
        names,
        ["listar_discos", "buscar_arquivos", "detalhes_item", "listar_pasta", "achar_duplicados", "catalogar_disco", "discos_plugados"]
    );
    for t in list["result"]["tools"].as_array().unwrap() {
        assert!(t["description"].as_str().unwrap().len() > 40);
        assert_eq!(t["inputSchema"]["type"], "object");
    }
    let r = c.request("nao/existe", json!({}));
    assert_eq!(r["error"]["code"], -32601);
    let r = c.request("tools/call", json!({"name": "inexistente", "arguments": {}}));
    assert_eq!(r["error"]["code"], -32602);
    c.send_raw("isso nao e json");
    let r = c.read();
    assert_eq!(r["error"]["code"], -32700);
    // continua vivo depois do lixo
    assert_eq!(c.request("ping", json!({}))["result"], json!({}));
}

#[test]
fn listar_discos() {
    let f = fixture();
    let mut c = Client::start(&f.catalog);
    let r = c.ok("listar_discos", json!({}));
    assert_eq!(r["totalDiscos"], 2);
    let d = &r["discos"][0];
    assert_eq!(d["nome"], "ARQUIVO");
    assert_eq!(d["presente"], false);
    assert_eq!(d["capacidadeBytes"], 1_000_000);
    assert_eq!(d["usadoBytes"], 600_000);
    assert_eq!(d["capacidade"], "1.0 MB");
    assert!(d["itens"].as_u64().unwrap() >= 5);
    assert!(d["ultimaLeitura"].as_str().unwrap().starts_with("20"));
}

#[test]
fn buscar_arquivos_com_filtros() {
    let f = fixture();
    let mut c = Client::start(&f.catalog);
    let r = c.ok("buscar_arquivos", json!({"texto": "cliente"}));
    assert_eq!(r["total"], 2);
    let first = &r["resultados"][0];
    assert_eq!(first["nome"], "CLIENTE_final.mov");
    assert_eq!(first["tipo"], "video");
    assert_eq!(first["tamanhoBytes"], 3000);
    assert_eq!(first["tamanho"], "3.0 KB");
    assert_eq!(first["presente"], false);
    assert!(first["caminho"].as_str().unwrap().ends_with("CLIENTE_final.mov"));

    // sem acento, por tipo e por disco (nome)
    let r = c.ok("buscar_arquivos", json!({"texto": "video institucional", "discos": ["arquivo"]}));
    assert_eq!(r["total"], 1);
    let r = c.ok("buscar_arquivos", json!({"tipos": ["audio"]}));
    assert_eq!(r["resultados"][0]["nome"], "musica.wav");
    assert_eq!(r["resultados"][0]["disco"], "BACKUP");

    // tamanho com unidade
    let r = c.ok("buscar_arquivos", json!({"tamanhoMin": "1KB", "tipos": ["video"]}));
    assert_eq!(r["total"], 2);
    let r = c.ok("buscar_arquivos", json!({"tamanhoMin": "1KB", "tamanhoMax": 5000, "discos": [2], "tipos": ["video"]}));
    assert_eq!(r["total"], 1);

    // datas (os arquivos foram criados agora)
    let r = c.ok("buscar_arquivos", json!({"modificadoDepois": "2020-01-01"}));
    assert!(r["total"].as_u64().unwrap() >= 5);
    let r = c.ok("buscar_arquivos", json!({"modificadoAntes": "2020-01-01"}));
    assert_eq!(r["total"], 0);

    // paginação
    let r = c.ok("buscar_arquivos", json!({"tipos": ["video", "audio", "documento"], "limite": 2}));
    assert_eq!(r["retornados"], 2);
    assert_eq!(r["proximoDeslocamento"], 2);
    let r2 = c.ok("buscar_arquivos", json!({"tipos": ["video", "audio", "documento"], "limite": 2, "deslocamento": 2}));
    assert_ne!(r["resultados"][0]["id"], r2["resultados"][0]["id"]);
}

#[test]
fn buscar_arquivos_valida_entradas() {
    let f = fixture();
    let mut c = Client::start(&f.catalog);
    assert!(c.fail("buscar_arquivos", json!({"limite": 501})).contains("limite"));
    assert!(c.fail("buscar_arquivos", json!({"limite": 0})).contains("limite"));
    assert!(c.fail("buscar_arquivos", json!({"limite": "muitos"})).contains("limite"));
    assert!(c.fail("buscar_arquivos", json!({"tipos": ["xyz"]})).contains("Tipo inválido"));
    assert!(c.fail("buscar_arquivos", json!({"tamanhoMin": "grande"})).contains("Tamanho inválido"));
    assert!(c.fail("buscar_arquivos", json!({"tamanhoMin": "2GB", "tamanhoMax": "1GB"})).contains("maior"));
    assert!(c.fail("buscar_arquivos", json!({"modificadoDepois": "ontem"})).contains("Data inválida"));
    assert!(c.fail("buscar_arquivos", json!({"discos": ["Não Existe"]})).contains("Não achei o disco"));
    assert!(c.fail("buscar_arquivos", json!({"discos": [999]})).contains("Não existe disco"));
    assert!(c.fail("buscar_arquivos", json!({"texto": "a".repeat(500)})).contains("longo"));
    assert!(c.fail("buscar_arquivos", json!({"texto": 5})).contains("texto"));
    // texto com aspas e curingas não quebra
    let r = c.ok("buscar_arquivos", json!({"texto": "\"%_'; drop"}));
    assert_eq!(r["total"], 0);
}

#[test]
fn detalhes_e_navegacao() {
    let f = fixture();
    let mut c = Client::start(&f.catalog);
    let r = c.ok("listar_pasta", json!({"disco": "ARQUIVO"}));
    assert_eq!(r["pasta"], "");
    assert_eq!(r["itens"][0]["nome"], "Projetos");
    assert_eq!(r["itens"][0]["tipo"], "pasta");
    let r = c.ok("listar_pasta", json!({"disco": "ARQUIVO", "caminho": "Projetos"}));
    let nomes: Vec<&str> = r["itens"].as_array().unwrap().iter().map(|i| i["nome"].as_str().unwrap()).collect();
    assert_eq!(nomes, ["2025", "Vídeo Institucional.mp4"]); // pastas primeiro
    assert_eq!(r["total"], 2);
    let r = c.ok("listar_pasta", json!({"disco": 1, "caminho": "/Projetos/2025/", "limite": 1}));
    assert_eq!(r["retornados"], 1);
    assert_eq!(r["proximoDeslocamento"], 1);

    let id = c.ok("listar_pasta", json!({"disco": "ARQUIVO"}))["itens"][0]["id"].as_i64().unwrap();
    let d = c.ok("detalhes_item", json!({"id": id}));
    assert_eq!(d["nome"], "Projetos");
    assert_eq!(d["itensNoTotal"], 4); // 2025, 2 arquivos dentro dela e o Vídeo Institucional
    let d = c.ok("detalhes_item", json!({"disco": "ARQUIVO", "caminho": "Projetos/2025/notas.txt"}));
    assert_eq!(d["tipo"], "documento");
    assert_eq!(d["tamanhoBytes"], 10);
    assert!(d["criadoEm"].is_string());
    assert_eq!(d["discoDetalhes"]["nome"], "ARQUIVO");

    assert!(c.fail("detalhes_item", json!({"id": 99999})).contains("não encontrado"));
    assert!(c.fail("detalhes_item", json!({})).contains("Informe"));
    assert!(c.fail("detalhes_item", json!({"id": -3})).contains("positivo"));
    assert!(c.fail("detalhes_item", json!({"disco": "ARQUIVO", "caminho": "Projetos/../x"})).contains(".."));
    assert!(c.fail("detalhes_item", json!({"disco": "ARQUIVO", "caminho": "Nada/aqui"})).contains("Não achei"));
    assert!(c.fail("listar_pasta", json!({})).contains("disco"));
    assert!(c.fail("listar_pasta", json!({"disco": "ARQUIVO", "caminho": "Projetos/2025/notas.txt"})).contains("não uma pasta"));
}

#[test]
fn duplicados() {
    let f = fixture();
    let mut c = Client::start(&f.catalog);
    // padrão 100 MB: nada
    let r = c.ok("achar_duplicados", json!({}));
    assert_eq!(r["gruposEncontrados"], 0);
    let r = c.ok("achar_duplicados", json!({"tamanhoMinimo": "1KB"}));
    assert_eq!(r["gruposEncontrados"], 1);
    let g = &r["grupos"][0];
    assert_eq!(g["nome"], "CLIENTE_final.mov");
    assert_eq!(g["copias"], 2);
    assert_eq!(g["desperdicioBytes"], 3000);
    assert_eq!(g["locais"].as_array().unwrap().len(), 2);
    // restrito a um disco: sem duplicado
    let r = c.ok("achar_duplicados", json!({"tamanhoMinimo": 1, "discos": ["ARQUIVO"]}));
    assert_eq!(r["gruposEncontrados"], 0);
    assert!(c.fail("achar_duplicados", json!({"limiteGrupos": 1000})).contains("limiteGrupos"));
}

#[test]
fn catalogar_e_plugados() {
    let f = fixture();
    let mut c = Client::start(&f.catalog);
    let r = c.ok("discos_plugados", json!({}));
    assert!(r["discos"].is_array());
    if cfg!(target_os = "macos") {
        assert!(r["total"].as_u64().unwrap() >= 1);
        assert!(r["discos"][0]["jaCatalogado"].is_boolean());
    }
    // recusa o que não é raiz de disco montado
    let tmp = tempfile::tempdir().unwrap();
    let t = c.fail("catalogar_disco", json!({"disco": tmp.path().to_string_lossy()}));
    assert!(t.contains("não é a raiz de um disco montado"), "{t}");
    assert!(c.fail("catalogar_disco", json!({"disco": "/Users"})).contains("não é a raiz"));
    assert!(c.fail("catalogar_disco", json!({"disco": "Disco Que Nao Existe"})).contains("não é a raiz"));
    assert!(c.fail("catalogar_disco", json!({})).contains("Informe"));
    assert!(c.fail("catalogar_disco", json!({"disco": "x", "incluirOcultos": "sim"})).contains("verdadeiro"));
    // o disco interno do computador nunca é lido pelo MCP (auditoria 2026-11-14)
    if cfg!(target_os = "macos") {
        assert!(c.fail("catalogar_disco", json!({"disco": "/"})).contains("disco interno"));
        let t = c.fail("catalogar_disco", json!({"disco": "/System/Volumes/Data"}));
        assert!(t.contains("disco interno") || t.contains("não é a raiz"), "{t}");
        let internal = r["discos"].as_array().unwrap().iter().find(|d| d["caminhoMontagem"] == "/").unwrap();
        assert_eq!(internal["podeCatalogar"], false);
    }
    // nada foi gravado
    assert_eq!(c.ok("listar_discos", json!({}))["totalDiscos"], 2);
}

#[test]
fn sem_catalogo() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nao-existe.sqlite");
    let mut c = Client::start(&path);
    for (tool, args) in [
        ("listar_discos", json!({})),
        ("buscar_arquivos", json!({"texto": "a"})),
        ("listar_pasta", json!({"disco": "x"})),
        ("achar_duplicados", json!({})),
    ] {
        let t = c.fail(tool, args);
        assert!(t.contains("Nenhum catálogo ainda"), "{tool}: {t}");
    }
    // discos_plugados funciona e não cria o arquivo do catálogo
    c.ok("discos_plugados", json!({}));
    assert!(!path.exists());
}
