use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Erro {
    pub codigo: String,
    pub mensagem: String,
    /// Campo do input a que o erro se refere (nome camelCase, ex.: "pagoEm"), quando o código
    /// sozinho não chega para o saber (ex.: `data_futura` numa multa com várias datas).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub campo: Option<String>,
}

impl Erro {
    /// Indica o campo do input a que o erro se refere.
    pub fn com_campo(mut self, campo: &str) -> Self {
        self.campo = Some(campo.to_string());
        self
    }
}

/// Marca os erros de data (`data_*`) de uma validação com o campo a que se referem.
/// Outros erros (`referencia_invalida`, `bd`, …) ficam como estão.
pub trait DataNoCampo<T> {
    fn data_no_campo(self, campo: &str) -> Res<T>;
}

impl<T> DataNoCampo<T> for Res<T> {
    fn data_no_campo(self, campo: &str) -> Res<T> {
        self.map_err(|e| {
            if e.codigo.starts_with("data_") {
                e.com_campo(campo)
            } else {
                e
            }
        })
    }
}

pub type Res<T> = Result<T, Erro>;

impl From<rusqlite::Error> for Erro {
    fn from(e: rusqlite::Error) -> Self {
        Erro {
            campo: None,
            codigo: "bd".into(),
            mensagem: e.to_string(),
        }
    }
}

impl From<std::io::Error> for Erro {
    fn from(e: std::io::Error) -> Self {
        Erro {
            campo: None,
            codigo: "io".into(),
            mensagem: e.to_string(),
        }
    }
}
