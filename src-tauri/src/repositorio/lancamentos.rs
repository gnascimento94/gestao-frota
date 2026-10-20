use super::motoristas::atribuicoes_regras;
use super::{
    data, data_opt, erro, fmt_data, inserir_leitura, nao_encontrado, texto, validar_data_carro,
    validar_km, validar_valor,
};
use crate::erro::Res;
use crate::regras::documentos::proximo_anual;
use crate::regras::gastos::motorista_em;
use chrono::{Months, NaiveDate};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManutencaoInput {
    pub carro_id: i64,
    pub data: String,
    pub km: Option<i64>,
    pub tipo_id: i64,
    /// "propria" | "oficina".
    pub executada_por: String,
    pub oficina: Option<String>,
    pub valor: i64,
    pub valor_pecas: Option<i64>,
    pub valor_mao_obra: Option<i64>,
    pub descricao: Option<String>,
    pub confirmado: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PneuInput {
    pub carro_id: i64,
    pub data: String,
    pub km: Option<i64>,
    /// FE | FD | TE | TD | suplente | todos.
    pub posicao: String,
    pub marca: Option<String>,
    pub modelo: Option<String>,
    /// troca | rodizio.
    pub motivo: String,
    pub valor: i64,
    pub confirmado: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentoInput {
    pub carro_id: i64,
    /// IUC | inspecao | seguro | outro.
    pub tipo: String,
    pub descricao: Option<String>,
    pub valor: i64,
    pub vencimento: String,
    pub pago_em: Option<String>,
    pub anual: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KmInput {
    pub carro_id: i64,
    pub data: String,
    pub km: i64,
    pub confirmado: bool,
}

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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoricoItem {
    /// manutencao | pneu | documento | multa | km.
    pub tipo: String,
    pub id: i64,
    pub carro_id: i64,
    pub matricula: String,
    pub data: String,
    pub km: Option<i64>,
    pub titulo: String,
    pub valor: Option<i64>,
    pub detalhe: Option<String>,
}

pub(crate) const TIPOS_DOCUMENTO: [&str; 4] = ["IUC", "inspecao", "seguro", "outro"];
const POSICOES: [&str; 6] = ["FE", "FD", "TE", "TD", "suplente", "todos"];
const MOTIVOS: [&str; 2] = ["troca", "rodizio"];
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

fn opcao(valor: &str, validos: &[&str], codigo: &str, msg: &str) -> Res<String> {
    let v = valor.trim();
    if validos.contains(&v) {
        Ok(v.to_string())
    } else {
        Err(erro(codigo, msg))
    }
}

/// Nome legível de um tipo de documento.
pub(crate) fn nome_documento(tipo: &str) -> &'static str {
    match tipo {
        "IUC" => "IUC",
        "inspecao" => "Inspeção",
        "seguro" => "Seguro",
        _ => "Outro",
    }
}

pub fn lancar_manutencao(conn: &mut Connection, m: &ManutencaoInput, hoje: NaiveDate) -> Res<i64> {
    let dia = data(&m.data)?;
    for v in [Some(m.valor), m.valor_pecas, m.valor_mao_obra]
        .into_iter()
        .flatten()
    {
        validar_valor(v)?;
    }
    let por = opcao(
        &m.executada_por,
        &["propria", "oficina"],
        "executada_por_invalida",
        "Indique quem fez a manutenção.",
    )?;
    let oficina = match por.as_str() {
        "oficina" => Some(
            texto(&m.oficina).ok_or_else(|| erro("falta_oficina", "Indique o nome da oficina."))?,
        ),
        _ => None,
    };
    let discriminado = m.valor_pecas.is_some() || m.valor_mao_obra.is_some();
    let (pecas, mao) = if discriminado {
        (
            Some(m.valor_pecas.unwrap_or(0)),
            Some(m.valor_mao_obra.unwrap_or(0)),
        )
    } else {
        (None, None)
    };
    if por == "propria" && mao.unwrap_or(0) != 0 {
        return Err(erro(
            "mao_obra_propria",
            "Numa manutenção feita por nós a mão de obra é 0.",
        ));
    }
    if let (Some(p), Some(o)) = (pecas, mao)
        && p + o != m.valor
    {
        return Err(erro(
            "valor_nao_bate",
            "O total tem de ser igual a peças + mão de obra.",
        ));
    }
    let tx = conn.transaction()?;
    validar_data_carro(&tx, m.carro_id, dia, hoje)?;
    if let Some(km) = m.km {
        validar_km(&tx, m.carro_id, dia, km, m.confirmado)?;
    }
    tx.execute(
        "INSERT INTO manutencoes (carro_id, data, km, tipo_id, executada_por, oficina, valor,
            valor_pecas, valor_mao_obra, descricao) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            m.carro_id,
            fmt_data(dia),
            m.km,
            m.tipo_id,
            por,
            oficina,
            m.valor,
            pecas,
            mao,
            texto(&m.descricao)
        ],
    )?;
    let id = tx.last_insert_rowid();
    if let Some(km) = m.km {
        inserir_leitura(&tx, m.carro_id, dia, km, "manutencao", Some(id))?;
    }
    tx.commit()?;
    Ok(id)
}

