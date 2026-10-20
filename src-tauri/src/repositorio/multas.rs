use super::motoristas::atribuicoes_regras;
use super::{
    Tabela, data, data_opt, erro, existe, fmt_data, nao_encontrado, nao_futura, opcao, texto,
    validar_data_carro, validar_valor,
};
use crate::erro::{DataNoCampo, Res};
use crate::regras::gastos::motorista_em;
use chrono::{Months, NaiveDate};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MultaInput {
    pub id: Option<i64>,
    pub carro_id: i64,
    pub motorista_id: Option<i64>,
    pub data_infracao: String,
    pub data_notificacao: Option<String>,
    pub condutor_identificado_em: Option<String>,
    pub infracao: String,
    pub pontos: i64,
    pub valor: i64,
    pub vencimento: Option<String>,
    pub pago_em: Option<String>,
    /// empresa | colaborador | coparticipado.
    pub responsavel: String,
    /// Só usado em `coparticipado`; nos outros casos é calculado.
    pub valor_empresa: Option<i64>,
    pub empresa_adiantou: bool,
    pub reembolsado_em: Option<String>,
    pub descricao: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResumoMultasMotorista {
    pub n: i64,
    pub total: i64,
    pub empresa: i64,
    pub colaborador: i64,
    pub por_reembolsar: i64,
    pub pontos_3_anos: i64,
}

/// Estado de uma multa. Calculado num único sítio (`ESTADO_MULTA_SQL`) para que o histórico,
/// a lista de multas e o resumo do motorista concordem.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EstadoMulta {
    PorIdentificarCondutor,
    PorPagar,
    PorReembolsar,
    Fechada,
}

impl EstadoMulta {
    pub(crate) fn de_sql(s: &str) -> Option<Self> {
        match s {
            "porIdentificarCondutor" => Some(Self::PorIdentificarCondutor),
            "porPagar" => Some(Self::PorPagar),
            "porReembolsar" => Some(Self::PorReembolsar),
            "fechada" => Some(Self::Fechada),
            _ => None,
        }
    }
}

/// Fragmento SQL (sobre a tabela `multas`) que devolve o estado como texto.
pub(crate) const ESTADO_MULTA_SQL: &str = "CASE
    WHEN motorista_id IS NULL THEN 'porIdentificarCondutor'
    WHEN pago_em IS NULL THEN 'porPagar'
    WHEN empresa_adiantou = 1 AND reembolsado_em IS NULL AND valor_empresa < valor
        THEN 'porReembolsar'
    ELSE 'fechada' END";

/// Multa tal como é mostrada nas listas.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MultaItem {
    pub id: i64,
    pub carro_id: i64,
    pub matricula: String,
    pub data_infracao: String,
    pub infracao: String,
    pub pontos: i64,
    pub valor: i64,
    pub valor_empresa: i64,
    pub valor_colaborador: i64,
    pub estado_multa: EstadoMulta,
    /// Condutor identificado.
    pub motorista_id: Option<i64>,
    pub vencimento: Option<String>,
    pub descricao: Option<String>,
}

