use super::{erro, nao_encontrado};
use crate::erro::Res;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TipoManutencao {
    pub id: i64,
    pub nome: String,
    pub ativo: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plano {
    pub id: i64,
    pub tipo_id: i64,
    pub tipo_nome: String,
    pub cada_km: Option<i64>,
    pub cada_meses: Option<i64>,
    pub todos_carros: bool,
    /// Carros escolhidos (vazio quando `todos_carros`).
    pub carros: Vec<i64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanoInput {
    pub id: Option<i64>,
    pub tipo_id: i64,
    pub cada_km: Option<i64>,
    pub cada_meses: Option<i64>,
    pub todos_carros: bool,
    pub carros: Vec<i64>,
}

pub fn listar_tipos(conn: &Connection) -> Res<Vec<TipoManutencao>> {
    let mut st = conn.prepare("SELECT id, nome, ativo FROM tipos_manutencao ORDER BY id")?;
    let v = st
        .query_map([], |r| {
            Ok(TipoManutencao {
                id: r.get(0)?,
                nome: r.get(1)?,
                ativo: r.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

/// Cria (`id: None`) ou renomeia um tipo de manutenção.
pub fn guardar_tipo(conn: &Connection, id: Option<i64>, nome: &str) -> Res<i64> {
    let nome = nome.trim();
    if nome.is_empty() {
        return Err(erro("falta_nome", "Indique o nome."));
    }
    let dup: Option<i64> = conn
        .query_row(
            "SELECT id FROM tipos_manutencao WHERE nome = ?1 AND id IS NOT ?2",
            params![nome, id],
            |r| r.get(0),
        )
        .optional()?;
    if dup.is_some() {
        return Err(erro(
            "tipo_duplicado",
            format!("Já existe um tipo de manutenção chamado {nome}."),
        ));
    }
    match id {
        None => {
            conn.execute("INSERT INTO tipos_manutencao (nome) VALUES (?1)", [nome])?;
            Ok(conn.last_insert_rowid())
        }
        Some(id) => {
            let n = conn.execute(
                "UPDATE tipos_manutencao SET nome = ?1, atualizado_em = datetime('now') WHERE id = ?2",
                params![nome, id],
            )?;
            if n == 0 {
                return Err(nao_encontrado());
            }
            Ok(id)
        }
    }
}

pub fn listar_planos(conn: &Connection) -> Res<Vec<Plano>> {
    let mut st = conn.prepare(
        "SELECT p.id, p.tipo_id, t.nome, p.cada_km, p.cada_meses, p.todos_carros
         FROM planos p JOIN tipos_manutencao t ON t.id = p.tipo_id ORDER BY t.nome, p.id",
    )?;
    let mut v = st
        .query_map([], |r| {
            Ok(Plano {
                id: r.get(0)?,
                tipo_id: r.get(1)?,
                tipo_nome: r.get(2)?,
                cada_km: r.get(3)?,
                cada_meses: r.get(4)?,
                todos_carros: r.get(5)?,
                carros: Vec::new(),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut st =
        conn.prepare("SELECT carro_id FROM planos_carros WHERE plano_id = ?1 ORDER BY carro_id")?;
    for p in &mut v {
        p.carros = st
            .query_map([p.id], |r| r.get(0))?
            .collect::<rusqlite::Result<Vec<i64>>>()?;
    }
    Ok(v)
}

pub fn guardar_plano(conn: &mut Connection, p: &PlanoInput) -> Res<i64> {
    if p.cada_km.is_none() && p.cada_meses.is_none() {
        return Err(erro("plano_vazio", "Indique os km, os meses ou ambos."));
    }
    if p.cada_km.is_some_and(|k| k <= 0) || p.cada_meses.is_some_and(|m| m <= 0) {
        return Err(erro(
            "valor_invalido",
            "Os intervalos têm de ser maiores que 0.",
        ));
    }
    if !p.todos_carros && p.carros.is_empty() {
        return Err(erro("plano_sem_carros", "Escolha pelo menos um carro."));
    }
    let tx = conn.transaction()?;
    let id = match p.id {
        None => {
            tx.execute(
                "INSERT INTO planos (tipo_id, cada_km, cada_meses, todos_carros) VALUES (?1, ?2, ?3, ?4)",
                params![p.tipo_id, p.cada_km, p.cada_meses, p.todos_carros],
            )?;
            tx.last_insert_rowid()
        }
        Some(id) => {
            let n = tx.execute(
                "UPDATE planos SET tipo_id = ?1, cada_km = ?2, cada_meses = ?3, todos_carros = ?4,
                    atualizado_em = datetime('now') WHERE id = ?5",
                params![p.tipo_id, p.cada_km, p.cada_meses, p.todos_carros, id],
            )?;
            if n == 0 {
                return Err(nao_encontrado());
            }
            tx.execute("DELETE FROM planos_carros WHERE plano_id = ?1", [id])?;
            id
        }
    };
    if !p.todos_carros {
        for c in &p.carros {
            tx.execute(
                "INSERT OR IGNORE INTO planos_carros (plano_id, carro_id) VALUES (?1, ?2)",
                params![id, c],
            )?;
        }
    }
    tx.commit()?;
    Ok(id)
}

pub fn apagar_plano(conn: &mut Connection, id: i64) -> Res<()> {
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM planos_carros WHERE plano_id = ?1", [id])?;
    if tx.execute("DELETE FROM planos WHERE id = ?1", [id])? == 0 {
        return Err(nao_encontrado());
    }
    tx.commit()?;
    Ok(())
}