/// Oficinas já usadas, distintas, mais usadas primeiro.
pub fn oficinas_usadas(conn: &Connection) -> Res<Vec<String>> {
    let mut st = conn.prepare(
        "SELECT oficina FROM manutencoes WHERE oficina IS NOT NULL
         GROUP BY oficina ORDER BY COUNT(*) DESC, oficina",
    )?;
    let v = st
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<String>>>()?;
    Ok(v)
}

pub fn lancar_pneu(conn: &mut Connection, p: &PneuInput, hoje: NaiveDate) -> Res<i64> {
    let dia = data(&p.data)?;
    validar_valor(p.valor)?;
    let posicao = opcao(
        &p.posicao,
        &POSICOES,
        "posicao_invalida",
        "Posição do pneu inválida.",
    )?;
    let motivo = opcao(&p.motivo, &MOTIVOS, "motivo_invalido", "Motivo inválido.")?;
    let tx = conn.transaction()?;
    validar_data_carro(&tx, p.carro_id, dia, hoje)?;
    if let Some(km) = p.km {
        validar_km(&tx, p.carro_id, dia, km, p.confirmado)?;
    }
    tx.execute(
        "INSERT INTO pneus (carro_id, data, km, posicao, marca, modelo, motivo, valor)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            p.carro_id,
            fmt_data(dia),
            p.km,
            posicao,
            texto(&p.marca),
            texto(&p.modelo),
            motivo,
            p.valor
        ],
    )?;
    let id = tx.last_insert_rowid();
    if let Some(km) = p.km {
        inserir_leitura(&tx, p.carro_id, dia, km, "pneu", Some(id))?;
    }
    tx.commit()?;
    Ok(id)
}

/// Insere o documento do ano seguinte se ainda não existir; devolve o novo id.
fn gerar_seguinte(conn: &Connection, id: i64) -> Res<Option<i64>> {
    let ja: i64 = conn.query_row(
        "SELECT COUNT(*) FROM documentos WHERE gerado_de_id = ?1",
        [id],
        |r| r.get(0),
    )?;
    if ja > 0 {
        return Ok(None);
    }
    let (carro_id, tipo, descricao, valor, venc): (i64, String, Option<String>, i64, String) = conn
        .query_row(
            "SELECT carro_id, tipo, descricao, valor, vencimento FROM documentos WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )?;
    let novo = proximo_anual(
        id,
        carro_id,
        &tipo,
        descricao.as_deref().unwrap_or(""),
        valor,
        data(&venc)?,
    );
    conn.execute(
        "INSERT INTO documentos (carro_id, tipo, descricao, valor, vencimento, anual, gerado_de_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            novo.carro_id,
            novo.tipo,
            texto(&Some(novo.descricao)),
            novo.valor,
            fmt_data(novo.vencimento),
            novo.anual,
            novo.gerado_de_id
        ],
    )?;
    Ok(Some(conn.last_insert_rowid()))
}

