use hdf_mcp::server::Server;
use std::io::{BufRead, Write};
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
                println!("hd-fantasma-mcp [--catalog <arquivo.sqlite>]\nServidor MCP do HD Fantasma (stdio). Sem --catalog usa Documentos/HD Fantasma/catalogo.sqlite.");
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
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
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
