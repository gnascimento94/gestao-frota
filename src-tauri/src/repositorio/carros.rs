use super::consultas::{despesas, periodo_ano};
use super::{contar, data_opt, erro, fmt_data, leituras, nao_encontrado, texto};
use crate::erro::Res;
use crate::regras::gastos::{Despesa, Periodo, resumir};
use crate::regras::km::km_atual;
use crate::regras::matricula::normalizar_matricula;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Carro {
    pub id: i64,
    pub matricula: String,
    pub marca: Option<String>,
    pub modelo: Option<String>,
    pub ano: Option<i32>,
    pub cor: Option<String>,
    pub km_inicial: i64,
    pub km_atual: i64,
    pub data_aquisicao: Option<String>,
    pub estado: String,
    pub data_saida: Option<String>,
    pub notas: Option<String>,
    /// Atribuição aberta: (motorista_id, nome).
    pub motorista_atual: Option<(i64, String)>,
    /// Cêntimos gastos no ano pedido.
    pub gasto_ano: i64,
    /// Cêntimos gastos desde sempre.
    pub gasto_total: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CarroInput {
    pub id: Option<i64>,
    pub matricula: String,
    pub marca: Option<String>,
    pub modelo: Option<String>,
    pub ano: Option<i32>,
    pub cor: Option<String>,
    pub km_inicial: i64,
    pub data_aquisicao: Option<String>,
    pub estado: String,
    pub data_saida: Option<String>,
    pub notas: Option<String>,
}

const ESTADOS: [&str; 3] = ["ativo", "inativo", "vendido"];

const SELECT_CARRO: &str =
    "SELECT c.id, c.matricula, c.marca, c.modelo, c.ano, c.cor, c.km_inicial,
        c.data_aquisicao, c.estado, c.data_saida, c.notas, a.motorista_id, m.nome
    FROM carros c
    LEFT JOIN atribuicoes a ON a.carro_id = c.id AND a.ate IS NULL
    LEFT JOIN motoristas m ON m.id = a.motorista_id";

fn ler(r: &rusqlite::Row) -> rusqlite::Result<Carro> {
    let mot: Option<i64> = r.get(11)?;
    let nome: Option<String> = r.get(12)?;
    Ok(Carro {
        id: r.get(0)?,
        matricula: r.get(1)?,
        marca: r.get(2)?,
        modelo: r.get(3)?,
        ano: r.get(4)?,
        cor: r.get(5)?,
        km_inicial: r.get(6)?,
        km_atual: 0,
        data_aquisicao: r.get(7)?,
        estado: r.get(8)?,
        data_saida: r.get(9)?,
        notas: r.get(10)?,
        motorista_atual: mot.zip(nome),
        gasto_ano: 0,
        gasto_total: 0,
    })
}

/// Período que cobre qualquer data possível nos registos.
fn desde_sempre() -> Periodo {
    Periodo {
        de: chrono::NaiveDate::from_ymd_opt(1900, 1, 1).expect("data válida"),
        ate: chrono::NaiveDate::from_ymd_opt(9999, 12, 31).expect("data válida"),
    }
}

fn completar(conn: &Connection, c: &mut Carro, todas: &[Despesa], ano: i32) -> Res<()> {
    c.km_atual = km_atual(c.km_inicial, &leituras(conn, c.id)?);
    let minhas: Vec<Despesa> = todas
        .iter()
        .filter(|d| d.carro_id == c.id)
        .copied()
        .collect();
    c.gasto_ano = resumir(&minhas, &periodo_ano(ano)?).total;
    c.gasto_total = resumir(&minhas, &desde_sempre()).total;
    Ok(())
}

pub fn listar_carros(conn: &Connection, so_ativos: bool, ano: i32) -> Res<Vec<Carro>> {
    let sql = format!("{SELECT_CARRO} WHERE (?1 = 0 OR c.estado = 'ativo') ORDER BY c.matricula");
    let mut st = conn.prepare(&sql)?;
    let mut v = st
        .query_map([so_ativos], ler)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let todas = despesas(conn, None)?;
    for c in &mut v {
        completar(conn, c, &todas, ano)?;
    }
    Ok(v)
}

pub fn obter_carro(conn: &Connection, id: i64, ano: i32) -> Res<Carro> {
    let sql = format!("{SELECT_CARRO} WHERE c.id = ?1");
    let mut c = conn
        .query_row(&sql, [id], ler)
        .optional()?
        .ok_or_else(nao_encontrado)?;
    let todas = despesas(conn, Some(&[id]))?;
    completar(conn, &mut c, &todas, ano)?;
    Ok(c)
}

pub fn guardar_carro(conn: &Connection, c: &CarroInput) -> Res<i64> {
    let matricula = normalizar_matricula(&c.matricula)?;
    if c.km_inicial < 0 {
        return Err(erro("km_invalido", "Os KM não podem ser negativos."));
    }
    let estado = c.estado.trim();
    if !ESTADOS.contains(&estado) {
        return Err(erro("estado_invalido", "Estado inválido."));
    }
    let aquisicao_dia = data_opt(&c.data_aquisicao)?;
    let saida_dia = data_opt(&c.data_saida)?;
    if let (Some(a), Some(s)) = (aquisicao_dia, saida_dia)
        && s < a
    {
        return Err(erro(
            "data_saida_invalida",
            "A data de saída não pode ser anterior à data de aquisição.",
        ));
    }
    let aquisicao = aquisicao_dia.map(fmt_data);
    let saida = saida_dia.map(fmt_data);
    let duplicada: Option<i64> = conn
        .query_row(
            "SELECT id FROM carros WHERE matricula = ?1 AND id IS NOT ?2",
            params![matricula, c.id],
            |r| r.get(0),
        )
        .optional()?;
    if duplicada.is_some() {
        return Err(erro(
            "matricula_duplicada",
            format!("Já existe um carro com a matrícula {matricula}."),
        ));
    }
    let (marca, modelo, cor, notas) = (
        texto(&c.marca),
        texto(&c.modelo),
        texto(&c.cor),
        texto(&c.notas),
    );
    match c.id {
        None => {
            conn.execute(
                "INSERT INTO carros (matricula, marca, modelo, ano, cor, km_inicial, data_aquisicao,
                    estado, data_saida, notas) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    matricula,
                    marca,
                    modelo,
                    c.ano,
                    cor,
                    c.km_inicial,
                    aquisicao,
                    estado,
                    saida,
                    notas
                ],
            )?;
            Ok(conn.last_insert_rowid())
        }
        Some(id) => {
            let n = conn.execute(
                "UPDATE carros SET matricula = ?1, marca = ?2, modelo = ?3, ano = ?4, cor = ?5,
                    km_inicial = ?6, data_aquisicao = ?7, estado = ?8, data_saida = ?9,
                    notas = ?10, atualizado_em = datetime('now') WHERE id = ?11",
                params![
                    matricula,
                    marca,
                    modelo,
                    c.ano,
                    cor,
                    c.km_inicial,
                    aquisicao,
                    estado,
                    saida,
                    notas,
                    id
                ],
            )?;
            if n == 0 {
                return Err(nao_encontrado());
            }
            Ok(id)
        }
    }
}

pub fn apagar_carro(conn: &Connection, id: i64) -> Res<()> {
    let historico = [
        "leituras_km",
        "manutencoes",
        "pneus",
        "documentos",
        "multas",
        "atribuicoes",
    ]
    .iter()
    .map(|t| {
        contar(
            conn,
            &format!("SELECT COUNT(*) FROM {t} WHERE carro_id = ?1"),
            id,
        )
    })
    .sum::<Res<i64>>()?;
    if historico > 0 {
        return Err(erro(
            "tem_historico",
            "Este carro tem histórico. Marque-o como vendido ou inativo.",
        ));
    }
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM planos_carros WHERE carro_id = ?1", [id])?;
    if tx.execute("DELETE FROM carros WHERE id = ?1", [id])? == 0 {
        return Err(nao_encontrado());
    }
    tx.commit()?;
    Ok(())
}
