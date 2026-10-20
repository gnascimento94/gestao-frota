use super::{
    Tabela, data, data_opt, erro, existe, fmt_data, inserir_leitura, nao_encontrado, nao_futura,
    opcao, texto, validar_data_carro, validar_km, validar_valor,
};
use crate::erro::{DataNoCampo, Res};
use crate::regras::documentos::proximo_anual;
use chrono::NaiveDate;
use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;

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

pub(crate) const TIPOS_DOCUMENTO: [&str; 4] = ["IUC", "inspecao", "seguro", "outro"];
const POSICOES: [&str; 6] = ["FE", "FD", "TE", "TD", "suplente", "todos"];
const MOTIVOS: [&str; 2] = ["troca", "rodizio"];

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
    // Três valores podem ser inválidos: `campo` diz qual.
    for (v, campo) in [
        (Some(m.valor), "valor"),
        (m.valor_pecas, "valorPecas"),
        (m.valor_mao_obra, "valorMaoObra"),
    ] {
        if let Some(v) = v {
            validar_valor(v).map_err(|e| e.com_campo(campo))?;
        }
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
        )
        .com_campo("valorMaoObra"));
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
    existe(&tx, Tabela::TiposManutencao, m.tipo_id)?;
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

/// Insere o documento do ano seguinte se ainda não existir e o carro estiver ativo
/// (carros inativos/vendidos ficam fora dos alertas, §6.7); devolve o novo id.
fn gerar_seguinte(conn: &Connection, id: i64) -> Res<Option<i64>> {
    let (ja, ativo): (i64, bool) = conn.query_row(
        "SELECT (SELECT COUNT(*) FROM documentos WHERE gerado_de_id = ?1),
                (SELECT c.estado = 'ativo' FROM documentos d JOIN carros c ON c.id = d.carro_id
                  WHERE d.id = ?1)",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if ja > 0 || !ativo {
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

/// Regista um documento/taxa. Se já vier pago, for anual e o carro estiver ativo,
/// cria também o do ano seguinte.
pub fn lancar_documento(conn: &mut Connection, d: &DocumentoInput, hoje: NaiveDate) -> Res<i64> {
    validar_valor(d.valor)?;
    let tipo = opcao(
        &d.tipo,
        &TIPOS_DOCUMENTO,
        "tipo_invalido",
        "Tipo de documento inválido.",
    )?;
    // Duas datas: cada erro diz a que campo se refere.
    let vencimento = data(&d.vencimento).data_no_campo("vencimento")?;
    let pago = data_opt(&d.pago_em).data_no_campo("pagoEm")?;
    if let Some(p) = pago {
        nao_futura(p, hoje).data_no_campo("pagoEm")?;
    }
    let tx = conn.transaction()?;
    existe(&tx, Tabela::Carros, d.carro_id)?;
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

/// Marca como pago; se `anual` (e o carro ativo), cria o do ano seguinte e devolve o seu id.
/// Data futura -> `data_futura`; já pago -> `ja_pago`.
pub fn pagar_documento(
    conn: &mut Connection,
    id: i64,
    pago_em: &str,
    hoje: NaiveDate,
) -> Res<Option<i64>> {
    let dia = data(pago_em)?;
    nao_futura(dia, hoje)?;
    let tx = conn.transaction()?;
    let atual: Option<(Option<String>, bool)> = tx
        .query_row(
            "SELECT pago_em, anual FROM documentos WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((pago, anual)) = atual else {
        return Err(nao_encontrado());
    };
    if pago.is_some() {
        return Err(erro("ja_pago", "Este documento já está pago."));
    }
    tx.execute(
        "UPDATE documentos SET pago_em = ?1, atualizado_em = datetime('now') WHERE id = ?2",
        params![fmt_data(dia), id],
    )?;
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
