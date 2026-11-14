//! Erros do motor. As mensagens são para o usuário final, em pt-BR.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Erro no catálogo: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("Erro de leitura de arquivo: {0}")]
    Io(#[from] std::io::Error),
    #[error("Leitura cancelada.")]
    Cancelled,
    #[error("{0}")]
    Msg(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn msg(s: impl Into<String>) -> Error {
        Error::Msg(s.into())
    }
}
