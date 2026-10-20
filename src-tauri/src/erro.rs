use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Erro {
    pub codigo: String,
    pub mensagem: String,
}

pub type Res<T> = Result<T, Erro>;

impl From<rusqlite::Error> for Erro {
    fn from(e: rusqlite::Error) -> Self {
        Erro { codigo: "bd".into(), mensagem: e.to_string() }
    }
}

impl From<std::io::Error> for Erro {
    fn from(e: std::io::Error) -> Self {
        Erro { codigo: "io".into(), mensagem: e.to_string() }
    }
}
