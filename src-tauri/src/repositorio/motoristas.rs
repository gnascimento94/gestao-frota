use super::{
    Tabela, col_data, col_data_opt, contar, data_opt, erro, existe, fmt_data, nao_encontrado, texto,
};
use crate::erro::Res;
use crate::regras::alertas::{Nivel, nivel_carta};
use crate::regras::gastos;
use chrono::NaiveDate;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Motorista {
    pub id: i64,
    pub nome: String,
    pub telefone: Option<String>,
    pub carta_numero: Option<String>,
    pub carta_validade: Option<String>,
    pub ativo: bool,
    pub notas: Option<String>,
    /// Alerta da carta (mesma regra do painel); sempre `None` para motoristas inativos.
    pub alerta_carta: Option<Nivel>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MotoristaInput {
    pub id: Option<i64>,
    pub nome: String,
    pub telefone: Option<String>,
    pub carta_numero: Option<String>,
    pub carta_validade: Option<String>,
    pub ativo: bool,
    pub notas: Option<String>,
}

/// Atribuição tal como é mostrada no ecrã (com ids e nomes).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Atribuicao {
    pub id: i64,
    pub carro_id: i64,
    pub matricula: String,
    pub motorista_id: i64,
    pub motorista_nome: String,
    pub de: String,
    /// `None` = atribuição atual.
    pub ate: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FiltroAtribuicoes {
    pub carro_id: Option<i64>,
    pub motorista_id: Option<i64>,
}

pub fn listar_motoristas(
    conn: &Connection,
    so_ativos: bool,
    hoje: NaiveDate,
) -> Res<Vec<Motorista>> {
    let mut st = conn.prepare(
        "SELECT id, nome, telefone, carta_numero, carta_validade, ativo, notas FROM motoristas
         WHERE (?1 = 0 OR ativo = 1) ORDER BY nome COLLATE NOCASE, id",
    )?;
    let v = st
        .query_map([so_ativos], |r| {
            let ativo: bool = r.get(5)?;
            let validade = col_data_opt(r, 4)?;
            Ok(Motorista {
                id: r.get(0)?,
                nome: r.get(1)?,
                telefone: r.get(2)?,
                carta_numero: r.get(3)?,
                carta_validade: r.get(4)?,
                ativo,
                notas: r.get(6)?,
                alerta_carta: validade
                    .filter(|_| ativo)
                    .and_then(|v| nivel_carta(v, hoje)),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

pub fn guardar_motorista(conn: &Connection, m: &MotoristaInput) -> Res<i64> {
    let nome = m.nome.trim();
    if nome.is_empty() {
        return Err(erro("falta_nome", "Indique o nome."));
    }
    let validade = data_opt(&m.carta_validade)?.map(fmt_data);
    let (tel, carta, notas) = (texto(&m.telefone), texto(&m.carta_numero), texto(&m.notas));
    match m.id {
        None => {
            conn.execute(
                "INSERT INTO motoristas (nome, telefone, carta_numero, carta_validade, ativo, notas)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![nome, tel, carta, validade, m.ativo, notas],
            )?;
            Ok(conn.last_insert_rowid())
        }
        Some(id) => {
            let n = conn.execute(
                "UPDATE motoristas SET nome = ?1, telefone = ?2, carta_numero = ?3,
                    carta_validade = ?4, ativo = ?5, notas = ?6, atualizado_em = datetime('now')
                 WHERE id = ?7",
                params![nome, tel, carta, validade, m.ativo, notas, id],
            )?;
            if n == 0 {
                return Err(nao_encontrado());
            }
            Ok(id)
        }
    }
}

pub fn apagar_motorista(conn: &Connection, id: i64) -> Res<()> {
    let n = contar(
        conn,
        "SELECT COUNT(*) FROM atribuicoes WHERE motorista_id = ?1",
        id,
    )? + contar(
        conn,
        "SELECT COUNT(*) FROM multas WHERE motorista_id = ?1",
        id,
    )?;
    if n > 0 {
        return Err(erro(
            "tem_historico",
            "Este motorista tem histórico. Marque-o como inativo.",
        ));
    }
    if conn.execute("DELETE FROM motoristas WHERE id = ?1", [id])? == 0 {
        return Err(nao_encontrado());
    }
    Ok(())
}

fn data_invalida(msg: &str) -> crate::erro::Erro {
    erro("data_invalida", msg)
}

/// Fecha a atribuição aberta do carro (`ate = data`) e, se `Some`, abre uma nova (`de = data`).
/// O motorista deixa a atribuição aberta que tenha noutro carro. Sem sobreposições por carro.
pub fn atribuir(
    conn: &mut Connection,
    carro_id: i64,
    motorista_id: Option<i64>,
    data: NaiveDate,
) -> Res<()> {
    let tx = conn.transaction()?;
    existe(&tx, Tabela::Carros, carro_id)?;
    if let Some(m) = motorista_id {
        existe(&tx, Tabela::Motoristas, m)?;
    }
    let dia = fmt_data(data);
    let aberta: Option<(i64, i64, String)> = tx
        .query_row(
            "SELECT id, motorista_id, de FROM atribuicoes WHERE carro_id = ?1 AND ate IS NULL",
            [carro_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    if let Some((_, mot, de)) = &aberta {
        if dia < *de {
            return Err(data_invalida(
                "A data é anterior ao início da atribuição atual deste carro.",
            ));
        }
        if motorista_id == Some(*mot) {
            return Ok(()); // já está atribuído
        }
    }
    let depois: i64 = tx.query_row(
        "SELECT COUNT(*) FROM atribuicoes WHERE carro_id = ?1 AND ate IS NOT NULL AND ate > ?2",
        params![carro_id, dia],
        |r| r.get(0),
    )?;
    if depois > 0 {
        return Err(data_invalida(
            "Já existe uma atribuição deste carro depois desta data.",
        ));
    }
    if let Some((id, _, _)) = aberta {
        tx.execute(
            "UPDATE atribuicoes SET ate = ?1, atualizado_em = datetime('now') WHERE id = ?2",
            params![dia, id],
        )?;
    }
    if let Some(m) = motorista_id {
        let noutro: Option<(i64, String)> = tx
            .query_row(
                "SELECT id, de FROM atribuicoes WHERE motorista_id = ?1 AND ate IS NULL",
                [m],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((id, de)) = noutro {
            if dia < de {
                return Err(data_invalida(
                    "A data é anterior ao início da atribuição atual deste motorista.",
                ));
            }
            tx.execute(
                "UPDATE atribuicoes SET ate = ?1, atualizado_em = datetime('now') WHERE id = ?2",
                params![dia, id],
            )?;
        }
        tx.execute(
            "INSERT INTO atribuicoes (carro_id, motorista_id, de) VALUES (?1, ?2, ?3)",
            params![carro_id, m, dia],
        )?;
    }
    tx.commit()?;
    Ok(())
}

pub fn atribuicoes_de(conn: &Connection, f: &FiltroAtribuicoes) -> Res<Vec<Atribuicao>> {
    let mut st = conn.prepare(
        "SELECT a.id, a.carro_id, c.matricula, a.motorista_id, m.nome, a.de, a.ate
         FROM atribuicoes a
         JOIN carros c ON c.id = a.carro_id
         JOIN motoristas m ON m.id = a.motorista_id
         WHERE (?1 IS NULL OR a.carro_id = ?1) AND (?2 IS NULL OR a.motorista_id = ?2)
         ORDER BY a.de DESC, a.id DESC",
    )?;
    let v = st
        .query_map(params![f.carro_id, f.motorista_id], |r| {
            Ok(Atribuicao {
                id: r.get(0)?,
                carro_id: r.get(1)?,
                matricula: r.get(2)?,
                motorista_id: r.get(3)?,
                motorista_nome: r.get(4)?,
                de: r.get(5)?,
                ate: r.get(6)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

/// Todas as atribuições, no formato das regras de gastos.
pub(crate) fn atribuicoes_regras(conn: &Connection) -> Res<Vec<gastos::Atribuicao>> {
    let mut st = conn.prepare("SELECT carro_id, motorista_id, de, ate FROM atribuicoes")?;
    let v = st
        .query_map([], |r| {
            Ok(gastos::Atribuicao {
                carro_id: r.get(0)?,
                motorista_id: r.get(1)?,
                de: col_data(r, 2)?,
                ate: col_data_opt(r, 3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

/// id -> nome de todos os motoristas.
pub(crate) fn nomes_motoristas(conn: &Connection) -> Res<std::collections::HashMap<i64, String>> {
    let mut st = conn.prepare("SELECT id, nome FROM motoristas")?;
    let v = st
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(v)
}