/// Multas (mais recentes primeiro), filtradas por carro e/ou motorista.
pub fn multas_listar(
    conn: &Connection,
    carro_id: Option<i64>,
    motorista_id: Option<i64>,
) -> Res<Vec<MultaItem>> {
    let sql = format!(
        "SELECT m.id, m.carro_id, c.matricula, m.data_infracao, m.infracao, m.pontos, m.valor,
                m.valor_empresa, m.motorista_id, m.vencimento, m.descricao,
                {ESTADO_MULTA_SQL}
         FROM multas m JOIN carros c ON c.id = m.carro_id
         WHERE (?1 IS NULL OR m.carro_id = ?1) AND (?2 IS NULL OR m.motorista_id = ?2)
         ORDER BY m.data_infracao DESC, m.id DESC"
    );
    let mut st = conn.prepare(&sql)?;
    let v = st
        .query_map(params![carro_id, motorista_id], |r| {
            let valor: i64 = r.get(6)?;
            let valor_empresa: i64 = r.get(7)?;
            let estado: String = r.get(11)?;
            Ok(MultaItem {
                id: r.get(0)?,
                carro_id: r.get(1)?,
                matricula: r.get(2)?,
                data_infracao: r.get(3)?,
                infracao: r.get(4)?,
                pontos: r.get(5)?,
                valor,
                valor_empresa,
                valor_colaborador: valor - valor_empresa,
                estado_multa: EstadoMulta::de_sql(&estado).unwrap_or(EstadoMulta::Fechada),
                motorista_id: r.get(8)?,
                vencimento: r.get(9)?,
                descricao: r.get(10)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

const INFRACOES: [&str; 8] = [
    "velocidade",
    "estacionamento",
    "telemovel",
    "semaforo",
    "cinto",
    "documentos",
    "portagem",
    "outra",
];
const RESPONSAVEIS: [&str; 3] = ["empresa", "colaborador", "coparticipado"];

/// Data opcional que, se existir, não pode ser futura.
fn passada(s: &Option<String>, hoje: NaiveDate) -> Res<Option<NaiveDate>> {
    let d = data_opt(s)?;
    if let Some(d) = d {
        nao_futura(d, hoje)?;
    }
    Ok(d)
}

/// Cria (`id: None`) ou altera uma multa. `valor_empresa` é calculado salvo em `coparticipado`.
pub fn guardar_multa(conn: &mut Connection, m: &MultaInput, hoje: NaiveDate) -> Res<i64> {
    // Cada erro de data diz a que campo do input se refere (`campo`, em camelCase).
    let infracao_dia = data(&m.data_infracao).data_no_campo("dataInfracao")?;
    let notificacao = passada(&m.data_notificacao, hoje)
        .data_no_campo("dataNotificacao")?
        .map(fmt_data);
    let identificado = passada(&m.condutor_identificado_em, hoje)
        .data_no_campo("condutorIdentificadoEm")?
        .map(fmt_data);
    let vencimento = data_opt(&m.vencimento)
        .data_no_campo("vencimento")?
        .map(fmt_data);
    let pago_dia = passada(&m.pago_em, hoje).data_no_campo("pagoEm")?;
    let reembolsado = passada(&m.reembolsado_em, hoje)
        .data_no_campo("reembolsadoEm")?
        .map(fmt_data);
    if pago_dia.is_some_and(|p| p < infracao_dia) {
        return Err(erro(
            "data_invalida",
            "O pagamento não pode ser anterior à infração.",
        )
        .com_campo("pagoEm"));
    }
    let pago = pago_dia.map(fmt_data);
    validar_valor(m.valor)?;
    if !(0..=12).contains(&m.pontos) {
        return Err(erro(
            "pontos_invalidos",
            "Os pontos têm de estar entre 0 e 12.",
        ));
    }
    let infracao = opcao(
        &m.infracao,
        &INFRACOES,
        "infracao_invalida",
        "Infração inválida.",
    )?;
    let resp = opcao(
        &m.responsavel,
        &RESPONSAVEIS,
        "responsavel_invalido",
        "Indique quem paga a multa.",
    )?;
    let valor_empresa = match resp.as_str() {
        "empresa" => m.valor,
        "colaborador" => 0,
        _ => match m.valor_empresa {
            Some(v) if v > 0 && v < m.valor => v,
            _ => {
                return Err(erro(
                    "coparticipacao_invalida",
                    "A parte da empresa tem de ser maior que 0 e menor que o valor da multa.",
                ));
            }
        },
    };
    let tx = conn.transaction()?;
    validar_data_carro(&tx, m.carro_id, infracao_dia, hoje).data_no_campo("dataInfracao")?;
    if let Some(mot) = m.motorista_id {
        existe(&tx, Tabela::Motoristas, mot)?;
    }
    let dia = fmt_data(infracao_dia);
    let descricao = texto(&m.descricao);
    let id = match m.id {
        None => {
            tx.execute(
                "INSERT INTO multas (carro_id, motorista_id, data_infracao, data_notificacao,
                    condutor_identificado_em, infracao, pontos, valor, vencimento, pago_em,
                    responsavel, valor_empresa, empresa_adiantou, reembolsado_em, descricao)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
                params![
                    m.carro_id,
                    m.motorista_id,
                    dia,
                    notificacao,
                    identificado,
                    infracao,
                    m.pontos,
                    m.valor,
                    vencimento,
                    pago,
                    resp,
                    valor_empresa,
                    m.empresa_adiantou,
                    reembolsado,
                    descricao
                ],
            )?;
            tx.last_insert_rowid()
        }
        Some(id) => {
            let n = tx.execute(
                "UPDATE multas SET carro_id = ?1, motorista_id = ?2, data_infracao = ?3,
                    data_notificacao = ?4, condutor_identificado_em = ?5, infracao = ?6,
                    pontos = ?7, valor = ?8, vencimento = ?9, pago_em = ?10, responsavel = ?11,
                    valor_empresa = ?12, empresa_adiantou = ?13, reembolsado_em = ?14,
                    descricao = ?15, atualizado_em = datetime('now')
                 WHERE id = ?16",
                params![
                    m.carro_id,
                    m.motorista_id,
                    dia,
                    notificacao,
                    identificado,
                    infracao,
                    m.pontos,
                    m.valor,
                    vencimento,
                    pago,
                    resp,
                    valor_empresa,
                    m.empresa_adiantou,
                    reembolsado,
                    descricao,
                    id
                ],
            )?;
            if n == 0 {
                return Err(nao_encontrado());
            }
            id
        }
    };
    tx.commit()?;
    Ok(id)
}

/// Motorista com atribuição no carro nessa data (sugestão de condutor para uma multa).
pub fn condutor_sugerido(conn: &Connection, carro_id: i64, data: NaiveDate) -> Res<Option<i64>> {
    Ok(motorista_em(&atribuicoes_regras(conn)?, carro_id, data))
}

pub fn marcar_reembolsado(conn: &Connection, id: i64, data: NaiveDate, hoje: NaiveDate) -> Res<()> {
    nao_futura(data, hoje)?;
    let n = conn.execute(
        "UPDATE multas SET reembolsado_em = ?1, atualizado_em = datetime('now') WHERE id = ?2",
        params![fmt_data(data), id],
    )?;
    if n == 0 {
        return Err(nao_encontrado());
    }
    Ok(())
}

pub fn identificar_condutor(
    conn: &Connection,
    id: i64,
    motorista_id: i64,
    data: NaiveDate,
    hoje: NaiveDate,
) -> Res<()> {
    nao_futura(data, hoje)?;
    existe(conn, Tabela::Motoristas, motorista_id)?;
    let n = conn.execute(
        "UPDATE multas SET motorista_id = ?1, condutor_identificado_em = ?2,
            atualizado_em = datetime('now') WHERE id = ?3",
        params![motorista_id, fmt_data(data), id],
    )?;
    if n == 0 {
        return Err(nao_encontrado());
    }
    Ok(())
}

/// Resumo das multas de um motorista; pontos com infração em [hoje − 3 anos, hoje].
pub fn multas_motorista(
    conn: &Connection,
    motorista_id: i64,
    hoje: NaiveDate,
) -> Res<ResumoMultasMotorista> {
    let limite = hoje
        .checked_sub_months(Months::new(36))
        .map(fmt_data)
        .unwrap_or_default();
    let sql = format!(
        "SELECT COUNT(*), COALESCE(SUM(valor), 0), COALESCE(SUM(valor_empresa), 0),
            COALESCE(SUM(valor - valor_empresa), 0),
            COALESCE(SUM(CASE WHEN {ESTADO_MULTA_SQL} = 'porReembolsar'
                THEN valor - valor_empresa ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN data_infracao >= ?2 THEN pontos ELSE 0 END), 0)
         FROM multas WHERE motorista_id = ?1"
    );
    Ok(conn.query_row(&sql, params![motorista_id, limite], |r| {
        Ok(ResumoMultasMotorista {
            n: r.get(0)?,
            total: r.get(1)?,
            empresa: r.get(2)?,
            colaborador: r.get(3)?,
            por_reembolsar: r.get(4)?,
            pontos_3_anos: r.get(5)?,
        })
    })?)
}
