use chrono::{Datelike, NaiveDate};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum Categoria {
    Manutencao,
    Pneus,
    Iuc,
    Inspecao,
    Seguro,
    Multa,
    Outro,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Despesa {
    pub carro_id: i64,
    pub categoria: Categoria,
    /// Cêntimos.
    pub valor: i64,
    pub pago: bool,
    pub data_ref: Option<NaiveDate>,
    /// `Some(x)`: motorista fixo (`Some(None)` = por identificar); `None`: resolver por atribuição.
    pub motorista_fixo: Option<Option<i64>>,
}

/// Período inclusivo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Periodo {
    pub de: NaiveDate,
    pub ate: NaiveDate,
}

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Resumo {
    pub total: i64,
    pub por_categoria: BTreeMap<Categoria, i64>,
    pub por_mes: BTreeMap<String, BTreeMap<Categoria, i64>>,
    pub por_carro: Vec<(i64, i64)>,
    pub por_pagar: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Atribuicao {
    pub carro_id: i64,
    pub motorista_id: i64,
    pub de: NaiveDate,
    pub ate: Option<NaiveDate>,
}

fn conta(d: &Despesa, p: &Periodo) -> Option<NaiveDate> {
    let data = d.data_ref?;
    (d.pago && data >= p.de && data <= p.ate).then_some(data)
}

pub fn resumir(despesas: &[Despesa], p: &Periodo) -> Resumo {
    let mut r = Resumo {
        total: 0,
        por_categoria: BTreeMap::new(),
        por_mes: BTreeMap::new(),
        por_carro: Vec::new(),
        por_pagar: 0,
    };
    let mut carros: BTreeMap<i64, i64> = BTreeMap::new();
    for d in despesas {
        if !d.pago {
            r.por_pagar += d.valor;
            continue;
        }
        let Some(data) = conta(d, p) else { continue };
        r.total += d.valor;
        *r.por_categoria.entry(d.categoria).or_insert(0) += d.valor;
        let mes = format!("{:04}-{:02}", data.year(), data.month());
        *r.por_mes
            .entry(mes)
            .or_default()
            .entry(d.categoria)
            .or_insert(0) += d.valor;
        *carros.entry(d.carro_id).or_insert(0) += d.valor;
    }
    r.por_carro = carros.into_iter().collect();
    r.por_carro
        .sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    r
}

pub fn motorista_em(atribs: &[Atribuicao], carro_id: i64, data: NaiveDate) -> Option<i64> {
    atribs
        .iter()
        .filter(|a| a.carro_id == carro_id && a.de <= data && a.ate.map_or(true, |f| data <= f))
        .max_by_key(|a| a.de)
        .map(|a| a.motorista_id)
}

pub fn por_motorista(
    despesas: &[Despesa],
    atribs: &[Atribuicao],
    p: &Periodo,
) -> Vec<(Option<i64>, i64)> {
    let mut m: BTreeMap<Option<i64>, i64> = BTreeMap::new();
    for d in despesas {
        let Some(data) = conta(d, p) else { continue };
        let mot = match d.motorista_fixo {
            Some(x) => x,
            None => motorista_em(atribs, d.carro_id, data),
        };
        *m.entry(mot).or_insert(0) += d.valor;
    }
    let mut v: Vec<_> = m.into_iter().collect();
    // Option ordena None primeiro; queremos Some asc e None por último.
    v.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then(a.0.is_none().cmp(&b.0.is_none()))
            .then(a.0.cmp(&b.0))
    });
    v
}

#[cfg(test)]
mod testes {
    use super::*;
    use Categoria::*;

