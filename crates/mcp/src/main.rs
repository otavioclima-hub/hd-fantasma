use hdf_mcp::server::Server;
use std::io::{BufRead, Read, Write};
use std::path::PathBuf;

fn main() {
    let mut args = std::env::args().skip(1);
    let mut catalog: Option<PathBuf> = None;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--catalog" => match args.next() {
                Some(p) => catalog = Some(PathBuf::from(p)),
                None => {
                    eprintln!("--catalog precisa de um caminho.");
                    std::process::exit(2);
                }
            },
            "--help" | "-h" => {
                println!("hd-fantasma-mcp [--catalog <arquivo.sqlite>]\nServidor MCP do HD Fantasma (stdio). Sem --catalog usa o catálogo do app (Mac: Documentos/HD Fantasma/catalogo.sqlite; Windows: %LOCALAPPDATA%\\HD Fantasma\\catalogo.sqlite).");
                return;
            }
            "--version" | "-V" => {
                println!("hd-fantasma-mcp {}", env!("CARGO_PKG_VERSION"));
                return;
            }
            other => {
                eprintln!("Opção desconhecida: {other}");
                std::process::exit(2);
            }
        }
    }
    let path = match catalog {
        Some(p) => p,
        None => match hdf_core::db::default_catalog_path() {
            Ok(p) => p,
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        },
    };
    let mut server = Server::new(path);
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    // One message per line; a line longer than this is refused instead of filling memory.
    const MAX_LINE: u64 = 8 * 1024 * 1024;
    let mut input = stdin.lock();
    loop {
        let mut buf = Vec::new();
        let n = match (&mut input).take(MAX_LINE + 1).read_until(b'\n', &mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        if n as u64 > MAX_LINE && buf.last() != Some(&b'\n') {
            // Skip the rest of the oversized line, then answer with a parse error.
            let mut sink = Vec::new();
            if input.read_until(b'\n', &mut sink).is_err() {
                break;
            }
            let reply = r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32600,"message":"Mensagem grande demais."}}"#;
            if writeln!(out, "{reply}").is_err() || out.flush().is_err() {
                break;
            }
            continue;
        }
        let line = String::from_utf8_lossy(&buf);
        let line = line.trim_end_matches(['\n', '\r']);
        if line.trim().is_empty() {
            continue;
        }
        if let Some(reply) = server.handle_message(&line) {
            if writeln!(out, "{reply}").is_err() || out.flush().is_err() {
                break;
            }
        }
    }
}
