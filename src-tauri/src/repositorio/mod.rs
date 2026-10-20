//! Acesso aos dados (SQLite): CRUD, validações e consultas.

pub mod carros;
pub mod config;
pub mod consultas;
pub mod historico;
pub mod lancamentos;
pub mod motoristas;
pub mod multas;
pub mod planos;

#[cfg(test)]
mod testes;

use crate::erro::{Erro, Res};
use crate::regras::km::{Leitura, km_abaixo_do_anterior};
use chrono::NaiveDate;
use rusqlite::types::Type;
use rusqlite::{Connection, OptionalExtension, Row, params};

pub(crate) fn erro(codigo: &str, mensagem: impl Into<String>) -> Erro {
    Erro {
        codigo: codigo.into(),
        mensagem: mensagem.into(),
    }
}

pub(crate) fn nao_encontrado() -> Erro {
    erro("nao_encontrado", "Registo não encontrado.")
}

/// Converte "AAAA-MM-DD" numa data; inválida -> `data_invalida`.
pub(crate) fn data(s: &str) -> Res<NaiveDate> {
    NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d")
        .map_err(|_| erro("data_invalida", format!("Data inválida: {}.", s.trim())))
}

/// Data opcional; string vazia conta como ausente.
pub(crate) fn data_opt(s: &Option<String>) -> Res<Option<NaiveDate>> {
    match texto(s) {
        Some(t) => data(&t).map(Some),
        None => Ok(None),
    }
}

pub(crate) fn fmt_data(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

/// Texto opcional aparado; vazio -> `None`.
pub(crate) fn texto(s: &Option<String>) -> Option<String> {
    s.as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
}

/// 70000 -> "70 000".
pub(crate) fn milhares(n: i64) -> String {
    let s = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(' ');
        }
        out.push(c);
    }
    if n < 0 { format!("-{out}") } else { out }
}

/// Lê uma coluna de data ("AAAA-MM-DD", ou datetime do SQLite: usa os 10 primeiros caracteres).
pub(crate) fn col_data(r: &Row, i: usize) -> rusqlite::Result<NaiveDate> {
    let s: String = r.get(i)?;
    let base = s.get(..10).unwrap_or(&s);
    NaiveDate::parse_from_str(base, "%Y-%m-%d")
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(i, Type::Text, Box::new(e)))
}

pub(crate) fn col_data_opt(r: &Row, i: usize) -> rusqlite::Result<Option<NaiveDate>> {
    let s: Option<String> = r.get(i)?;
    match s {
        None => Ok(None),
        Some(_) => col_data(r, i).map(Some),
    }
}

pub(crate) fn referencia_invalida() -> Erro {
    erro("referencia_invalida", "O registo escolhido já não existe.")
}

/// Tabelas que podem ser referenciadas por um input.
#[derive(Clone, Copy)]
pub(crate) enum Tabela {
    Carros,
    Motoristas,
    TiposManutencao,
}

/// Garante que o registo referenciado existe (senão `referencia_invalida`).
pub(crate) fn existe(conn: &Connection, tabela: Tabela, id: i64) -> Res<()> {
    let sql = match tabela {
        Tabela::Carros => "SELECT 1 FROM carros WHERE id = ?1",
        Tabela::Motoristas => "SELECT 1 FROM motoristas WHERE id = ?1",
        Tabela::TiposManutencao => "SELECT 1 FROM tipos_manutencao WHERE id = ?1",
    };
    let v: Option<i64> = conn.query_row(sql, [id], |r| r.get(0)).optional()?;
    v.map(|_| ()).ok_or_else(referencia_invalida)
}

/// Data > hoje -> `data_futura`.
pub(crate) fn nao_futura(d: NaiveDate, hoje: NaiveDate) -> Res<()> {
    if d > hoje {
        return Err(erro("data_futura", "A data não pode ser no futuro."));
    }
    Ok(())
}

/// Valor de uma lista fechada (aparado) ou erro `codigo`.
pub(crate) fn opcao(valor: &str, validos: &[&str], codigo: &str, msg: &str) -> Res<String> {
    let v = valor.trim();
    if validos.contains(&v) {
        Ok(v.to_string())
    } else {
        Err(erro(codigo, msg))
    }
}

pub(crate) fn validar_valor(v: i64) -> Res<()> {
    if v < 0 {
        return Err(erro("valor_invalido", "O valor não pode ser negativo."));
    }
    Ok(())
}

/// Data de um acontecimento: não pode ser futura nem anterior à aquisição do carro.
pub(crate) fn validar_data_carro(
    conn: &Connection,
    carro_id: i64,
    d: NaiveDate,
    hoje: NaiveDate,
) -> Res<()> {
    let aquisicao: Option<Option<String>> = conn
        .query_row(
            "SELECT data_aquisicao FROM carros WHERE id = ?1",
            [carro_id],
            |r| r.get(0),
        )
        .optional()?;
    let Some(aquisicao) = aquisicao else {
        return Err(referencia_invalida());
    };
    nao_futura(d, hoje)?;
    if let Some(a) = aquisicao.as_deref().map(data).transpose()?
        && d < a
    {
        return Err(erro(
            "data_antes_aquisicao",
            "A data é anterior à data de aquisição do carro.",
        ));
    }
    Ok(())
}

pub(crate) fn leituras(conn: &Connection, carro_id: i64) -> Res<Vec<Leitura>> {
    let mut st = conn.prepare("SELECT data, km FROM leituras_km WHERE carro_id = ?1")?;
    let v = st
        .query_map([carro_id], |r| {
            Ok(Leitura {
                data: col_data(r, 0)?,
                km: r.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

/// KM negativo -> `km_invalido`; abaixo da leitura anterior sem confirmação -> `km_abaixo`.
pub(crate) fn validar_km(
    conn: &Connection,
    carro_id: i64,
    d: NaiveDate,
    km: i64,
    confirmado: bool,
) -> Res<()> {
    if km < 0 {
        return Err(erro("km_invalido", "Os KM não podem ser negativos."));
    }
    if confirmado {
        return Ok(());
    }
    if let Some(anterior) = km_abaixo_do_anterior(&leituras(conn, carro_id)?, d, km) {
        return Err(erro(
            "km_abaixo",
            format!(
                "O último KM registado era {}. Confirma {}?",
                milhares(anterior),
                milhares(km)
            ),
        ));
    }
    Ok(())
}

pub(crate) fn inserir_leitura(
    conn: &Connection,
    carro_id: i64,
    d: NaiveDate,
    km: i64,
    origem: &str,
    origem_id: Option<i64>,
) -> Res<i64> {
    conn.execute(
        "INSERT INTO leituras_km (carro_id, data, km, origem, origem_id) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![carro_id, fmt_data(d), km, origem, origem_id],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Conta linhas devolvidas por `SELECT COUNT(*) ...` com um parâmetro.
pub(crate) fn contar(conn: &Connection, sql: &str, id: i64) -> Res<i64> {
    Ok(conn.query_row(sql, [id], |r| r.get(0))?)
}