    fn d(m: u32, dia: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, m, dia).unwrap()
    }
    fn desp(
        carro_id: i64,
        categoria: Categoria,
        valor: i64,
        pago: bool,
        data: Option<NaiveDate>,
    ) -> Despesa {
        Despesa {
            carro_id,
            categoria,
            valor,
            pago,
            data_ref: data,
            motorista_fixo: None,
        }
    }
    fn s1() -> Periodo {
        Periodo {
            de: d(1, 1),
            ate: d(6, 30),
        }
    }
    fn atr(carro_id: i64, motorista_id: i64, de: NaiveDate, ate: Option<NaiveDate>) -> Atribuicao {
        Atribuicao {
            carro_id,
            motorista_id,
            de,
            ate,
        }
    }

    #[test]
    fn soma_so_o_periodo_e_so_pagos() {
        let ds = [
            desp(1, Manutencao, 10_000, true, Some(d(3, 10))),
            desp(
                1,
                Pneus,
                40_000,
                true,
                Some(NaiveDate::from_ymd_opt(2026, 12, 1).unwrap()),
            ),
            desp(1, Iuc, 15_000, false, None),
            desp(1, Seguro, 7_000, true, None),
        ];
        let r = resumir(&ds, &s1());
        assert_eq!(r.total, 10_000);
        assert_eq!(r.por_pagar, 15_000);
    }

    #[test]
    fn limites_do_periodo_sao_inclusivos() {
        let ds = [
            desp(1, Manutencao, 100, true, Some(d(1, 1))),
            desp(1, Manutencao, 200, true, Some(d(6, 30))),
            desp(1, Manutencao, 400, true, Some(d(7, 1))),
        ];
        assert_eq!(resumir(&ds, &s1()).total, 300);
    }

    #[test]
    fn agrupa_por_mes_e_categoria() {
        let ds = [
            desp(1, Manutencao, 100, true, Some(d(3, 1))),
            desp(2, Manutencao, 250, true, Some(d(3, 20))),
            desp(1, Seguro, 900, true, Some(d(4, 2))),
        ];
        let r = resumir(&ds, &s1());
        assert_eq!(r.por_mes["2026-03"][&Manutencao], 350);
        assert_eq!(r.por_mes["2026-04"][&Seguro], 900);
        assert_eq!(r.por_mes.len(), 2);
        assert_eq!(r.por_categoria[&Manutencao], 350);
    }

    #[test]
    fn ranking_de_carros_desc() {
        let ds = [
            desp(1, Manutencao, 100, true, Some(d(3, 1))),
            desp(2, Manutencao, 500, true, Some(d(3, 1))),
            desp(3, Manutencao, 100, true, Some(d(3, 1))),
            desp(1, Pneus, 50, true, Some(d(3, 1))),
        ];
        assert_eq!(
            resumir(&ds, &s1()).por_carro,
            vec![(2, 500), (1, 150), (3, 100)]
        );
    }

    #[test]
    fn motorista_pela_data_da_despesa() {
        let a = [
            atr(1, 10, d(1, 1), Some(d(3, 31))),
            atr(1, 20, d(4, 1), None),
        ];
        assert_eq!(motorista_em(&a, 1, d(3, 15)), Some(10));
        assert_eq!(motorista_em(&a, 1, d(4, 10)), Some(20));
        let ds = [
            desp(1, Manutencao, 100, true, Some(d(3, 15))),
            desp(1, Manutencao, 300, true, Some(d(4, 10))),
        ];
        assert_eq!(
            por_motorista(&ds, &a, &s1()),
            vec![(Some(20), 300), (Some(10), 100)]
        );
    }

    #[test]
    fn troca_no_proprio_dia_conta_o_novo() {
        let a = [
            atr(1, 10, d(1, 1), Some(d(4, 1))),
            atr(1, 20, d(4, 1), None),
        ];
        assert_eq!(motorista_em(&a, 1, d(4, 1)), Some(20));
    }

    #[test]
    fn sem_atribuicao_e_sem_motorista() {
        let a = [
            atr(1, 10, d(2, 1), Some(d(2, 28))),
            atr(2, 30, d(1, 1), None),
        ];
        assert_eq!(motorista_em(&a, 1, d(3, 1)), None);
        let ds = [
            desp(1, Manutencao, 100, true, Some(d(3, 1))),
            desp(1, Manutencao, 50, true, Some(d(3, 2))),
        ];
        assert_eq!(por_motorista(&ds, &a, &s1()), vec![(None, 150)]);
    }

    #[test]
    fn multa_conta_para_o_condutor_e_so_parte_da_empresa() {
        let a = [atr(1, 20, d(5, 1), None)];
        let mut multa = desp(1, Multa, 4_000, true, Some(d(5, 20)));
        multa.motorista_fixo = Some(Some(10));
        let r = resumir(&[multa], &s1());
        assert_eq!(r.total, 4_000);
        assert_eq!(por_motorista(&[multa], &a, &s1()), vec![(Some(10), 4_000)]);
        multa.motorista_fixo = Some(None);
        assert_eq!(por_motorista(&[multa], &a, &s1()), vec![(None, 4_000)]);
    }

    #[test]
    fn ordem_de_motoristas_desempata_some_asc_e_none_por_ultimo() {
        let a = [atr(1, 5, d(1, 1), None)];
        let ds = [
            desp(2, Outro, 100, true, Some(d(3, 1))),
            desp(1, Outro, 100, true, Some(d(3, 1))),
        ];
        assert_eq!(
            por_motorista(&ds, &a, &s1()),
            vec![(Some(5), 100), (None, 100)]
        );
    }

    #[test]
    fn categoria_serializa_em_camel_case() {
        assert_eq!(
            serde_json::to_string(&Manutencao).unwrap(),
            "\"manutencao\""
        );
        assert_eq!(serde_json::to_string(&Inspecao).unwrap(), "\"inspecao\"");
    }
}