/// Regista um documento/taxa. Se já vier pago e for anual, cria também o do ano seguinte.
pub fn lancar_documento(conn: &mut Connection, d: &DocumentoInput, hoje: NaiveDate) -> Res<i64> {
    validar_valor(d.valor)?;
    let tipo = opcao(
        &d.tipo,
        &TIPOS_DOCUMENTO,
        "tipo_invalido",
        "Tipo de documento inválido.",
    )?;
    let vencimento = data(&d.vencimento)?;
    let pago = data_opt(&d.pago_em)?;
    if pago.is_some_and(|p| p > hoje) {
        return Err(erro("data_futura", "A data não pode ser no futuro."));
    }
    let tx = conn.transaction()?;
    let existe: Option<i64> = tx
        .query_row("SELECT id FROM carros WHERE id = ?1", [d.carro_id], |r| {
            r.get(0)
        })
        .optional()?;
    if existe.is_none() {
        return Err(erro("carro_inexistente", "Carro não encontrado."));
    }
    tx.execute(
        "INSERT INTO documentos (carro_id, tipo, descricao, valor, vencimento, pago_em, anual)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            d.carro_id,
            tipo,
            texto(&d.descricao),
            d.valor,
            fmt_data(vencimento),
            pago.map(fmt_data),
            d.anual
        ],
    )?;
    let id = tx.last_insert_rowid();
    if d.anual && pago.is_some() {
        gerar_seguinte(&tx, id)?;
    }
    tx.commit()?;
    Ok(id)
}

/// Marca como pago; se `anual`, cria o do ano seguinte (uma só vez) e devolve o seu id.
pub fn pagar_documento(conn: &mut Connection, id: i64, pago_em: NaiveDate) -> Res<Option<i64>> {
    let tx = conn.transaction()?;
    let n = tx.execute(
        "UPDATE documentos SET pago_em = ?1, atualizado_em = datetime('now') WHERE id = ?2",
        params![fmt_data(pago_em), id],
    )?;
    if n == 0 {
        return Err(nao_encontrado());
    }
    let anual: bool = tx.query_row("SELECT anual FROM documentos WHERE id = ?1", [id], |r| {
        r.get(0)
    })?;
    let novo = if anual {
        gerar_seguinte(&tx, id)?
    } else {
        None
    };
    tx.commit()?;
    Ok(novo)
}

pub fn atualizar_km(conn: &mut Connection, k: &KmInput, hoje: NaiveDate) -> Res<i64> {
    let dia = data(&k.data)?;
    let tx = conn.transaction()?;
    validar_data_carro(&tx, k.carro_id, dia, hoje)?;
    validar_km(&tx, k.carro_id, dia, k.km, k.confirmado)?;
    let id = inserir_leitura(&tx, k.carro_id, dia, k.km, "manual", None)?;
    tx.commit()?;
    Ok(id)
}

