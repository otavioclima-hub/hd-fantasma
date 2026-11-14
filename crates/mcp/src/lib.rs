//! Servidor MCP do HD Fantasma (ADR 0005).
//!
//! Fala o protocolo MCP (JSON-RPC 2.0, uma mensagem por linha) só por stdin/stdout,
//! escrito à mão com `serde_json`: não puxa nenhuma biblioteca de rede (ADR 0015).

pub mod server;
pub mod tools;
pub mod util;
