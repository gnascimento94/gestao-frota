use chrono::{Datelike, Duration, Months, NaiveDate, Weekday};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::HashSet;

#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Nivel {
    Vencido,
    Proximo,
}

#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TipoAlerta {
    Manutencao,
    Documento,
    Carta,
    IdentificarCondutor,
    Multa,
}

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Alerta {
    pub nivel: Nivel,
    pub tipo: TipoAlerta,
    pub carro_id: Option<i64>,
    pub motorista_id: Option<i64>,
    /// Plano, documento, motorista ou multa, conforme o tipo.
    pub ref_id: i64,
    pub titulo: String,
    /// Negativo quando vencido.
    pub falta_km: Option<i64>,
    /// Dias de calendário; negativo quando vencido.
    pub falta_dias: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CarroAlerta {
    pub id: i64,
    pub km_atual: i64,
    pub base_km: i64,
    /// data_aquisicao ou criado_em.
    pub base_data: NaiveDate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanoAlerta {
    pub id: i64,
    pub tipo_id: i64,
    pub tipo_nome: String,
    pub cada_km: Option<i64>,
    pub cada_meses: Option<u32>,
    /// `None` = todos os carros.
    pub carros: Option<Vec<i64>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UltimaManutencao {
    pub carro_id: i64,
    pub tipo_id: i64,
    pub data: NaiveDate,
    pub km: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Limiares {
    pub km: i64,
    pub dias: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocAlerta {
    pub id: i64,
    pub carro_id: i64,
    pub tipo_nome: String,
    pub vencimento: NaiveDate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CartaAlerta {
    pub motorista_id: i64,
    pub nome: String,
    pub validade: NaiveDate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MultaAlerta {
    pub id: i64,
    pub carro_id: i64,
    pub data_notificacao: Option<NaiveDate>,
    pub condutor_identificado: bool,
    pub vencimento: Option<NaiveDate>,
    pub pago: bool,
}

const DIAS_DOCUMENTO: i64 = 30;
const DIAS_CARTA: i64 = 60;
const DIAS_UTEIS_ALERTA_CONDUTOR: i64 = 5;
const DIAS_UTEIS_IDENTIFICAR: u32 = 15;

fn dias(a: NaiveDate, b: NaiveDate) -> i64 {
    (a - b).num_days()
}

pub fn alertas_manutencao(
    carros: &[CarroAlerta],
    planos: &[PlanoAlerta],
    ultimas: &[UltimaManutencao],
    lim: &Limiares,
    hoje: NaiveDate,
) -> Vec<Alerta> {
    let mut out = Vec::new();
    for plano in planos {
        if plano.cada_km.is_none() && plano.cada_meses.is_none() {
            continue;
        }
        for carro in carros {
            if let Some(escolhidos) = &plano.carros {
                if !escolhidos.contains(&carro.id) {
                    continue;
                }
            }
            let ultima = ultimas
                .iter()
                .filter(|u| u.carro_id == carro.id && u.tipo_id == plano.tipo_id)
                .max_by_key(|u| u.data);
            let (base_km, base_data) = match ultima {
                Some(u) => (u.km, u.data),
                None => (carro.base_km, carro.base_data),
            };
            let falta_km = plano.cada_km.map(|k| base_km + k - carro.km_atual);
            let falta_dias = plano
                .cada_meses
                .and_then(|m| base_data.checked_add_months(Months::new(m)))
                .map(|p| dias(p, hoje));

            let vencido = falta_km.is_some_and(|k| k <= 0) || falta_dias.is_some_and(|d| d <= 0);
            let proximo =
                falta_km.is_some_and(|k| k < lim.km) || falta_dias.is_some_and(|d| d < lim.dias);
            let nivel = if vencido {
                Nivel::Vencido
            } else if proximo {
                Nivel::Proximo
            } else {
                continue;
            };
            out.push(Alerta {
                nivel,
                tipo: TipoAlerta::Manutencao,
                carro_id: Some(carro.id),
                motorista_id: None,
                ref_id: plano.id,
                titulo: plano.tipo_nome.clone(),
                falta_km,
                falta_dias,
            });
        }
    }
    out
}

/// Vencido só depois do dia limite (falta < 0); até `limiar` dias antes (inclusive) é Proximo.
fn nivel_prazo(falta_dias: i64, limiar: i64) -> Option<Nivel> {
    if falta_dias < 0 {
        Some(Nivel::Vencido)
    } else if falta_dias <= limiar {
        Some(Nivel::Proximo)
    } else {
        None
    }
}

pub fn alertas_documentos(docs: &[DocAlerta], hoje: NaiveDate) -> Vec<Alerta> {
    docs.iter()
        .filter_map(|doc| {
            let falta = dias(doc.vencimento, hoje);
            Some(Alerta {
                nivel: nivel_prazo(falta, DIAS_DOCUMENTO)?,
                tipo: TipoAlerta::Documento,
                carro_id: Some(doc.carro_id),
                motorista_id: None,
                ref_id: doc.id,
                titulo: doc.tipo_nome.clone(),
                falta_km: None,
                falta_dias: Some(falta),
            })
        })
        .collect()
}

pub fn alertas_cartas(c: &[CartaAlerta], hoje: NaiveDate) -> Vec<Alerta> {
    c.iter()
        .filter_map(|carta| {
            let falta = dias(carta.validade, hoje);
            Some(Alerta {
                nivel: nivel_prazo(falta, DIAS_CARTA)?,
                tipo: TipoAlerta::Carta,
                carro_id: None,
                motorista_id: Some(carta.motorista_id),
                ref_id: carta.motorista_id,
                titulo: "Carta de condução".to_string(),
                falta_km: None,
                falta_dias: Some(falta),
            })
        })
        .collect()
}

/// Domingo de Páscoa (algoritmo de Meeus/Butcher).
fn pascoa(ano: i32) -> NaiveDate {
    let a = ano % 19;
    let b = ano / 100;
    let c = ano % 100;
    let dd = b / 4;
    let e = b % 4;
    let f = (b + 8) / 25;
    let g = (b - f + 1) / 3;
    let h = (19 * a + b - dd - g + 15) % 30;
    let i = c / 4;
    let k = c % 4;
    let l = (32 + 2 * e + 2 * i - h - k) % 7;
    let m = (a + 11 * h + 22 * l) / 451;
    let mes = (h + l - 7 * m + 114) / 31;
    let dia = (h + l - 7 * m + 114) % 31 + 1;
    NaiveDate::from_ymd_opt(ano, mes as u32, dia as u32).expect("Páscoa válida")
}

pub fn feriados_pt(ano: i32) -> Vec<NaiveDate> {
    let mut v: Vec<NaiveDate> = [
        (1, 1),
        (4, 25),
        (5, 1),
        (6, 10),
        (8, 15),
        (10, 5),
        (11, 1),
        (12, 1),
        (12, 8),
        (12, 25),
    ]
    .iter()
    .filter_map(|&(m, d)| NaiveDate::from_ymd_opt(ano, m, d))
    .collect();
    let p = pascoa(ano);
    v.push(p - Duration::days(2));
    v.push(p);
    v.push(p + Duration::days(60));
    v.sort();
    v
}

fn feriados_entre(a: NaiveDate, b: NaiveDate) -> HashSet<NaiveDate> {
    (a.year()..=b.year()).flat_map(feriados_pt).collect()
}

fn e_util(d: NaiveDate, feriados: &HashSet<NaiveDate>) -> bool {
    !matches!(d.weekday(), Weekday::Sat | Weekday::Sun) && !feriados.contains(&d)
}

/// `n` dias úteis estritamente depois de `d` (o próprio `d` não conta).
pub fn somar_dias_uteis(d: NaiveDate, n: u32) -> NaiveDate {
    let mut feriados = feriados_entre(d, d);
    let mut ano_carregado = d.year();
    let mut atual = d;
    let mut restam = n;
    while restam > 0 {
        atual += Duration::days(1);
        if atual.year() != ano_carregado {
            ano_carregado = atual.year();
            feriados.extend(feriados_pt(ano_carregado));
        }
        if e_util(atual, &feriados) {
            restam -= 1;
        }
    }
    atual
}

/// Dias úteis em (a, b]; 0 se `b <= a`.
pub fn dias_uteis_entre(a: NaiveDate, b: NaiveDate) -> i64 {
    if b <= a {
        return 0;
    }
    let feriados = feriados_entre(a, b);
    let mut n = 0;
    let mut atual = a;
    while atual < b {
        atual += Duration::days(1);
        if e_util(atual, &feriados) {
            n += 1;
        }
    }
    n
}

fn mes_pt(m: u32) -> &'static str {
    [
        "jan", "fev", "mar", "abr", "mai", "jun", "jul", "ago", "set", "out", "nov", "dez",
    ][(m as usize - 1) % 12]
}

pub fn alertas_multas(m: &[MultaAlerta], hoje: NaiveDate) -> Vec<Alerta> {
    let mut out = Vec::new();
    for multa in m {
        if !multa.condutor_identificado {
            if let Some(notif) = multa.data_notificacao {
                let prazo = somar_dias_uteis(notif, DIAS_UTEIS_IDENTIFICAR);
                let nivel = if hoje > prazo {
                    Some(Nivel::Vencido)
                } else if dias_uteis_entre(hoje, prazo) <= DIAS_UTEIS_ALERTA_CONDUTOR {
                    Some(Nivel::Proximo)
                } else {
                    None
                };
                if let Some(nivel) = nivel {
                    out.push(Alerta {
                        nivel,
                        tipo: TipoAlerta::IdentificarCondutor,
                        carro_id: Some(multa.carro_id),
                        motorista_id: None,
                        ref_id: multa.id,
                        titulo: format!(
                            "Identificar condutor até {:02} {}",
                            prazo.day(),
                            mes_pt(prazo.month())
                        ),
                        falta_km: None,
                        falta_dias: Some(dias(prazo, hoje)),
                    });
                }
            }
        }
        if !multa.pago {
            if let Some(venc) = multa.vencimento {
                let falta = dias(venc, hoje);
                if let Some(nivel) = nivel_prazo(falta, DIAS_DOCUMENTO) {
                    out.push(Alerta {
                        nivel,
                        tipo: TipoAlerta::Multa,
                        carro_id: Some(multa.carro_id),
                        motorista_id: None,
                        ref_id: multa.id,
                        titulo: "Multa por pagar".to_string(),
                        falta_km: None,
                        falta_dias: Some(falta),
                    });
                }
            }
        }
    }
    out
}

fn rank(n: Nivel) -> u8 {
    match n {
        Nivel::Vencido => 0,
        Nivel::Proximo => 1,
    }
}

fn chave(v: Option<i64>) -> i64 {
    v.unwrap_or(i64::MAX)
}

/// Vencido antes de Proximo; depois menor `falta_dias`, depois menor `falta_km` (None = +inf). Estável.
pub fn ordenar(alertas: &mut Vec<Alerta>) {
    alertas.sort_by(|a, b| -> Ordering {
        rank(a.nivel)
            .cmp(&rank(b.nivel))
            .then(chave(a.falta_dias).cmp(&chave(b.falta_dias)))
            .then(chave(a.falta_km).cmp(&chave(b.falta_km)))
    });
}

#[cfg(test)]
mod testes {
    use super::*;

    fn d(a: i32, m: u32, dia: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(a, m, dia).unwrap()
    }
    fn hoje() -> NaiveDate {
        d(2026, 10, 20)
    }
    fn lim() -> Limiares {
        Limiares {
            km: 1_000,
            dias: 30,
        }
    }
    fn carro(km_atual: i64) -> CarroAlerta {
        CarroAlerta {
            id: 1,
            km_atual,
            base_km: 50_000,
            base_data: d(2026, 9, 1),
        }
    }
    fn plano(cada_km: Option<i64>, cada_meses: Option<u32>) -> PlanoAlerta {
        PlanoAlerta {
            id: 9,
            tipo_id: 4,
            tipo_nome: "Mudança de óleo".into(),
            cada_km,
            cada_meses,
            carros: None,
        }
    }
    fn alerta(nivel: Nivel, fd: Option<i64>, fk: Option<i64>) -> Alerta {
        Alerta {
            nivel,
            tipo: TipoAlerta::Documento,
            carro_id: Some(1),
            motorista_id: None,
            ref_id: 1,
            titulo: String::new(),
            falta_km: fk,
            falta_dias: fd,
        }
    }

    #[test]
    fn plano_km_sem_historico_conta_do_km_inicial() {
        let r = alertas_manutencao(
            &[carro(64_500)],
            &[plano(Some(15_000), None)],
            &[],
            &lim(),
            hoje(),
        );
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].nivel, Nivel::Proximo);
        assert_eq!(r[0].tipo, TipoAlerta::Manutencao);
        assert_eq!(r[0].falta_km, Some(500));
        assert_eq!(r[0].falta_dias, None);
        assert_eq!(r[0].titulo, "Mudança de óleo");
        assert_eq!(r[0].carro_id, Some(1));
        assert_eq!(r[0].ref_id, 9);
    }

    #[test]
    fn vencido_por_km() {
        let r = alertas_manutencao(
            &[carro(65_200)],
            &[plano(Some(15_000), None)],
            &[],
            &lim(),
            hoje(),
        );
        assert_eq!(r[0].nivel, Nivel::Vencido);
        assert_eq!(r[0].falta_km, Some(-200));
    }

    #[test]
    fn vencido_por_data_mesmo_com_km_ok() {
        let mut c = carro(51_000);
        c.base_data = d(2025, 10, 1);
        let r = alertas_manutencao(&[c], &[plano(Some(15_000), Some(12))], &[], &lim(), hoje());
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].nivel, Nivel::Vencido);
        assert_eq!(r[0].falta_dias, Some(-19));
        assert_eq!(r[0].falta_km, Some(14_000));
    }

    #[test]
    fn ultima_manutencao_do_tipo_muda_a_base() {
        let u = UltimaManutencao {
            carro_id: 1,
            tipo_id: 4,
            data: d(2026, 9, 1),
            km: 64_000,
        };
        let r = alertas_manutencao(
            &[carro(64_500)],
            &[plano(Some(15_000), None)],
            &[u],
            &lim(),
            hoje(),
        );
        assert!(r.is_empty());
        // outro tipo não interfere
        let outro = UltimaManutencao { tipo_id: 5, ..u };
        let r = alertas_manutencao(
            &[carro(64_500)],
            &[plano(Some(15_000), None)],
            &[outro],
            &lim(),
            hoje(),
        );
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn plano_so_para_carros_escolhidos() {
        let mut p = plano(Some(15_000), None);
        p.carros = Some(vec![2]);
        let r = alertas_manutencao(&[carro(65_200)], &[p.clone()], &[], &lim(), hoje());
        assert!(r.is_empty());
        p.carros = Some(vec![1]);
        let r = alertas_manutencao(&[carro(65_200)], &[p], &[], &lim(), hoje());
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn sem_alerta_quando_longe() {
        let r = alertas_manutencao(
            &[carro(55_000)],
            &[plano(Some(15_000), Some(12))],
            &[],
            &lim(),
            hoje(),
        );
        assert!(r.is_empty());
    }

    #[test]
    fn fim_de_mes_soma_meses_sem_panico() {
        let mut c = carro(50_000);
        c.base_data = d(2026, 1, 31);
        // próxima data 2026-02-28; hoje 2026-02-20 -> 8 dias
        let r = alertas_manutencao(&[c], &[plano(None, Some(1))], &[], &lim(), d(2026, 2, 20));
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].falta_dias, Some(8));
        assert_eq!(r[0].falta_km, None);
    }

    #[test]
    fn documento_30_dias_proximo_e_passado_vencido() {
        let doc = |v| DocAlerta {
            id: 3,
            carro_id: 1,
            tipo_nome: "IUC".into(),
            vencimento: v,
        };
        let h = hoje();
        let r = alertas_documentos(&[doc(h + Duration::days(30))], h);
        assert_eq!(r[0].nivel, Nivel::Proximo);
        assert_eq!(r[0].falta_dias, Some(30));
        assert_eq!(r[0].titulo, "IUC");
        assert_eq!(r[0].tipo, TipoAlerta::Documento);
        assert!(alertas_documentos(&[doc(h + Duration::days(31))], h).is_empty());
        let r = alertas_documentos(&[doc(h - Duration::days(1))], h);
        assert_eq!(r[0].nivel, Nivel::Vencido);
        assert_eq!(r[0].falta_dias, Some(-1));
        let r = alertas_documentos(&[doc(h)], h);
        assert_eq!(r[0].nivel, Nivel::Proximo);
        assert_eq!(r[0].falta_dias, Some(0));
    }

    #[test]
    fn carta_60_dias() {
        let c = |v| CartaAlerta {
            motorista_id: 5,
            nome: "Rui".into(),
            validade: v,
        };
        let h = hoje();
        let r = alertas_cartas(&[c(h + Duration::days(60))], h);
        assert_eq!(r[0].nivel, Nivel::Proximo);
        assert_eq!(r[0].tipo, TipoAlerta::Carta);
        assert_eq!(r[0].motorista_id, Some(5));
        assert_eq!(r[0].carro_id, None);
        assert_eq!(r[0].titulo, "Carta de condução");
        assert!(alertas_cartas(&[c(h + Duration::days(61))], h).is_empty());
        let r = alertas_cartas(&[c(h - Duration::days(2))], h);
        assert_eq!(r[0].nivel, Nivel::Vencido);
    }

    #[test]
    fn ordenacao_vencidos_primeiro() {
        let mut v = vec![
            alerta(Nivel::Proximo, Some(3), None),
            alerta(Nivel::Vencido, Some(-1), None),
            alerta(Nivel::Proximo, None, Some(100)),
            alerta(Nivel::Vencido, Some(-10), None),
            alerta(Nivel::Proximo, Some(3), Some(50)),
            alerta(Nivel::Proximo, Some(2), None),
        ];
        ordenar(&mut v);
        let got: Vec<_> = v
            .iter()
            .map(|a| (a.nivel, a.falta_dias, a.falta_km))
            .collect();
        assert_eq!(
            got,
            vec![
                (Nivel::Vencido, Some(-10), None),
                (Nivel::Vencido, Some(-1), None),
                (Nivel::Proximo, Some(2), None),
                (Nivel::Proximo, Some(3), Some(50)),
                (Nivel::Proximo, Some(3), None),
                (Nivel::Proximo, None, Some(100)),
            ]
        );
    }

    #[test]
    fn pascoa_e_feriados_moveis() {
        let f26 = feriados_pt(2026);
        assert!(f26.contains(&d(2026, 4, 5)));
        assert!(f26.contains(&d(2026, 4, 3)));
        assert!(f26.contains(&d(2026, 6, 4)));
        for dt in [
            d(2026, 1, 1),
            d(2026, 4, 25),
            d(2026, 5, 1),
            d(2026, 6, 10),
            d(2026, 8, 15),
            d(2026, 10, 5),
            d(2026, 11, 1),
            d(2026, 12, 1),
            d(2026, 12, 8),
            d(2026, 12, 25),
        ] {
            assert!(f26.contains(&dt), "{dt}");
        }
        assert_eq!(f26.len(), 13);
        assert!(feriados_pt(2027).contains(&d(2027, 3, 28)));
    }

    #[test]
    fn quinze_dias_uteis_saltam_fins_de_semana_e_feriados() {
        // 2026-11-30, 12-02,03,04,07,09,10,11,14,15,16,17,18,21,22 (1 e 8 dez feriados)
        assert_eq!(somar_dias_uteis(d(2026, 11, 27), 15), d(2026, 12, 22));
        assert_eq!(somar_dias_uteis(d(2026, 12, 31), 1), d(2027, 1, 4));
        assert_eq!(somar_dias_uteis(d(2026, 10, 1), 15), d(2026, 10, 23));
        assert_eq!(dias_uteis_entre(d(2026, 10, 20), d(2026, 10, 23)), 3);
        assert_eq!(dias_uteis_entre(d(2026, 10, 23), d(2026, 10, 20)), 0);
    }

    fn multa(notif: Option<NaiveDate>, ident: bool) -> MultaAlerta {
        MultaAlerta {
            id: 11,
            carro_id: 1,
            data_notificacao: notif,
            condutor_identificado: ident,
            vencimento: None,
            pago: false,
        }
    }

    #[test]
    fn identificar_condutor_proximo_e_vencido() {
        let m = multa(Some(d(2026, 10, 1)), false);
        let r = alertas_multas(&[m], d(2026, 10, 20));
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].nivel, Nivel::Proximo);
        assert_eq!(r[0].tipo, TipoAlerta::IdentificarCondutor);
        assert_eq!(r[0].falta_dias, Some(3));
        assert_eq!(r[0].titulo, "Identificar condutor até 23 out");
        assert_eq!(r[0].carro_id, Some(1));
        assert_eq!(r[0].ref_id, 11);
        let identificado = multa(Some(d(2026, 10, 1)), true);
        assert!(alertas_multas(&[identificado], d(2026, 10, 20)).is_empty());
        // no próprio prazo ainda é Proximo
        let r = alertas_multas(&[m], d(2026, 10, 23));
        assert_eq!(r[0].nivel, Nivel::Proximo);
        assert_eq!(r[0].falta_dias, Some(0));
        let r = alertas_multas(&[m], d(2026, 10, 26));
        assert_eq!(r[0].nivel, Nivel::Vencido);
        assert_eq!(r[0].falta_dias, Some(-3));
        // fronteira: 5 dias úteis restantes -> Proximo; 6 -> nada
        assert_eq!(
            alertas_multas(&[m], d(2026, 10, 16))[0].nivel,
            Nivel::Proximo
        );
        assert!(alertas_multas(&[m], d(2026, 10, 15)).is_empty());
    }

    #[test]
    fn limiares_de_manutencao_sao_estritos() {
        let km = |atual| {
            alertas_manutencao(
                &[carro(atual)],
                &[plano(Some(15_000), None)],
                &[],
                &lim(),
                hoje(),
            )
        };
        assert!(km(64_000).is_empty()); // falta 1000
        assert_eq!(km(64_001)[0].nivel, Nivel::Proximo); // falta 999
        assert_eq!(km(65_000)[0].nivel, Nivel::Vencido); // falta 0
        let dias = |base: NaiveDate| {
            let mut c = carro(50_000);
            c.base_data = base;
            alertas_manutencao(&[c], &[plano(None, Some(1))], &[], &lim(), hoje())
        };
        assert!(dias(d(2026, 10, 19)).is_empty()); // falta 30
        assert_eq!(dias(d(2026, 10, 18))[0].nivel, Nivel::Proximo); // falta 29
        assert_eq!(dias(d(2026, 9, 20))[0].nivel, Nivel::Vencido); // falta 0
    }

    #[test]
    fn varias_ultimas_do_mesmo_tipo_usa_a_mais_recente() {
        let u = |data, km| UltimaManutencao {
            carro_id: 1,
            tipo_id: 4,
            data,
            km,
        };
        let antiga = u(d(2026, 1, 1), 50_000);
        let recente = u(d(2026, 9, 1), 64_000);
        for ultimas in [[antiga, recente], [recente, antiga]] {
            let r = alertas_manutencao(
                &[carro(64_500)],
                &[plano(Some(15_000), None)],
                &ultimas,
                &lim(),
                hoje(),
            );
            assert!(r.is_empty());
        }
    }

    #[test]
    fn multa_pagamento_como_documentos() {
        let mut m = multa(None, true);
        m.vencimento = Some(d(2026, 11, 10));
        let r = alertas_multas(&[m], hoje());
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].tipo, TipoAlerta::Multa);
        assert_eq!(r[0].titulo, "Multa por pagar");
        assert_eq!(r[0].nivel, Nivel::Proximo);
        assert_eq!(r[0].falta_dias, Some(21));
        m.pago = true;
        assert!(alertas_multas(&[m], hoje()).is_empty());
        m.pago = false;
        m.vencimento = Some(d(2026, 10, 1));
        assert_eq!(alertas_multas(&[m], hoje())[0].nivel, Nivel::Vencido);
    }
}