/// Cria (`id: None`) ou altera uma multa. `valor_empresa` é calculado salvo em `coparticipado`.
pub fn guardar_multa(conn: &mut Connection, m: &MultaInput, hoje: NaiveDate) -> Res<i64> {
    let infracao_dia = data(&m.data_infracao)?;
    let notificacao = data_opt(&m.data_notificacao)?.map(fmt_data);
    let identificado = data_opt(&m.condutor_identificado_em)?.map(fmt_data);
    let vencimento = data_opt(&m.vencimento)?.map(fmt_data);
    let pago = data_opt(&m.pago_em)?.map(fmt_data);
    let reembolsado = data_opt(&m.reembolsado_em)?.map(fmt_data);
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
    validar_data_carro(&tx, m.carro_id, infracao_dia, hoje)?;
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

pub fn marcar_reembolsado(conn: &Connection, id: i64, data: NaiveDate) -> Res<()> {
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
) -> Res<()> {
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
    Ok(conn.query_row(
        "SELECT COUNT(*), COALESCE(SUM(valor), 0), COALESCE(SUM(valor_empresa), 0),
            COALESCE(SUM(valor - valor_empresa), 0),
            COALESCE(SUM(CASE WHEN empresa_adiantou = 1 AND reembolsado_em IS NULL
                THEN valor - valor_empresa ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN data_infracao >= ?2 THEN pontos ELSE 0 END), 0)
         FROM multas WHERE motorista_id = ?1",
        params![motorista_id, limite],
        |r| {
            Ok(ResumoMultasMotorista {
                n: r.get(0)?,
                total: r.get(1)?,
                empresa: r.get(2)?,
                colaborador: r.get(3)?,
                por_reembolsar: r.get(4)?,
                pontos_3_anos: r.get(5)?,
            })
        },
    )?)
}

/// Apaga um lançamento ("manutencao" | "pneu" | "documento" | "multa" | "km"),
/// incluindo a leitura de KM associada.
pub fn apagar_lancamento(conn: &mut Connection, tipo: &str, id: i64) -> Res<()> {
    let tx = conn.transaction()?;
    let n = match tipo {
        "manutencao" | "pneu" => {
            tx.execute(
                "DELETE FROM leituras_km WHERE origem = ?1 AND origem_id = ?2",
                params![tipo, id],
            )?;
            let tabela = if tipo == "pneu" {
                "pneus"
            } else {
                "manutencoes"
            };
            tx.execute(&format!("DELETE FROM {tabela} WHERE id = ?1"), [id])?
        }
        "documento" => {
            tx.execute(
                "UPDATE documentos SET gerado_de_id = NULL WHERE gerado_de_id = ?1",
                [id],
            )?;
            tx.execute("DELETE FROM documentos WHERE id = ?1", [id])?
        }
        "multa" => tx.execute("DELETE FROM multas WHERE id = ?1", [id])?,
        "km" => tx.execute(
            "DELETE FROM leituras_km WHERE id = ?1 AND origem = 'manual'",
            [id],
        )?,
        _ => return Err(erro("tipo_invalido", "Tipo de lançamento inválido.")),
    };
    if n == 0 {
        return Err(nao_encontrado());
    }
    tx.commit()?;
    Ok(())
}

/// Linha do tempo (mais recente primeiro) de um carro ou de todos.
pub fn historico(conn: &Connection, carro_id: Option<i64>, limite: i64) -> Res<Vec<HistoricoItem>> {
    let mut st = conn.prepare(
        "SELECT * FROM (
            SELECT 'manutencao' AS tipo, m.id, m.carro_id, m.data, m.km, t.nome AS a,
                   COALESCE(m.descricao, m.oficina) AS b, m.valor, m.criado_em
              FROM manutencoes m JOIN tipos_manutencao t ON t.id = m.tipo_id
            UNION ALL
            SELECT 'pneu', id, carro_id, data, km, posicao,
                   NULLIF(trim(COALESCE(marca, '') || ' ' || COALESCE(modelo, '')), ''),
                   valor, criado_em FROM pneus
            UNION ALL
            SELECT 'documento', id, carro_id, COALESCE(pago_em, vencimento), NULL, tipo,
                   descricao, valor, criado_em FROM documentos
            UNION ALL
            SELECT 'multa', id, carro_id, data_infracao, NULL, infracao, descricao, valor,
                   criado_em FROM multas
            UNION ALL
            SELECT 'km', id, carro_id, data, km, NULL, NULL, NULL, criado_em
              FROM leituras_km WHERE origem = 'manual'
         ) h JOIN carros c ON c.id = h.carro_id
         WHERE (?1 IS NULL OR h.carro_id = ?1)
         ORDER BY h.data DESC, h.criado_em DESC, h.id DESC
         LIMIT ?2",
    )?;
    let v = st
        .query_map(params![carro_id, limite], |r| {
            let tipo: String = r.get(0)?;
            let a: Option<String> = r.get(5)?;
            let a = a.unwrap_or_default();
            let titulo = match tipo.as_str() {
                "manutencao" => a,
                "pneu" => format!("Pneus ({a})"),
                "documento" => nome_documento(&a).to_string(),
                "multa" => format!("Multa ({a})"),
                _ => "Atualização de KM".to_string(),
            };
            Ok(HistoricoItem {
                tipo,
                id: r.get(1)?,
                carro_id: r.get(2)?,
                matricula: r.get("matricula")?,
                data: r.get(3)?,
                km: r.get(4)?,
                titulo,
                valor: r.get(7)?,
                detalhe: r.get(6)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}
