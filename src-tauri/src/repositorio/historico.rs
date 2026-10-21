use super::lancamentos::{nome_documento, nome_posicoes};
use super::multas::{ESTADO_MULTA_SQL, EstadoMulta};
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
    /// Documentos ainda por pagar.
    pub por_pagar: bool,
    /// Prazo de pagamento (documentos e multas).
    pub vencimento: Option<String>,
    /// Multas: condutor identificado.
    pub motorista_id: Option<i64>,
    /// Só nas multas.
    pub estado_multa: Option<EstadoMulta>,
}

/// Linha do tempo (mais recente primeiro) de um carro ou de todos.
///
/// Documentos por pagar não têm data de acontecimento: usam a data de registo
/// (`criado_em`) e, no mesmo dia, ficam depois dos lançamentos com data real;
/// o prazo vem em `vencimento`.
pub fn historico(conn: &Connection, carro_id: Option<i64>, limite: i64) -> Res<Vec<HistoricoItem>> {
    let sql = format!(
        "SELECT h.tipo, h.id, h.carro_id, h.data, h.km, h.a, h.b, h.valor, h.vence, h.estado,
                h.por_pagar, h.mot, c.matricula
         FROM (
            SELECT 'manutencao' AS tipo, m.id, m.carro_id, m.data, m.km, t.nome AS a,
                   COALESCE(m.descricao, m.oficina) AS b, m.valor, NULL AS vence, 0 AS ord,
                   m.criado_em, NULL AS estado, 0 AS por_pagar, NULL AS mot
              FROM manutencoes m JOIN tipos_manutencao t ON t.id = m.tipo_id
            UNION ALL
            SELECT 'pneu', id, carro_id, data, km, posicao,
                   NULLIF(trim(COALESCE(marca, '') || ' ' || COALESCE(modelo, '')), ''),
                   valor, NULL, 0, criado_em, NULL, 0, NULL FROM pneus
            UNION ALL
            SELECT 'documento', id, carro_id, COALESCE(pago_em, substr(criado_em, 1, 10)), NULL,
                   tipo, descricao, valor, vencimento,
                   CASE WHEN pago_em IS NULL THEN 1 ELSE 0 END, criado_em,
                   NULL, CASE WHEN pago_em IS NULL THEN 1 ELSE 0 END, NULL FROM documentos
            UNION ALL
            SELECT 'multa', id, carro_id, data_infracao, NULL, infracao, descricao, valor,
                   vencimento, 0, criado_em, {ESTADO_MULTA_SQL}, 0, motorista_id FROM multas
            UNION ALL
            SELECT 'km', id, carro_id, data, km, NULL, NULL, NULL, NULL, 0, criado_em,
                   NULL, 0, NULL
              FROM leituras_km WHERE origem = 'manual'
         ) h JOIN carros c ON c.id = h.carro_id
         WHERE (?1 IS NULL OR h.carro_id = ?1)
         ORDER BY h.data DESC, h.ord, h.criado_em DESC, h.id DESC
         LIMIT ?2"
    );
    let mut st = conn.prepare(&sql)?;
    let v = st
        .query_map(params![carro_id, limite], |r| {
            let tipo: String = r.get(0)?;
            let a: Option<String> = r.get(5)?;
            let a = a.unwrap_or_default();
            let titulo = match tipo.as_str() {
                "manutencao" => a,
                "pneu" => format!("Pneus ({})", nome_posicoes(&a)),
                "documento" => nome_documento(&a).to_string(),
                "multa" => format!("Multa ({a})"),
                _ => "Atualização de KM".to_string(),
            };
            let estado: Option<String> = r.get(9)?;
            Ok(HistoricoItem {
                tipo,
                id: r.get(1)?,
                carro_id: r.get(2)?,
                matricula: r.get(12)?,
                data: r.get(3)?,
                km: r.get(4)?,
                titulo,
                valor: r.get(7)?,
                detalhe: r.get(6)?,
                por_pagar: r.get::<_, i64>(10)? == 1,
                vencimento: r.get(8)?,
                motorista_id: r.get(11)?,
                estado_multa: estado.as_deref().and_then(EstadoMulta::de_sql),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}
