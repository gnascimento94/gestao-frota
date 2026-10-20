use super::lancamentos::nome_documento;
use crate::erro::Res;
use rusqlite::{Connection, params};
use serde::Serialize;

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

/// Linha do tempo (mais recente primeiro) de um carro ou de todos.
///
/// Documentos por pagar não têm data de acontecimento: usam a data de registo
/// (`criado_em`) e, no mesmo dia, ficam depois dos lançamentos com data real;
/// o vencimento aparece no `detalhe`.
pub fn historico(conn: &Connection, carro_id: Option<i64>, limite: i64) -> Res<Vec<HistoricoItem>> {
    let mut st = conn.prepare(
        "SELECT h.tipo, h.id, h.carro_id, h.data, h.km, h.a, h.b, h.valor, h.vence, c.matricula
         FROM (
            SELECT 'manutencao' AS tipo, m.id, m.carro_id, m.data, m.km, t.nome AS a,
                   COALESCE(m.descricao, m.oficina) AS b, m.valor, NULL AS vence, 0 AS ord,
                   m.criado_em
              FROM manutencoes m JOIN tipos_manutencao t ON t.id = m.tipo_id
            UNION ALL
            SELECT 'pneu', id, carro_id, data, km, posicao,
                   NULLIF(trim(COALESCE(marca, '') || ' ' || COALESCE(modelo, '')), ''),
                   valor, NULL, 0, criado_em FROM pneus
            UNION ALL
            SELECT 'documento', id, carro_id, COALESCE(pago_em, substr(criado_em, 1, 10)), NULL,
                   tipo, descricao, valor, CASE WHEN pago_em IS NULL THEN vencimento END,
                   CASE WHEN pago_em IS NULL THEN 1 ELSE 0 END, criado_em FROM documentos
            UNION ALL
            SELECT 'multa', id, carro_id, data_infracao, NULL, infracao,
                   CASE WHEN motorista_id IS NULL THEN 'por identificar condutor'
                        WHEN pago_em IS NULL THEN 'por pagar'
                        WHEN empresa_adiantou = 1 AND reembolsado_em IS NULL
                             AND valor_empresa < valor THEN 'por reembolsar'
                        ELSE 'fechada' END || COALESCE(' · ' || descricao, ''),
                   valor, NULL, 0, criado_em FROM multas
            UNION ALL
            SELECT 'km', id, carro_id, data, km, NULL, NULL, NULL, NULL, 0, criado_em
              FROM leituras_km WHERE origem = 'manual'
         ) h JOIN carros c ON c.id = h.carro_id
         WHERE (?1 IS NULL OR h.carro_id = ?1)
         ORDER BY h.data DESC, h.ord, h.criado_em DESC, h.id DESC
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
            let b: Option<String> = r.get(6)?;
            let vence: Option<String> = r.get(8)?;
            let detalhe = match (b, vence) {
                (Some(b), Some(v)) => Some(format!("{b} · vence a {v}")),
                (None, Some(v)) => Some(format!("vence a {v}")),
                (b, None) => b,
            };
            Ok(HistoricoItem {
                tipo,
                id: r.get(1)?,
                carro_id: r.get(2)?,
                matricula: r.get(9)?,
                data: r.get(3)?,
                km: r.get(4)?,
                titulo,
                valor: r.get(7)?,
                detalhe,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}
