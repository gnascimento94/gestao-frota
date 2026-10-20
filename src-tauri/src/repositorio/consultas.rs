use super::config::obter_limiares;
use super::lancamentos::{HistoricoItem, historico, nome_documento};
use super::motoristas::{atribuicoes_regras, nomes_motoristas};
use super::{col_data, col_data_opt, data, erro};
use crate::erro::Res;
use crate::regras::alertas::{
    Alerta, CarroAlerta, CartaAlerta, DocAlerta, MultaAlerta, PlanoAlerta, UltimaManutencao,
    alertas_cartas, alertas_documentos, alertas_manutencao, alertas_multas, ordenar,
};
use crate::regras::gastos::{
    self, Categoria, Despesa, Periodo, Resumo, motorista_em, por_motorista, resumir,
};
use crate::regras::km::km_atual;
use chrono::{Datelike, NaiveDate};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Painel {
    pub gasto_mes: i64,
    pub gasto_ano: i64,
    pub por_pagar: i64,
    pub carros_ativos: i64,
    pub alertas: Vec<Alerta>,
    pub recentes: Vec<HistoricoItem>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FiltroRelatorio {
    pub de: String,
    pub ate: String,
    pub carro_ids: Option<Vec<i64>>,
    pub motorista_id: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelManutencao {
    pub interna: i64,
    pub oficina: i64,
    pub pecas: i64,
    pub mao_obra: i64,
    pub nao_discriminado: i64,
    pub por_oficina: Vec<(String, i64)>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelMultas {
    pub n: i64,
    pub total: i64,
    pub empresa: i64,
    pub colaboradores: i64,
    pub por_reembolsar: i64,
    /// (infração, n.º, total)
    pub por_infracao: Vec<(String, i64, i64)>,
    /// (motorista, nome, n.º, pontos)
    pub por_motorista: Vec<(Option<i64>, String, i64, i64)>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Relatorio {
    pub resumo: Resumo,
    pub por_motorista: Vec<(Option<i64>, String, i64)>,
    pub por_tipo_manutencao: Vec<(String, i64)>,
    pub manutencao: RelManutencao,
    pub multas: RelMultas,
}

pub(crate) fn periodo_ano(ano: i32) -> Res<Periodo> {
    let d = |m, dia| {
        NaiveDate::from_ymd_opt(ano, m, dia).ok_or_else(|| erro("data_invalida", "Ano inválido."))
    };
    Ok(Periodo {
        de: d(1, 1)?,
        ate: d(12, 31)?,
    })
}

fn categoria_documento(tipo: &str) -> Categoria {
    match tipo {
        "IUC" => Categoria::Iuc,
        "inspecao" => Categoria::Inspecao,
        "seguro" => Categoria::Seguro,
        _ => Categoria::Outro,
    }
}

fn quer(carro_ids: Option<&[i64]>, carro_id: i64) -> bool {
    carro_ids.is_none_or(|ids| ids.contains(&carro_id))
}

/// Todas as despesas (manutenções, pneus, documentos e parte da empresa das multas).
pub fn despesas(conn: &Connection, carro_ids: Option<&[i64]>) -> Res<Vec<Despesa>> {
    let mut out = Vec::new();
    for tabela in ["manutencoes", "pneus"] {
        let categoria = if tabela == "pneus" {
            Categoria::Pneus
        } else {
            Categoria::Manutencao
        };
        let mut st = conn.prepare(&format!("SELECT carro_id, valor, data FROM {tabela}"))?;
        let v = st
            .query_map([], |r| {
                Ok(Despesa {
                    carro_id: r.get(0)?,
                    categoria,
                    valor: r.get(1)?,
                    pago: true,
                    data_ref: Some(col_data(r, 2)?),
                    motorista_fixo: None,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        out.extend(v);
    }
    let mut st = conn.prepare("SELECT carro_id, valor, pago_em, tipo FROM documentos")?;
    let v = st
        .query_map([], |r| {
            let pago_em = col_data_opt(r, 2)?;
            let tipo: String = r.get(3)?;
            Ok(Despesa {
                carro_id: r.get(0)?,
                categoria: categoria_documento(&tipo),
                valor: r.get(1)?,
                pago: pago_em.is_some(),
                data_ref: pago_em,
                motorista_fixo: None,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    out.extend(v);
    let mut st =
        conn.prepare("SELECT carro_id, valor_empresa, pago_em, motorista_id FROM multas")?;
    let v = st
        .query_map([], |r| {
            let pago_em = col_data_opt(r, 2)?;
            Ok(Despesa {
                carro_id: r.get(0)?,
                categoria: Categoria::Multa,
                valor: r.get(1)?,
                pago: pago_em.is_some(),
                data_ref: pago_em,
                motorista_fixo: Some(r.get(3)?),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    out.extend(v);
    out.retain(|d| quer(carro_ids, d.carro_id));
    Ok(out)
}

/// Motorista a quem a despesa é atribuída (o mesmo critério de `por_motorista`).
fn motorista_da(d: &Despesa, atribs: &[gastos::Atribuicao]) -> Option<i64> {
    match d.motorista_fixo {
        Some(m) => m,
        None => d
            .data_ref
            .and_then(|dr| motorista_em(atribs, d.carro_id, dr)),
    }
}

pub fn painel(conn: &Connection, hoje: NaiveDate) -> Res<Painel> {
    let todas = despesas(conn, None)?;
    let inicio_mes = hoje.with_day(1).unwrap_or(hoje);
    let fim_mes = inicio_mes
        .checked_add_months(chrono::Months::new(1))
        .and_then(|d| d.pred_opt())
        .unwrap_or(hoje);
    let mes = resumir(
        &todas,
        &Periodo {
            de: inicio_mes,
            ate: fim_mes,
        },
    );
    let ano = resumir(&todas, &periodo_ano(hoje.year())?);
    let carros_ativos: i64 = conn.query_row(
        "SELECT COUNT(*) FROM carros WHERE estado = 'ativo'",
        [],
        |r| r.get(0),
    )?;
    Ok(Painel {
        gasto_mes: mes.total,
        gasto_ano: ano.total,
        por_pagar: ano.por_pagar,
        carros_ativos,
        alertas: alertas(conn, hoje)?,
        recentes: historico(conn, None, 10)?,
    })
}

fn ordenar_desc(m: HashMap<String, i64>) -> Vec<(String, i64)> {
    let mut v: Vec<_> = m.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    v
}

pub fn relatorio(conn: &Connection, f: &FiltroRelatorio) -> Res<Relatorio> {
    let p = Periodo {
        de: data(&f.de)?,
        ate: data(&f.ate)?,
    };
    if p.de > p.ate {
        return Err(erro(
            "data_invalida",
            "A data inicial é depois da data final.",
        ));
    }
    let ids = f.carro_ids.as_deref();
    let atribs = atribuicoes_regras(conn)?;
    let nomes = nomes_motoristas(conn)?;
    let nome = |m: Option<i64>, sem: &str| {
        m.and_then(|id| nomes.get(&id).cloned())
            .unwrap_or_else(|| sem.to_string())
    };
    let mut ds = despesas(conn, ids)?;
    if let Some(m) = f.motorista_id {
        ds.retain(|d| motorista_da(d, &atribs) == Some(m));
    }
    let resumo = resumir(&ds, &p);
    let por_mot = por_motorista(&ds, &atribs, &p)
        .into_iter()
        .map(|(m, v)| (m, nome(m, "Sem motorista"), v))
        .collect();

    let (manutencao, por_tipo) = rel_manutencao(conn, &p, ids, f.motorista_id, &atribs)?;
    let mut multas = rel_multas(conn, &p, ids, f.motorista_id)?;
    for linha in &mut multas.por_motorista {
        linha.1 = nome(linha.0, "Por identificar");
    }
    Ok(Relatorio {
        resumo,
        por_motorista: por_mot,
        por_tipo_manutencao: por_tipo,
        manutencao,
        multas,
    })
}

fn rel_manutencao(
    conn: &Connection,
    p: &Periodo,
    ids: Option<&[i64]>,
    motorista: Option<i64>,
    atribs: &[gastos::Atribuicao],
) -> Res<(RelManutencao, Vec<(String, i64)>)> {
    let mut st = conn.prepare(
        "SELECT m.carro_id, m.data, t.nome, m.executada_por, m.oficina, m.valor, m.valor_pecas,
                m.valor_mao_obra
         FROM manutencoes m JOIN tipos_manutencao t ON t.id = m.tipo_id",
    )?;
    type Linha = (
        i64,
        NaiveDate,
        String,
        String,
        Option<String>,
        i64,
        Option<i64>,
        Option<i64>,
    );
    let linhas = st
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                col_data(r, 1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<Linha>>>()?;
    let mut rel = RelManutencao::default();
    let mut por_tipo = HashMap::new();
    let mut por_oficina = HashMap::new();
    for (carro, dia, tipo, por, oficina, valor, pecas, mao) in linhas {
        if dia < p.de || dia > p.ate || !quer(ids, carro) {
            continue;
        }
        if motorista.is_some() && motorista_em(atribs, carro, dia) != motorista {
            continue;
        }
        *por_tipo.entry(tipo).or_insert(0) += valor;
        if por == "oficina" {
            rel.oficina += valor;
            *por_oficina.entry(oficina.unwrap_or_default()).or_insert(0) += valor;
        } else {
            rel.interna += valor;
        }
        match (pecas, mao) {
            (None, None) => rel.nao_discriminado += valor,
            _ => {
                rel.pecas += pecas.unwrap_or(0);
                rel.mao_obra += mao.unwrap_or(0);
            }
        }
    }
    rel.por_oficina = ordenar_desc(por_oficina);
    Ok((rel, ordenar_desc(por_tipo)))
}

/// Multas com infração no período (nomes dos motoristas preenchidos por quem chama).
fn rel_multas(
    conn: &Connection,
    p: &Periodo,
    ids: Option<&[i64]>,
    motorista: Option<i64>,
) -> Res<RelMultas> {
    let mut st = conn.prepare(
        "SELECT carro_id, motorista_id, data_infracao, infracao, pontos, valor, valor_empresa,
                empresa_adiantou, reembolsado_em IS NOT NULL
         FROM multas",
    )?;
    type Linha = (
        i64,
        Option<i64>,
        NaiveDate,
        String,
        i64,
        i64,
        i64,
        bool,
        bool,
    );
    let linhas = st
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                col_data(r, 2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
                r.get(8)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<Linha>>>()?;
    let mut rel = RelMultas::default();
    let mut por_infracao: BTreeMap<String, (i64, i64)> = BTreeMap::new();
    let mut por_mot: BTreeMap<Option<i64>, (i64, i64)> = BTreeMap::new();
    for (carro, mot, dia, infracao, pontos, valor, empresa, adiantou, reembolsado) in linhas {
        if dia < p.de || dia > p.ate || !quer(ids, carro) {
            continue;
        }
        if motorista.is_some() && mot != motorista {
            continue;
        }
        let colaborador = valor - empresa;
        rel.n += 1;
        rel.total += valor;
        rel.empresa += empresa;
        rel.colaboradores += colaborador;
        if adiantou && !reembolsado {
            rel.por_reembolsar += colaborador;
        }
        let e = por_infracao.entry(infracao).or_insert((0, 0));
        e.0 += 1;
        e.1 += valor;
        let e = por_mot.entry(mot).or_insert((0, 0));
        e.0 += 1;
        e.1 += pontos;
    }
    rel.por_infracao = por_infracao
        .into_iter()
        .map(|(i, (n, t))| (i, n, t))
        .collect();
    rel.por_infracao
        .sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0)));
    rel.por_motorista = por_mot
        .into_iter()
        .map(|(m, (n, pts))| (m, String::new(), n, pts))
        .collect();
    rel.por_motorista.sort_by(|a, b| {
        b.2.cmp(&a.2)
            .then(a.0.is_none().cmp(&b.0.is_none()))
            .then(a.0.cmp(&b.0))
    });
    Ok(rel)
}

/// Alertas de carros ativos (manutenção, documentos, multas) e de motoristas ativos (carta).
pub fn alertas(conn: &Connection, hoje: NaiveDate) -> Res<Vec<Alerta>> {
    let mut st = conn.prepare(
        "SELECT id, km_inicial, COALESCE(data_aquisicao, criado_em) FROM carros WHERE estado = 'ativo'",
    )?;
    let base = st
        .query_map([], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, col_data(r, 2)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut carros = Vec::with_capacity(base.len());
    for (id, km_inicial, base_data) in base {
        carros.push(CarroAlerta {
            id,
            km_atual: km_atual(km_inicial, &super::leituras(conn, id)?),
            base_km: km_inicial,
            base_data,
        });
    }
    let ativo = |id: i64| carros.iter().any(|c| c.id == id);

    let mut st = conn.prepare(
        "SELECT p.id, p.tipo_id, t.nome, p.cada_km, p.cada_meses, p.todos_carros
         FROM planos p JOIN tipos_manutencao t ON t.id = p.tipo_id",
    )?;
    let mut planos = st
        .query_map([], |r| {
            let todos: bool = r.get(5)?;
            Ok(PlanoAlerta {
                id: r.get(0)?,
                tipo_id: r.get(1)?,
                tipo_nome: r.get(2)?,
                cada_km: r.get(3)?,
                cada_meses: r.get(4)?,
                carros: (!todos).then(Vec::new),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut st = conn.prepare("SELECT carro_id FROM planos_carros WHERE plano_id = ?1")?;
    for p in &mut planos {
        if let Some(v) = &mut p.carros {
            *v = st
                .query_map([p.id], |r| r.get(0))?
                .collect::<rusqlite::Result<Vec<i64>>>()?;
        }
    }

    // KM da manutenção; sem KM, a maior leitura até essa data (ou o km_inicial).
    let mut st = conn.prepare(
        "SELECT m.carro_id, m.tipo_id, m.data,
                COALESCE(m.km, (SELECT MAX(l.km) FROM leituras_km l
                                 WHERE l.carro_id = m.carro_id AND l.data <= m.data), c.km_inicial)
         FROM manutencoes m JOIN carros c ON c.id = m.carro_id WHERE c.estado = 'ativo'",
    )?;
    let ultimas = st
        .query_map([], |r| {
            Ok(UltimaManutencao {
                carro_id: r.get(0)?,
                tipo_id: r.get(1)?,
                data: col_data(r, 2)?,
                km: r.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let lim = obter_limiares(conn)?;

    let mut st = conn.prepare(
        "SELECT id, carro_id, tipo, descricao, vencimento FROM documentos WHERE pago_em IS NULL",
    )?;
    let docs = st
        .query_map([], |r| {
            let tipo: String = r.get(2)?;
            let desc: Option<String> = r.get(3)?;
            let tipo_nome = match (tipo.as_str(), desc) {
                ("outro", Some(d)) => d,
                (t, _) => nome_documento(t).to_string(),
            };
            Ok(DocAlerta {
                id: r.get(0)?,
                carro_id: r.get(1)?,
                tipo_nome,
                vencimento: col_data(r, 4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .filter(|d| ativo(d.carro_id))
        .collect::<Vec<_>>();

    let mut st = conn.prepare(
        "SELECT id, nome, carta_validade FROM motoristas WHERE ativo = 1 AND carta_validade IS NOT NULL",
    )?;
    let cartas = st
        .query_map([], |r| {
            Ok(CartaAlerta {
                motorista_id: r.get(0)?,
                nome: r.get(1)?,
                validade: col_data(r, 2)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut st = conn.prepare(
        "SELECT id, carro_id, data_notificacao, condutor_identificado_em IS NOT NULL, vencimento,
                pago_em IS NOT NULL FROM multas",
    )?;
    let multas = st
        .query_map([], |r| {
            Ok(MultaAlerta {
                id: r.get(0)?,
                carro_id: r.get(1)?,
                data_notificacao: col_data_opt(r, 2)?,
                condutor_identificado: r.get(3)?,
                vencimento: col_data_opt(r, 4)?,
                pago: r.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .filter(|m| ativo(m.carro_id))
        .collect::<Vec<_>>();

    let mut out = alertas_manutencao(&carros, &planos, &ultimas, &lim, hoje);
    out.extend(alertas_documentos(&docs, hoje));
    out.extend(alertas_cartas(&cartas, hoje));
    out.extend(alertas_multas(&multas, hoje));
    ordenar(&mut out);
    Ok(out)
}
