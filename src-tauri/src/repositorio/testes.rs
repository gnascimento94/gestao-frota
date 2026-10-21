use super::carros::*;
use super::config::*;
use super::consultas::*;
use super::historico::*;
use super::lancamentos::*;
use super::motoristas::*;
use super::multas::*;
use super::planos::*;
use crate::migracoes::migrar;
use crate::regras::alertas::{Limiares, Motivo, Nivel, TipoAlerta};
use crate::regras::gastos::Categoria;
use chrono::NaiveDate;
use rusqlite::Connection;

fn d(a: i32, m: u32, dia: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(a, m, dia).unwrap()
}
fn hoje() -> NaiveDate {
    d(2026, 10, 20)
}
fn bd() -> Connection {
    let mut c = Connection::open_in_memory().unwrap();
    c.execute_batch("PRAGMA foreign_keys=ON").unwrap();
    migrar(&mut c).unwrap();
    c
}
fn contar(c: &Connection, sql: &str) -> i64 {
    c.query_row(sql, [], |r| r.get(0)).unwrap()
}

fn carro_input(mat: &str) -> CarroInput {
    CarroInput {
        id: None,
        matricula: mat.into(),
        marca: Some("Renault".into()),
        modelo: Some("Kangoo".into()),
        ano: Some(2020),
        cor: Some(" ".into()),
        km_inicial: 50_000,
        data_aquisicao: None,
        estado: "ativo".into(),
        data_saida: None,
        notas: None,
    }
}
fn carro(c: &Connection, mat: &str) -> i64 {
    guardar_carro(c, &carro_input(mat)).unwrap()
}
fn motorista(c: &Connection, nome: &str) -> i64 {
    guardar_motorista(
        c,
        &MotoristaInput {
            id: None,
            nome: nome.into(),
            telefone: None,
            carta_numero: None,
            carta_validade: None,
            ativo: true,
            notas: None,
        },
    )
    .unwrap()
}
fn manut(carro_id: i64, data: &str, km: Option<i64>) -> ManutencaoInput {
    ManutencaoInput {
        carro_id,
        data: data.into(),
        km,
        tipo_id: 1,
        executada_por: "propria".into(),
        oficina: None,
        valor: 1_000,
        valor_pecas: None,
        valor_mao_obra: None,
        descricao: None,
        confirmado: false,
    }
}
fn pneu(carro_id: i64, data: &str, valor: i64) -> PneuInput {
    PneuInput {
        carro_id,
        data: data.into(),
        km: None,
        posicao: "todos".into(),
        marca: None,
        modelo: None,
        motivo: "troca".into(),
        valor,
        confirmado: false,
    }
}
fn doc(carro_id: i64, tipo: &str, valor: i64, venc: &str, pago: Option<&str>) -> DocumentoInput {
    DocumentoInput {
        carro_id,
        tipo: tipo.into(),
        descricao: None,
        valor,
        vencimento: venc.into(),
        pago_em: pago.map(Into::into),
        anual: false,
    }
}
fn multa(carro_id: i64, mot: Option<i64>, data: &str, valor: i64, resp: &str) -> MultaInput {
    MultaInput {
        id: None,
        carro_id,
        motorista_id: mot,
        data_infracao: data.into(),
        data_notificacao: None,
        condutor_identificado_em: None,
        infracao: "velocidade".into(),
        pontos: 0,
        valor,
        vencimento: None,
        pago_em: None,
        responsavel: resp.into(),
        valor_empresa: None,
        empresa_adiantou: false,
        reembolsado_em: None,
        descricao: None,
    }
}
fn km(carro_id: i64, data: &str, km: i64, confirmado: bool) -> KmInput {
    KmInput {
        carro_id,
        data: data.into(),
        km,
        confirmado,
    }
}

#[test]
fn matricula_duplicada_em_formatos_diferentes() {
    let c = bd();
    let id = carro(&c, "aa12bb");
    let e = guardar_carro(&c, &carro_input("AA-12-BB")).unwrap_err();
    assert_eq!(e.codigo, "matricula_duplicada");
    assert_eq!(e.mensagem, "Já existe um carro com a matrícula AA-12-BB.");
    // atualizar o próprio carro com a mesma matrícula é permitido
    let mut i = carro_input("AA 12 BB");
    i.id = Some(id);
    assert_eq!(guardar_carro(&c, &i).unwrap(), id);
    let car = obter_carro(&c, id, 2026).unwrap();
    assert_eq!(car.matricula, "AA-12-BB");
    assert_eq!(car.cor, None); // string vazia -> NULL
    assert_eq!(
        guardar_carro(&c, &carro_input("xx")).unwrap_err().codigo,
        "matricula_invalida"
    );
}

#[test]
fn nao_apaga_carro_com_historico() {
    let mut c = bd();
    let a = carro(&c, "AA-12-BB");
    let b = carro(&c, "CC-34-DD");
    lancar_manutencao(&mut c, &manut(a, "2026-10-01", None), hoje()).unwrap();
    let e = apagar_carro(&c, a).unwrap_err();
    assert_eq!(e.codigo, "tem_historico");
    assert_eq!(
        e.mensagem,
        "Este carro tem histórico. Marque-o como vendido ou inativo."
    );
    apagar_carro(&c, b).unwrap();
    assert_eq!(contar(&c, "SELECT COUNT(*) FROM carros"), 1);
    // motorista com atribuição também tem histórico
    let m = motorista(&c, "Ana");
    atribuir(&mut c, a, Some(m), d(2026, 1, 1)).unwrap();
    assert_eq!(apagar_motorista(&c, m).unwrap_err().codigo, "tem_historico");
    let m2 = motorista(&c, "Bruno");
    apagar_motorista(&c, m2).unwrap();
}

#[test]
fn manutencao_cria_leitura_e_atualiza_km() {
    let mut c = bd();
    let a = carro(&c, "AA-12-BB");
    let id = lancar_manutencao(&mut c, &manut(a, "2026-10-01", Some(61_000)), hoje()).unwrap();
    let car = obter_carro(&c, a, 2026).unwrap();
    assert_eq!(car.km_atual, 61_000);
    assert_eq!(car.gasto_ano, 1_000);
    assert_eq!(obter_carro(&c, a, 2025).unwrap().gasto_ano, 0);
    let (origem, origem_id): (String, i64) = c
        .query_row(
            "SELECT origem, origem_id FROM leituras_km WHERE carro_id=?1",
            [a],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((origem.as_str(), origem_id), ("manutencao", id));
}

#[test]
fn km_abaixo_pede_confirmacao() {
    let mut c = bd();
    let a = carro(&c, "AA-12-BB");
    atualizar_km(&mut c, &km(a, "2026-10-01", 70_000, false), hoje()).unwrap();
    let e = atualizar_km(&mut c, &km(a, "2026-10-10", 69_000, false), hoje()).unwrap_err();
    assert_eq!(e.codigo, "km_abaixo");
    assert!(e.mensagem.contains("70 000"));
    assert_eq!(
        e.mensagem,
        "O último KM registado era 70 000. Confirma 69 000?"
    );
    // manutenção e pneu também pedem confirmação
    let e = lancar_manutencao(&mut c, &manut(a, "2026-10-10", Some(69_000)), hoje()).unwrap_err();
    assert_eq!(e.codigo, "km_abaixo");
    let mut p = pneu(a, "2026-10-10", 100);
    p.km = Some(69_000);
    assert_eq!(
        lancar_pneu(&mut c, &p, hoje()).unwrap_err().codigo,
        "km_abaixo"
    );
    p.confirmado = true;
    lancar_pneu(&mut c, &p, hoje()).unwrap();
    atualizar_km(&mut c, &km(a, "2026-10-10", 69_000, true), hoje()).unwrap();
    // leitura com data anterior à de 70 000 não compara com ela
    atualizar_km(&mut c, &km(a, "2026-09-01", 60_000, false), hoje()).unwrap();
    assert_eq!(obter_carro(&c, a, 2026).unwrap().km_atual, 70_000);
}

#[test]
fn data_futura_recusada_mas_vencimento_futuro_ok() {
    let mut c = bd();
    let a = carro(&c, "AA-12-BB");
    let e = lancar_manutencao(&mut c, &manut(a, "2026-10-21", None), hoje()).unwrap_err();
    assert_eq!(e.codigo, "data_futura");
    assert_eq!(e.mensagem, "A data não pode ser no futuro.");
    lancar_manutencao(&mut c, &manut(a, "2026-10-20", None), hoje()).unwrap();
    assert_eq!(
        lancar_pneu(&mut c, &pneu(a, "2026-11-01", 1), hoje())
            .unwrap_err()
            .codigo,
        "data_futura"
    );
    assert_eq!(
        atualizar_km(&mut c, &km(a, "2026-11-01", 60_000, false), hoje())
            .unwrap_err()
            .codigo,
        "data_futura"
    );
    lancar_documento(&mut c, &doc(a, "IUC", 15_000, "2027-05-31", None), hoje()).unwrap();
    assert_eq!(
        lancar_documento(
            &mut c,
            &doc(a, "IUC", 15_000, "2027-05-31", Some("2026-11-01")),
            hoje()
        )
        .unwrap_err()
        .codigo,
        "data_futura"
    );
    let mut m = multa(a, None, "2026-10-21", 6_000, "empresa");
    assert_eq!(
        guardar_multa(&mut c, &m, hoje()).unwrap_err().codigo,
        "data_futura"
    );
    m.data_infracao = "2026-10-01".into();
    m.vencimento = Some("2026-12-01".into());
    guardar_multa(&mut c, &m, hoje()).unwrap();
    assert_eq!(
        lancar_manutencao(&mut c, &manut(a, "2026-13-01", None), hoje())
            .unwrap_err()
            .codigo,
        "data_invalida"
    );
}

#[test]
fn data_antes_da_aquisicao_recusada() {
    let mut c = bd();
    let mut i = carro_input("AA-12-BB");
    i.data_aquisicao = Some("2026-03-01".into());
    let a = guardar_carro(&c, &i).unwrap();
    let e = lancar_manutencao(&mut c, &manut(a, "2026-02-28", None), hoje()).unwrap_err();
    assert_eq!(e.codigo, "data_antes_aquisicao");
    assert_eq!(
        atualizar_km(&mut c, &km(a, "2026-02-01", 51_000, false), hoje())
            .unwrap_err()
            .codigo,
        "data_antes_aquisicao"
    );
    lancar_manutencao(&mut c, &manut(a, "2026-03-01", None), hoje()).unwrap();
}

#[test]
fn coparticipacao_fora_dos_limites_recusada() {
    let mut c = bd();
    let a = carro(&c, "AA-12-BB");
    let mut m = multa(a, None, "2026-10-01", 12_000, "coparticipado");
    for v in [Some(0), Some(12_000), None, Some(-1), Some(13_000)] {
        m.valor_empresa = v;
        let e = guardar_multa(&mut c, &m, hoje()).unwrap_err();
        assert_eq!(e.codigo, "coparticipacao_invalida");
        assert_eq!(
            e.mensagem,
            "A parte da empresa tem de ser maior que 0 e menor que o valor da multa."
        );
    }
    m.valor_empresa = Some(4_000);
    let id = guardar_multa(&mut c, &m, hoje()).unwrap();
    let ve = |c: &Connection, id: i64| -> i64 {
        c.query_row("SELECT valor_empresa FROM multas WHERE id=?1", [id], |r| {
            r.get(0)
        })
        .unwrap()
    };
    assert_eq!(ve(&c, id), 4_000);
    // empresa/colaborador: calculado (ignora o que vier)
    let mut e = multa(a, None, "2026-10-01", 12_000, "empresa");
    e.valor_empresa = Some(1);
    let ide = guardar_multa(&mut c, &e, hoje()).unwrap();
    assert_eq!(ve(&c, ide), 12_000);
    let mut col = multa(a, None, "2026-10-01", 12_000, "colaborador");
    col.id = Some(ide);
    assert_eq!(guardar_multa(&mut c, &col, hoje()).unwrap(), ide);
    assert_eq!(ve(&c, ide), 0);
}

#[test]
fn multa_colaborador_nao_conta_no_gasto_da_frota() {
    let mut c = bd();
    let a = carro(&c, "AA-12-BB");
    let mut m = multa(a, None, "2026-05-01", 6_000, "colaborador");
    m.pago_em = Some("2026-05-10".into());
    guardar_multa(&mut c, &m, hoje()).unwrap();
    lancar_manutencao(&mut c, &manut(a, "2026-05-01", None), hoje()).unwrap();
    let r = relatorio(
        &c,
        &FiltroRelatorio {
            de: "2026-01-01".into(),
            ate: "2026-12-31".into(),
            carro_ids: None,
            motorista_id: None,
        },
        hoje(),
    )
    .unwrap();
    assert_eq!(r.resumo.total, 1_000); // só a manutenção
    assert_eq!(r.multas.n, 1);
    assert_eq!(r.multas.total, 6_000);
    assert_eq!(r.multas.empresa, 0);
    assert_eq!(r.multas.colaboradores, 6_000);
}

#[test]
fn adiantamento_fica_por_reembolsar_ate_marcar() {
    let mut c = bd();
    let a = carro(&c, "AA-12-BB");
    let ana = motorista(&c, "Ana");
    let mut m = multa(a, Some(ana), "2026-09-01", 12_000, "coparticipado");
    m.valor_empresa = Some(4_000);
    m.empresa_adiantou = true;
    m.pago_em = Some("2026-09-10".into());
    let id = guardar_multa(&mut c, &m, hoje()).unwrap();
    let r = multas_motorista(&c, ana, hoje()).unwrap();
    assert_eq!(r.n, 1);
    assert_eq!(r.total, 12_000);
    assert_eq!(r.empresa, 4_000);
    assert_eq!(r.colaborador, 8_000);
    assert_eq!(r.por_reembolsar, 8_000);
    // o histórico e a lista de multas concordam com o resumo
    let h = historico(&c, Some(a), 10).unwrap();
    assert_eq!(h[0].estado_multa, Some(EstadoMulta::PorReembolsar));
    let l = multas_listar(&c, None, Some(ana)).unwrap();
    assert_eq!(l[0].estado_multa, EstadoMulta::PorReembolsar);
    assert_eq!(l[0].valor_colaborador, 8_000);
    marcar_reembolsado(&c, id, d(2026, 10, 1), hoje()).unwrap();
    assert_eq!(multas_motorista(&c, ana, hoje()).unwrap().por_reembolsar, 0);
    let h = historico(&c, Some(a), 10).unwrap();
    assert_eq!(h[0].estado_multa, Some(EstadoMulta::Fechada));
    // sem adiantamento não há nada por reembolsar
    let mut sem = multa(a, Some(ana), "2026-09-02", 5_000, "colaborador");
    sem.empresa_adiantou = false;
    guardar_multa(&mut c, &sem, hoje()).unwrap();
    assert_eq!(multas_motorista(&c, ana, hoje()).unwrap().por_reembolsar, 0);
}

#[test]
fn estado_da_multa_nos_quatro_casos() {
    let mut c = bd();
    let a = carro(&c, "AA-12-BB");
    let ana = motorista(&c, "Ana");
    // sem condutor
    let m = multa(a, None, "2026-09-01", 6_000, "empresa");
    let sem_condutor = guardar_multa(&mut c, &m, hoje()).unwrap();
    // com condutor, por pagar
    let m = multa(a, Some(ana), "2026-09-02", 6_000, "empresa");
    let por_pagar = guardar_multa(&mut c, &m, hoje()).unwrap();
    // paga pela empresa por conta do colaborador, sem reembolso
    let mut m = multa(a, Some(ana), "2026-09-03", 6_000, "colaborador");
    m.pago_em = Some("2026-09-10".into());
    m.empresa_adiantou = true;
    let por_reembolsar = guardar_multa(&mut c, &m, hoje()).unwrap();
    // paga e sem pendências
    let mut m = multa(a, Some(ana), "2026-09-04", 6_000, "empresa");
    m.pago_em = Some("2026-09-10".into());
    let fechada = guardar_multa(&mut c, &m, hoje()).unwrap();

    let esperado = [
        (sem_condutor, EstadoMulta::PorIdentificarCondutor),
        (por_pagar, EstadoMulta::PorPagar),
        (por_reembolsar, EstadoMulta::PorReembolsar),
        (fechada, EstadoMulta::Fechada),
    ];
    let lista = multas_listar(&c, Some(a), None).unwrap();
    let hist = historico(&c, Some(a), 50).unwrap();
    for (id, estado) in esperado {
        assert_eq!(
            lista.iter().find(|x| x.id == id).unwrap().estado_multa,
            estado
        );
        let h = hist
            .iter()
            .find(|x| x.tipo == "multa" && x.id == id)
            .unwrap();
        assert_eq!(h.estado_multa, Some(estado));
    }
    // o filtro por motorista só traz as multas dele; a por identificar não aparece
    assert_eq!(multas_listar(&c, None, Some(ana)).unwrap().len(), 3);
    // a métrica "por reembolsar" concorda com as linhas
    let somatorio: i64 = multas_listar(&c, None, Some(ana))
        .unwrap()
        .iter()
        .filter(|x| x.estado_multa == EstadoMulta::PorReembolsar)
        .map(|x| x.valor_colaborador)
        .sum();
    assert_eq!(
        multas_motorista(&c, ana, hoje()).unwrap().por_reembolsar,
        somatorio
    );
    assert_eq!(somatorio, 6_000);
}

#[test]
fn alerta_da_carta_so_para_motoristas_ativos() {
    let c = bd();
    let mk = |nome: &str, validade: &str, ativo: bool| {
        guardar_motorista(
            &c,
            &MotoristaInput {
                id: None,
                nome: nome.into(),
                telefone: None,
                carta_numero: None,
                carta_validade: Some(validade.into()),
                ativo,
                notas: None,
            },
        )
        .unwrap()
    };
    mk("Vencida", "2026-10-19", true);
    mk("Proxima", "2026-12-19", true); // 60 dias
    mk("Longe", "2026-12-20", true); // 61 dias
    mk("Inativa", "2026-10-01", false);
    let v = listar_motoristas(&c, false, hoje()).unwrap();
    let nivel = |n: &str| v.iter().find(|m| m.nome == n).unwrap().alerta_carta;
    assert_eq!(nivel("Vencida"), Some(Nivel::Vencido));
    assert_eq!(nivel("Proxima"), Some(Nivel::Proximo));
    assert_eq!(nivel("Longe"), None);
    assert_eq!(nivel("Inativa"), None);
}

#[test]
fn carro_traz_gasto_total() {
    let mut c = bd();
    let a = carro(&c, "AA-12-BB");
    lancar_manutencao(&mut c, &manut(a, "2025-03-01", None), hoje()).unwrap();
    lancar_manutencao(&mut c, &manut(a, "2026-03-01", None), hoje()).unwrap();
    let r = obter_carro(&c, a, 2026).unwrap();
    assert_eq!(r.gasto_total, 2 * r.gasto_ano);
    assert!(r.gasto_ano > 0);
}

#[test]
fn condutor_sugerido_pela_atribuicao() {
    let mut c = bd();
    let a = carro(&c, "AA-12-BB");
    let ana = motorista(&c, "Ana");
    let bruno = motorista(&c, "Bruno");
    atribuir(&mut c, a, Some(ana), d(2026, 1, 1)).unwrap();
    atribuir(&mut c, a, Some(bruno), d(2026, 6, 1)).unwrap();
    assert_eq!(condutor_sugerido(&c, a, d(2025, 12, 1)).unwrap(), None);
    assert_eq!(condutor_sugerido(&c, a, d(2026, 3, 1)).unwrap(), Some(ana));
    assert_eq!(
        condutor_sugerido(&c, a, d(2026, 6, 1)).unwrap(),
        Some(bruno)
    );
    assert_eq!(
        condutor_sugerido(&c, a, d(2026, 7, 1)).unwrap(),
        Some(bruno)
    );
    // identificar condutor numa multa por identificar
    let id = guardar_multa(
        &mut c,
        &multa(a, None, "2026-07-01", 6_000, "empresa"),
        hoje(),
    )
    .unwrap();
    identificar_condutor(&c, id, bruno, d(2026, 7, 5), hoje()).unwrap();
    let (mot, em): (i64, String) = c
        .query_row(
            "SELECT motorista_id, condutor_identificado_em FROM multas WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((mot, em.as_str()), (bruno, "2026-07-05"));
}

#[test]
fn pontos_so_dos_ultimos_3_anos() {
    let mut c = bd();
    let a = carro(&c, "AA-12-BB");
    let ana = motorista(&c, "Ana");
    for (data, pontos) in [("2023-10-19", 4), ("2023-10-20", 2), ("2026-01-01", 3)] {
        let mut m = multa(a, Some(ana), data, 1_000, "empresa");
        m.pontos = pontos;
        guardar_multa(&mut c, &m, hoje()).unwrap();
    }
    let r = multas_motorista(&c, ana, hoje()).unwrap();
    assert_eq!(r.n, 3);
    assert_eq!(r.pontos_3_anos, 5); // 2 + 3; a de 2023-10-19 já passou dos 3 anos
}

#[test]
fn oficina_obrigatoria_e_soma_pecas_mao_de_obra() {
    let mut c = bd();
    let a = carro(&c, "AA-12-BB");
    let mut m = manut(a, "2026-10-01", None);
    m.executada_por = "oficina".into();
    m.oficina = Some("  ".into());
    assert_eq!(
        lancar_manutencao(&mut c, &m, hoje()).unwrap_err().codigo,
        "falta_oficina"
    );
    m.oficina = Some(" Auto Silva ".into());
    m.valor_pecas = Some(5_000);
    m.valor_mao_obra = Some(3_000);
    m.valor = 9_000;
    assert_eq!(
        lancar_manutencao(&mut c, &m, hoje()).unwrap_err().codigo,
        "valor_nao_bate"
    );
    m.valor = 8_000;
    lancar_manutencao(&mut c, &m, hoje()).unwrap();
    let mut p = manut(a, "2026-10-01", None);
    p.valor_pecas = Some(500);
    p.valor_mao_obra = Some(500);
    assert_eq!(
        lancar_manutencao(&mut c, &p, hoje()).unwrap_err().codigo,
        "mao_obra_propria"
    );
    let mut neg = manut(a, "2026-10-01", None);
    neg.valor = -1;
    assert_eq!(
        lancar_manutencao(&mut c, &neg, hoje()).unwrap_err().codigo,
        "valor_invalido"
    );
    let mut m2 = m.clone();
    m2.oficina = Some("Pneus Lda".into());
    lancar_manutencao(&mut c, &m2, hoje()).unwrap();
    lancar_manutencao(&mut c, &m, hoje()).unwrap();
    assert_eq!(
        oficinas_usadas(&c).unwrap(),
        vec!["Auto Silva", "Pneus Lda"]
    );
}

#[test]
fn pagar_iuc_anual_cria_o_seguinte() {
    let mut c = bd();
    let a = carro(&c, "AA-12-BB");
    let mut i = doc(a, "IUC", 15_000, "2026-05-31", None);
    i.anual = true;
    i.descricao = Some("IUC 2026".into());
    let id = lancar_documento(&mut c, &i, hoje()).unwrap();
    let novo = pagar_documento(&mut c, id, "2026-05-20", hoje())
        .unwrap()
        .unwrap();
    let (venc, pago, gerado, valor, tipo): (String, Option<String>, i64, i64, String) = c
        .query_row(
            "SELECT vencimento, pago_em, gerado_de_id, valor, tipo FROM documentos WHERE id=?1",
            [novo],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .unwrap();
    assert_eq!(venc, "2027-05-31");
    assert_eq!(pago, None);
    assert_eq!(gerado, id);
    assert_eq!(valor, 15_000);
    assert_eq!(tipo, "IUC");
    // pagar outra vez é recusado (e não duplica)
    let e = pagar_documento(&mut c, id, "2026-05-21", hoje()).unwrap_err();
    assert_eq!(e.codigo, "ja_pago");
    assert_eq!(e.mensagem, "Este documento já está pago.");
    // não anual: nada
    let n = lancar_documento(
        &mut c,
        &doc(a, "seguro", 30_000, "2026-11-01", None),
        hoje(),
    )
    .unwrap();
    // data futura recusada
    assert_eq!(
        pagar_documento(&mut c, n, "2026-10-21", hoje())
            .unwrap_err()
            .codigo,
        "data_futura"
    );
    assert_eq!(
        pagar_documento(&mut c, n, "2026-10-01", hoje()).unwrap(),
        None
    );
    assert_eq!(
        pagar_documento(&mut c, 999, "2026-10-01", hoje())
            .unwrap_err()
            .codigo,
        "nao_encontrado"
    );
    assert_eq!(contar(&c, "SELECT COUNT(*) FROM documentos"), 3);
}

#[test]
fn pagar_anual_de_carro_vendido_nao_gera_seguinte() {
    let mut c = bd();
    let a = carro(&c, "AA-12-BB");
    let mut i = doc(a, "IUC", 15_000, "2026-05-31", None);
    i.anual = true;
    let id = lancar_documento(&mut c, &i, hoje()).unwrap();
    let mut v = carro_input("AA-12-BB");
    v.id = Some(a);
    v.estado = "vendido".into();
    guardar_carro(&c, &v).unwrap();
    assert_eq!(
        pagar_documento(&mut c, id, "2026-05-20", hoje()).unwrap(),
        None
    );
    // lançar já pago num carro inativo também não gera
    v.estado = "inativo".into();
    guardar_carro(&c, &v).unwrap();
    let mut j = doc(a, "seguro", 30_000, "2026-06-30", Some("2026-06-01"));
    j.anual = true;
    lancar_documento(&mut c, &j, hoje()).unwrap();
    assert_eq!(contar(&c, "SELECT COUNT(*) FROM documentos"), 2);
    // de volta a ativo: lançar já pago gera o seguinte
    v.estado = "ativo".into();
    guardar_carro(&c, &v).unwrap();
    lancar_documento(&mut c, &j, hoje()).unwrap();
    assert_eq!(contar(&c, "SELECT COUNT(*) FROM documentos"), 4);
}

#[test]
fn pagar_iuc_anual_nao_poe_o_seguinte_no_topo_dos_recentes() {
    // `criado_em` vem do relógio do SQLite (UTC): usar esse dia como "hoje".
    let mut c = bd();
    let hoje: NaiveDate = {
        let s: String = c.query_row("SELECT date('now')", [], |r| r.get(0)).unwrap();
        NaiveDate::parse_from_str(&s, "%Y-%m-%d").unwrap()
    };
    let a = carro(&c, "AA-12-BB");
    let mut i = doc(a, "IUC", 15_000, "2026-05-31", None);
    i.anual = true;
    let id = lancar_documento(&mut c, &i, hoje).unwrap();
    let novo = pagar_documento(&mut c, id, "2026-05-20", hoje)
        .unwrap()
        .unwrap();
    let m = lancar_manutencao(&mut c, &manut(a, &hoje.to_string(), None), hoje).unwrap();
    let r = historico(&c, None, 10).unwrap();
    assert_eq!((r[0].tipo.as_str(), r[0].id), ("manutencao", m));
    let gerado = r
        .iter()
        .find(|x| x.tipo == "documento" && x.id == novo)
        .unwrap();
    assert_eq!(gerado.data, hoje.to_string()); // data de registo
    assert_eq!(gerado.detalhe, None);
    assert!(gerado.por_pagar);
    assert_eq!(gerado.vencimento.as_deref(), Some("2027-05-31"));
    let pago = r
        .iter()
        .find(|x| x.tipo == "documento" && x.id == id)
        .unwrap();
    assert_eq!(pago.data, "2026-05-20");
    assert_eq!(pago.detalhe, None);
    assert_eq!(painel(&c, hoje).unwrap().recentes[0].tipo, "manutencao");
}

#[test]
fn multa_recusa_datas_futuras_e_pagamento_antes_da_infracao() {
    let mut c = bd();
    let a = carro(&c, "AA-12-BB");
    let ana = motorista(&c, "Ana");
    let base = multa(a, Some(ana), "2026-09-01", 6_000, "empresa");
    type Campo = fn(&mut MultaInput) -> &mut Option<String>;
    let campos: [Campo; 4] = [
        |m| &mut m.pago_em,
        |m| &mut m.reembolsado_em,
        |m| &mut m.condutor_identificado_em,
        |m| &mut m.data_notificacao,
    ];
    for campo in campos {
        let mut m = base.clone();
        *campo(&mut m) = Some("2026-10-21".into());
        assert_eq!(
            guardar_multa(&mut c, &m, hoje()).unwrap_err().codigo,
            "data_futura"
        );
        *campo(&mut m) = Some("2026-10-20".into());
        guardar_multa(&mut c, &m, hoje()).unwrap();
    }
    let mut m = base.clone();
    m.pago_em = Some("2026-08-31".into());
    let e = guardar_multa(&mut c, &m, hoje()).unwrap_err();
    assert_eq!(e.codigo, "data_invalida");
    assert_eq!(e.mensagem, "O pagamento não pode ser anterior à infração.");
    m.pago_em = Some("2026-09-01".into());
    let id = guardar_multa(&mut c, &m, hoje()).unwrap();
    assert_eq!(
        marcar_reembolsado(&c, id, d(2026, 10, 21), hoje())
            .unwrap_err()
            .codigo,
        "data_futura"
    );
    marcar_reembolsado(&c, id, d(2026, 10, 20), hoje()).unwrap();
    assert_eq!(
        identificar_condutor(&c, id, ana, d(2026, 10, 21), hoje())
            .unwrap_err()
            .codigo,
        "data_futura"
    );
    identificar_condutor(&c, id, ana, d(2026, 10, 20), hoje()).unwrap();
}

#[test]
fn referencias_inexistentes_e_data_de_saida() {
    let mut c = bd();
    let a = carro(&c, "AA-12-BB");
    let ref_inv = |e: crate::erro::Erro| {
        assert_eq!(e.codigo, "referencia_invalida");
        assert_eq!(e.mensagem, "O registo escolhido já não existe.");
    };
    let mut m = manut(a, "2026-10-01", None);
    m.tipo_id = 999;
    ref_inv(lancar_manutencao(&mut c, &m, hoje()).unwrap_err());
    ref_inv(lancar_manutencao(&mut c, &manut(999, "2026-10-01", None), hoje()).unwrap_err());
    ref_inv(lancar_pneu(&mut c, &pneu(999, "2026-10-01", 1), hoje()).unwrap_err());
    ref_inv(lancar_documento(&mut c, &doc(999, "IUC", 1, "2026-10-01", None), hoje()).unwrap_err());
    ref_inv(atualizar_km(&mut c, &km(999, "2026-10-01", 1, false), hoje()).unwrap_err());
    ref_inv(
        guardar_multa(
            &mut c,
            &multa(a, Some(999), "2026-10-01", 1, "empresa"),
            hoje(),
        )
        .unwrap_err(),
    );
    let id = guardar_multa(&mut c, &multa(a, None, "2026-10-01", 1, "empresa"), hoje()).unwrap();
    ref_inv(identificar_condutor(&c, id, 999, d(2026, 10, 2), hoje()).unwrap_err());
    ref_inv(atribuir(&mut c, a, Some(999), d(2026, 1, 1)).unwrap_err());
    ref_inv(atribuir(&mut c, 999, None, d(2026, 1, 1)).unwrap_err());
    let mut p = PlanoInput {
        id: None,
        tipo_id: 999,
        cada_km: Some(1_000),
        cada_meses: None,
        todos_carros: true,
        carros: vec![],
    };
    ref_inv(guardar_plano(&mut c, &p).unwrap_err());
    p.tipo_id = 1;
    p.todos_carros = false;
    p.carros = vec![999];
    ref_inv(guardar_plano(&mut c, &p).unwrap_err());
    // data de saída antes da aquisição
    let mut i = carro_input("CC-34-DD");
    i.data_aquisicao = Some("2026-03-01".into());
    i.data_saida = Some("2026-02-28".into());
    assert_eq!(
        guardar_carro(&c, &i).unwrap_err().codigo,
        "data_saida_invalida"
    );
    i.data_saida = Some("2026-03-01".into());
    guardar_carro(&c, &i).unwrap();
}

#[test]
fn apagar_manutencao_remove_a_leitura() {
    let mut c = bd();
    let a = carro(&c, "AA-12-BB");
    let id = lancar_manutencao(&mut c, &manut(a, "2026-10-01", Some(61_000)), hoje()).unwrap();
    apagar_lancamento(&mut c, "manutencao", id).unwrap();
    assert_eq!(obter_carro(&c, a, 2026).unwrap().km_atual, 50_000);
    assert_eq!(contar(&c, "SELECT COUNT(*) FROM leituras_km"), 0);
    assert_eq!(contar(&c, "SELECT COUNT(*) FROM manutencoes"), 0);
    assert_eq!(
        apagar_lancamento(&mut c, "xpto", 1).unwrap_err().codigo,
        "tipo_invalido"
    );
    // "km" só apaga leituras manuais
    let p = {
        let mut p = pneu(a, "2026-10-01", 100);
        p.km = Some(62_000);
        lancar_pneu(&mut c, &p, hoje()).unwrap()
    };
    let leitura_pneu: i64 = c
        .query_row("SELECT id FROM leituras_km WHERE origem='pneu'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert!(apagar_lancamento(&mut c, "km", leitura_pneu).is_err());
    apagar_lancamento(&mut c, "pneu", p).unwrap();
    assert_eq!(contar(&c, "SELECT COUNT(*) FROM leituras_km"), 0);
    let l = atualizar_km(&mut c, &km(a, "2026-10-02", 63_000, false), hoje()).unwrap();
    apagar_lancamento(&mut c, "km", l).unwrap();
    assert_eq!(contar(&c, "SELECT COUNT(*) FROM leituras_km"), 0);
}

#[test]
fn atribuir_fecha_a_anterior() {
    let mut c = bd();
    let c1 = carro(&c, "AA-12-BB");
    let c2 = carro(&c, "CC-34-DD");
    let a = motorista(&c, "Ana");
    let b = motorista(&c, "Bruno");
    atribuir(&mut c, c1, Some(a), d(2026, 1, 1)).unwrap();
    atribuir(&mut c, c1, Some(b), d(2026, 4, 1)).unwrap();
    let v = atribuicoes_de(
        &c,
        &FiltroAtribuicoes {
            carro_id: Some(c1),
            motorista_id: None,
        },
    )
    .unwrap();
    assert_eq!(v.len(), 2);
    let va = v.iter().find(|x| x.motorista_id == a).unwrap();
    let vb = v.iter().find(|x| x.motorista_id == b).unwrap();
    assert_eq!(va.ate.as_deref(), Some("2026-04-01"));
    assert_eq!(vb.de, "2026-04-01");
    assert_eq!(vb.ate, None);
    assert_eq!(
        obter_carro(&c, c1, 2026).unwrap().motorista_atual,
        Some((b, "Bruno".to_string()))
    );
    // data antes da atribuição aberta
    assert_eq!(
        atribuir(&mut c, c1, Some(a), d(2026, 3, 1))
            .unwrap_err()
            .codigo,
        "data_invalida"
    );
    // B passa para o carro 2: fecha a do carro 1
    atribuir(&mut c, c2, Some(b), d(2026, 5, 1)).unwrap();
    assert_eq!(obter_carro(&c, c1, 2026).unwrap().motorista_atual, None);
    let vb = atribuicoes_de(
        &c,
        &FiltroAtribuicoes {
            carro_id: None,
            motorista_id: Some(b),
        },
    )
    .unwrap();
    assert_eq!(vb.len(), 2);
    assert_eq!(vb.iter().filter(|x| x.ate.is_none()).count(), 1);
    // None só fecha
    atribuir(&mut c, c2, None, d(2026, 6, 1)).unwrap();
    assert_eq!(obter_carro(&c, c2, 2026).unwrap().motorista_atual, None);
    assert_eq!(
        contar(&c, "SELECT COUNT(*) FROM atribuicoes WHERE ate IS NULL"),
        0
    );
    // não pode sobrepor uma atribuição já fechada
    assert_eq!(
        atribuir(&mut c, c2, Some(a), d(2026, 5, 15))
            .unwrap_err()
            .codigo,
        "data_invalida"
    );
}

/// Cenário: 2 carros, 2 motoristas; ano 2026.
/// - Ana no carro 1 desde 2026-01-01; Bruno no carro 2 desde 2026-03-01.
/// - Carro 1: manutenção oficina "Auto Silva" 2026-02-10 (peças 5 000 + mão 3 000 = 8 000) -> Ana.
/// - Carro 2: manutenção própria 2026-02-15, Travões, 2 000 só total -> Sem motorista.
/// - Carro 2: pneus 2026-04-01, 30 000 -> Bruno.
/// - Carro 1: pneus 2025-12-01, 10 000 -> fora do período.
/// - Carro 1: IUC 15 000 pago 2026-05-20 -> Ana. Carro 2: IUC 15 000 por pagar -> por_pagar.
/// - Carro 2: multa velocidade 2026-07-01, condutora Ana (não Bruno!), coparticipada 12 000 /
///   empresa 4 000, adiantada, paga 2026-07-10, 3 pontos -> Ana 4 000.
/// - Carro 1: multa estacionamento 2026-08-01, Ana, colaborador 6 000, paga 2026-08-05 -> 0.
///
/// Totais: 8 000 + 2 000 + 30 000 + 15 000 + 4 000 + 0 = 59 000.
/// Por carro: c2 = 2 000 + 30 000 + 4 000 = 36 000; c1 = 8 000 + 15 000 + 0 = 23 000.
/// Por motorista: Bruno 30 000; Ana 8 000 + 15 000 + 4 000 = 27 000; Sem motorista 2 000.
/// Multas: n 2, total 18 000, empresa 4 000, colaboradores 8 000 + 6 000 = 14 000,
/// por reembolsar 8 000.
#[test]
fn relatorio_ano_por_categoria_e_motorista() {
    let mut c = bd();
    let c1 = carro(&c, "AA-11-AA");
    let c2 = carro(&c, "BB-22-BB");
    let ana = motorista(&c, "Ana");
    let bruno = motorista(&c, "Bruno");
    atribuir(&mut c, c1, Some(ana), d(2026, 1, 1)).unwrap();
    atribuir(&mut c, c2, Some(bruno), d(2026, 3, 1)).unwrap();

    let mut m1 = manut(c1, "2026-02-10", None);
    m1.executada_por = "oficina".into();
    m1.oficina = Some("Auto Silva".into());
    m1.valor = 8_000;
    m1.valor_pecas = Some(5_000);
    m1.valor_mao_obra = Some(3_000);
    lancar_manutencao(&mut c, &m1, hoje()).unwrap();
    let mut m2 = manut(c2, "2026-02-15", None);
    m2.tipo_id = 3;
    m2.valor = 2_000;
    lancar_manutencao(&mut c, &m2, hoje()).unwrap();
    lancar_pneu(&mut c, &pneu(c2, "2026-04-01", 30_000), hoje()).unwrap();
    lancar_pneu(&mut c, &pneu(c1, "2025-12-01", 10_000), hoje()).unwrap();
    lancar_documento(
        &mut c,
        &doc(c1, "IUC", 15_000, "2026-05-31", Some("2026-05-20")),
        hoje(),
    )
    .unwrap();
    lancar_documento(&mut c, &doc(c2, "IUC", 15_000, "2026-11-10", None), hoje()).unwrap();
    let mut mu1 = multa(c2, Some(ana), "2026-07-01", 12_000, "coparticipado");
    mu1.valor_empresa = Some(4_000);
    mu1.empresa_adiantou = true;
    mu1.pago_em = Some("2026-07-10".into());
    mu1.pontos = 3;
    guardar_multa(&mut c, &mu1, hoje()).unwrap();
    let mut mu2 = multa(c1, Some(ana), "2026-08-01", 6_000, "colaborador");
    mu2.infracao = "estacionamento".into();
    mu2.pago_em = Some("2026-08-05".into());
    guardar_multa(&mut c, &mu2, hoje()).unwrap();

    let filtro = |mot: Option<i64>| FiltroRelatorio {
        de: "2026-01-01".into(),
        ate: "2026-12-31".into(),
        carro_ids: None,
        motorista_id: mot,
    };
    let r = relatorio(&c, &filtro(None), hoje()).unwrap();
    assert_eq!(r.resumo.total, 59_000);
    assert_eq!(r.resumo.por_pagar, 15_000);
    assert_eq!(r.resumo.por_pagar_vencido, 0);
    assert_eq!(r.resumo.por_pagar_proximo, 15_000);
    assert_eq!(r.resumo.por_pagar_dias, 30);
    assert_eq!(r.resumo.por_categoria[&Categoria::Manutencao], 10_000);
    assert_eq!(r.resumo.por_categoria[&Categoria::Pneus], 30_000);
    assert_eq!(r.resumo.por_categoria[&Categoria::Iuc], 15_000);
    assert_eq!(r.resumo.por_categoria[&Categoria::Multa], 4_000);
    assert_eq!(r.resumo.por_carro, vec![(c2, 36_000), (c1, 23_000)]);
    assert_eq!(
        r.por_motorista,
        vec![
            (Some(bruno), "Bruno".to_string(), 30_000),
            (Some(ana), "Ana".to_string(), 27_000),
            (None, SEM_MOTORISTA.to_string(), 2_000),
        ]
    );
    assert_eq!(
        r.por_tipo_manutencao,
        vec![
            ("Mudança de óleo".to_string(), 8_000),
            ("Travões".to_string(), 2_000)
        ]
    );
    assert_eq!(r.manutencao.interna, 2_000);
    assert_eq!(r.manutencao.oficina, 8_000);
    assert_eq!(r.manutencao.pecas, 5_000);
    assert_eq!(r.manutencao.mao_obra, 3_000);
    assert_eq!(r.manutencao.nao_discriminado, 2_000);
    assert_eq!(
        r.manutencao.por_oficina,
        vec![("Auto Silva".to_string(), 8_000)]
    );
    assert_eq!(r.multas.n, 2);
    assert_eq!(r.multas.total, 18_000);
    assert_eq!(r.multas.empresa, 4_000);
    assert_eq!(r.multas.colaboradores, 14_000);
    assert_eq!(r.multas.por_reembolsar, 8_000);
    assert_eq!(
        r.multas.por_infracao,
        vec![
            ("velocidade".to_string(), 1, 12_000),
            ("estacionamento".to_string(), 1, 6_000)
        ]
    );
    assert_eq!(
        r.multas.por_motorista,
        vec![(Some(ana), "Ana".to_string(), 2, 3)]
    );

    // Filtro por motorista: Bruno = só os pneus de 30 000.
    let rb = relatorio(&c, &filtro(Some(bruno)), hoje()).unwrap();
    assert_eq!(rb.resumo.total, 30_000);
    assert_eq!(rb.resumo.por_categoria.len(), 1);
    assert_eq!(rb.multas.n, 0);
    assert_eq!(rb.manutencao.interna + rb.manutencao.oficina, 0);
    // Ana = 8 000 + 15 000 + 4 000 = 27 000 (a multa no carro 2 conta para ela).
    let ra = relatorio(&c, &filtro(Some(ana)), hoje()).unwrap();
    assert_eq!(ra.resumo.total, 27_000);
    assert_eq!(ra.manutencao.oficina, 8_000);
    assert_eq!(ra.multas.n, 2);

    // Filtro por carro: só o carro 1 -> 8 000 + 15 000 = 23 000.
    let mut f = filtro(None);
    f.carro_ids = Some(vec![c1]);
    let r1 = relatorio(&c, &f, hoje()).unwrap();
    assert_eq!(r1.resumo.total, 23_000);
    assert_eq!(r1.resumo.por_pagar, 0);
    assert_eq!(r1.multas.n, 1);

    // gasto_ano do carro
    assert_eq!(obter_carro(&c, c2, 2026).unwrap().gasto_ano, 36_000);
    assert_eq!(obter_carro(&c, c1, 2025).unwrap().gasto_ano, 10_000);
}

#[test]
fn painel_conta_alertas_de_carros_ativos_apenas() {
    let mut c = bd();
    let ativo = carro(&c, "AA-12-BB");
    let vendido = carro(&c, "CC-34-DD");
    atualizar_km(&mut c, &km(ativo, "2026-10-01", 66_000, false), hoje()).unwrap();
    atualizar_km(&mut c, &km(vendido, "2026-10-01", 70_000, false), hoje()).unwrap();
    let mut v = carro_input("CC-34-DD");
    v.id = Some(vendido);
    v.estado = "vendido".into();
    v.data_saida = Some("2026-10-05".into());
    guardar_carro(&c, &v).unwrap();
    guardar_plano(
        &mut c,
        &PlanoInput {
            id: None,
            tipo_id: 1,
            cada_km: Some(15_000),
            cada_meses: None,
            todos_carros: true,
            carros: vec![],
        },
    )
    .unwrap();
    // documento por pagar do carro vendido: nem alerta nem "por pagar"
    lancar_documento(
        &mut c,
        &doc(vendido, "seguro", 1_000, "2026-10-25", None),
        hoje(),
    )
    .unwrap();
    // por pagar do carro ativo, longe do vencimento (sem alerta)
    lancar_documento(
        &mut c,
        &doc(ativo, "seguro", 2_000, "2027-06-01", None),
        hoje(),
    )
    .unwrap();
    lancar_manutencao(&mut c, &manut(ativo, "2026-10-02", None), hoje()).unwrap();
    // gasto do vendido em outubro (pneus não mexem nos planos); o vendido não tem
    // manutenção nenhuma: o plano estaria vencido (70 000 >= 50 000 + 15 000) se contasse.
    lancar_pneu(&mut c, &pneu(vendido, "2026-10-03", 1_000), hoje()).unwrap();

    let p = painel(&c, hoje()).unwrap();
    assert_eq!(p.carros_ativos, 1);
    assert_eq!(
        p.alertas.len(),
        0,
        "a manutenção do tipo 1 zera o alerta do ativo; o vendido fica de fora"
    );

    // apagar a manutenção do carro ativo -> volta o alerta (base = km_inicial 50 000)
    let id: i64 = c
        .query_row(
            "SELECT id FROM manutencoes WHERE carro_id=?1",
            [ativo],
            |r| r.get(0),
        )
        .unwrap();
    apagar_lancamento(&mut c, "manutencao", id).unwrap();
    let p = painel(&c, hoje()).unwrap();
    assert_eq!(p.alertas.len(), 1);
    assert_eq!(p.alertas[0].tipo, TipoAlerta::Manutencao);
    assert_eq!(p.alertas[0].carro_id, Some(ativo));
    assert_eq!(p.alertas[0].falta_km, Some(-1_000));
    assert_eq!(p.alertas[0].matricula.as_deref(), Some("AA-12-BB"));
    assert_eq!(p.alertas[0].motivo, Some(Motivo::Km));
    assert_eq!((p.ano, p.mes), (2026, 10));
    assert_eq!(p.gasto_mes, 1_000); // pneus do vendido em outubro
    assert_eq!(p.gasto_ano, 1_000);
    // o documento do ativo vence daqui a mais de 30 dias e o do vendido não conta
    assert_eq!(p.por_pagar, 0);
    assert_eq!(p.recentes.len(), 5); // 2 KM + 2 documentos + 1 pneu
    assert_eq!(listar_carros(&c, true, 2026).unwrap().len(), 1);
    assert_eq!(listar_carros(&c, false, 2026).unwrap().len(), 2);
}

#[test]
fn limiares_tipos_e_planos() {
    let mut c = bd();
    assert_eq!(
        obter_limiares(&c).unwrap(),
        Limiares {
            km: 1_000,
            dias: 30
        }
    );
    guardar_limiares(&c, &Limiares { km: 500, dias: 15 }).unwrap();
    assert_eq!(obter_limiares(&c).unwrap(), Limiares { km: 500, dias: 15 });
    assert_eq!(listar_tipos(&c).unwrap().len(), 11);
    assert_eq!(
        guardar_tipo(&c, None, "Filtros").unwrap_err().codigo,
        "tipo_duplicado"
    );
    let t = guardar_tipo(&c, None, " Lavagem ").unwrap();
    let a = carro(&c, "AA-12-BB");
    let mut pi = PlanoInput {
        id: None,
        tipo_id: t,
        cada_km: None,
        cada_meses: None,
        todos_carros: false,
        carros: vec![a],
    };
    assert_eq!(
        guardar_plano(&mut c, &pi).unwrap_err().codigo,
        "plano_vazio"
    );
    pi.cada_meses = Some(6);
    let id = guardar_plano(&mut c, &pi).unwrap();
    let ps = listar_planos(&c).unwrap();
    assert_eq!(ps.len(), 1);
    assert_eq!(ps[0].tipo_nome, "Lavagem");
    assert_eq!(ps[0].carros, vec![a]);
    apagar_plano(&mut c, id).unwrap();
    assert!(listar_planos(&c).unwrap().is_empty());
}

#[test]
fn historico_ordenado_por_data() {
    let mut c = bd();
    let a = carro(&c, "AA-12-BB");
    lancar_manutencao(&mut c, &manut(a, "2026-03-01", Some(55_000)), hoje()).unwrap();
    lancar_pneu(&mut c, &pneu(a, "2026-05-01", 100), hoje()).unwrap();
    atualizar_km(&mut c, &km(a, "2026-04-01", 56_000, false), hoje()).unwrap();
    let h = historico(&c, Some(a), 10).unwrap();
    let tipos: Vec<_> = h.iter().map(|x| x.tipo.as_str()).collect();
    assert_eq!(tipos, vec!["pneu", "km", "manutencao"]);
    assert_eq!(h[2].titulo, "Mudança de óleo");
    assert_eq!(h[2].km, Some(55_000));
    assert_eq!(historico(&c, Some(a), 2).unwrap().len(), 2);
    let json = serde_json::to_string(&h[0]).unwrap();
    assert!(json.contains("\"carroId\""));
}

#[test]
fn erros_de_data_indicam_o_campo() {
    let mut c = bd();
    let a = carro(&c, "AA-12-BB");
    let base = multa(a, None, "2026-09-01", 6_000, "empresa");

    // pago_em anterior à infração
    let mut m = base.clone();
    m.pago_em = Some("2026-08-31".into());
    let e = guardar_multa(&mut c, &m, hoje()).unwrap_err();
    assert_eq!(e.codigo, "data_invalida");
    assert_eq!(e.campo.as_deref(), Some("pagoEm"));

    // pago_em no futuro
    m.pago_em = Some("2026-10-21".into());
    let e = guardar_multa(&mut c, &m, hoje()).unwrap_err();
    assert_eq!(e.codigo, "data_futura");
    assert_eq!(e.campo.as_deref(), Some("pagoEm"));

    // cada data da multa indica o seu campo
    type Campo = fn(&mut MultaInput) -> &mut Option<String>;
    let campos: [(Campo, &str); 4] = [
        (|m| &mut m.data_notificacao, "dataNotificacao"),
        (
            |m| &mut m.condutor_identificado_em,
            "condutorIdentificadoEm",
        ),
        (|m| &mut m.reembolsado_em, "reembolsadoEm"),
        (|m| &mut m.vencimento, "vencimento"),
    ];
    for (campo, nome) in campos {
        let mut m = base.clone();
        *campo(&mut m) = Some("2026-13-01".into());
        let e = guardar_multa(&mut c, &m, hoje()).unwrap_err();
        assert_eq!(e.codigo, "data_invalida");
        assert_eq!(e.campo.as_deref(), Some(nome));
    }
    let mut m = base.clone();
    m.data_infracao = "2026-10-21".into();
    let e = guardar_multa(&mut c, &m, hoje()).unwrap_err();
    assert_eq!(
        (e.codigo.as_str(), e.campo.as_deref()),
        ("data_futura", Some("dataInfracao"))
    );

    // documento: vencimento e pago_em
    let e = lancar_documento(&mut c, &doc(a, "IUC", 100, "2026-13-01", None), hoje()).unwrap_err();
    assert_eq!(e.campo.as_deref(), Some("vencimento"));
    let e = lancar_documento(
        &mut c,
        &doc(a, "IUC", 100, "2027-01-01", Some("2026-10-21")),
        hoje(),
    )
    .unwrap_err();
    assert_eq!(
        (e.codigo.as_str(), e.campo.as_deref()),
        ("data_futura", Some("pagoEm"))
    );

    // manutenção: qual dos valores; referência inválida não ganha campo
    let mut mt = manut(a, "2026-10-01", None);
    mt.valor_pecas = Some(-1);
    let e = lancar_manutencao(&mut c, &mt, hoje()).unwrap_err();
    assert_eq!(
        (e.codigo.as_str(), e.campo.as_deref()),
        ("valor_invalido", Some("valorPecas"))
    );
    let mut m = base.clone();
    m.carro_id = 999;
    let e = guardar_multa(&mut c, &m, hoje()).unwrap_err();
    assert_eq!((e.codigo.as_str(), e.campo), ("referencia_invalida", None));
}

#[test]
fn erro_sem_campo_nao_serializa_campo() {
    let e = crate::repositorio::erro("so_consulta", "Só consulta.");
    assert_eq!(
        serde_json::to_string(&e).unwrap(),
        r#"{"codigo":"so_consulta","mensagem":"Só consulta."}"#
    );
    let e = e.com_campo("pagoEm");
    assert!(
        serde_json::to_string(&e)
            .unwrap()
            .contains(r#""campo":"pagoEm""#)
    );
}

#[test]
fn alertas_trazem_matricula_nome_e_data() {
    let mut c = bd();
    let a = carro(&c, "AA-12-BB");
    let mut joao = MotoristaInput {
        id: None,
        nome: "João Silva".into(),
        telefone: None,
        carta_numero: None,
        carta_validade: Some("2026-12-12".into()),
        ativo: true,
        notas: None,
    };
    let jid = guardar_motorista(&c, &joao).unwrap();
    joao.id = Some(jid);
    lancar_documento(&mut c, &doc(a, "IUC", 2_000, "2026-10-31", None), hoje()).unwrap();
    let al = alertas(&c, hoje()).unwrap();
    let docu = al.iter().find(|x| x.tipo == TipoAlerta::Documento).unwrap();
    assert_eq!(docu.matricula.as_deref(), Some("AA-12-BB"));
    assert_eq!(docu.data, Some(d(2026, 10, 31)));
    let carta = al.iter().find(|x| x.tipo == TipoAlerta::Carta).unwrap();
    assert_eq!(carta.nome.as_deref(), Some("João Silva"));
    assert_eq!(carta.matricula, None);
    assert_eq!(carta.data, Some(d(2026, 12, 12)));
    let json = serde_json::to_value(docu).unwrap();
    assert_eq!(json["data"], "2026-10-31");
    assert_eq!(json["matricula"], "AA-12-BB");
}

#[test]
fn relatorio_traz_total_por_mes() {
    let mut c = bd();
    let a = carro(&c, "AA-12-BB");
    lancar_manutencao(&mut c, &manut(a, "2026-03-01", None), hoje()).unwrap();
    lancar_pneu(&mut c, &pneu(a, "2026-03-05", 2_500), hoje()).unwrap();
    let r = relatorio(
        &c,
        &FiltroRelatorio {
            de: "2026-01-01".into(),
            ate: "2026-12-31".into(),
            carro_ids: None,
            motorista_id: None,
        },
        hoje(),
    )
    .unwrap();
    assert_eq!(r.resumo.total_por_mes.get("2026-03"), Some(&3_500));
    assert_eq!(r.resumo.total_por_mes.len(), 1);
}

#[test]
fn relatorio_por_reembolsar_usa_a_mesma_regra_da_ficha() {
    let mut c = bd();
    let a = carro(&c, "AA-12-BB");
    let ana = motorista(&c, "Ana");
    // adiantou e pagou: por reembolsar (8 000)
    let mut m = multa(a, Some(ana), "2026-09-01", 12_000, "coparticipado");
    m.valor_empresa = Some(4_000);
    m.empresa_adiantou = true;
    m.pago_em = Some("2026-09-10".into());
    guardar_multa(&mut c, &m, hoje()).unwrap();
    // adiantamento marcado mas ainda por pagar: ainda não há nada a reembolsar
    let mut m = multa(a, Some(ana), "2026-09-02", 5_000, "colaborador");
    m.empresa_adiantou = true;
    guardar_multa(&mut c, &m, hoje()).unwrap();
    // paga com adiantamento mas sem condutor identificado: por identificar, não por reembolsar
    let mut m = multa(a, None, "2026-09-03", 3_000, "colaborador");
    m.empresa_adiantou = true;
    m.pago_em = Some("2026-09-10".into());
    guardar_multa(&mut c, &m, hoje()).unwrap();

    let filtro = |motorista_id| FiltroRelatorio {
        de: "2026-01-01".into(),
        ate: "2026-12-31".into(),
        carro_ids: None,
        motorista_id,
    };
    let soma = |l: Vec<MultaItem>| -> i64 {
        l.iter()
            .filter(|x| x.estado_multa == EstadoMulta::PorReembolsar)
            .map(|x| x.valor_colaborador)
            .sum()
    };
    let ficha = multas_motorista(&c, ana, hoje()).unwrap().por_reembolsar;
    assert_eq!(ficha, 8_000);
    assert_eq!(
        relatorio(&c, &filtro(Some(ana)), hoje())
            .unwrap()
            .multas
            .por_reembolsar,
        ficha
    );
    assert_eq!(soma(multas_listar(&c, None, Some(ana)).unwrap()), ficha);
    let todas = soma(multas_listar(&c, None, None).unwrap());
    assert_eq!(
        relatorio(&c, &filtro(None), hoje())
            .unwrap()
            .multas
            .por_reembolsar,
        todas
    );
    assert_eq!(todas, 8_000);
}
#[test]
fn tipo_de_manutencao_desativa_e_reativa() {
    let c = bd();
    let id = guardar_tipo(&c, None, "Correia").unwrap();
    definir_tipo_ativo(&c, id, false).unwrap();
    let t = listar_tipos(&c)
        .unwrap()
        .into_iter()
        .find(|t| t.id == id)
        .unwrap();
    assert!(!t.ativo);
    definir_tipo_ativo(&c, id, true).unwrap();
    let t = listar_tipos(&c)
        .unwrap()
        .into_iter()
        .find(|t| t.id == id)
        .unwrap();
    assert!(t.ativo);
    assert_eq!(
        definir_tipo_ativo(&c, 9999, false).unwrap_err().codigo,
        "nao_encontrado"
    );
}

/// "Por pagar (hoje)": um IUC anual pago gera o do ano seguinte (por pagar, a
/// vencer daqui a um ano) — esse não conta. Vencidos e próximos 30 dias contam.
#[test]
fn por_pagar_hoje_exclui_o_documento_do_ano_seguinte() {
    let mut c = bd();
    let c1 = carro(&c, "AA-11-AA");
    let mut iuc = doc(c1, "IUC", 15_000, "2026-10-31", None);
    iuc.anual = true;
    let id = lancar_documento(&mut c, &iuc, hoje()).unwrap();
    // antes de pagar: vence dentro de 30 dias -> conta
    let p = painel(&c, hoje()).unwrap();
    assert_eq!(
        (p.por_pagar, p.por_pagar_vencido, p.por_pagar_proximo),
        (15_000, 0, 15_000)
    );
    let novo = pagar_documento(&mut c, id, "2026-10-15", hoje()).unwrap();
    assert!(novo.is_some(), "gerou o IUC do ano seguinte");
    // seguro vencido e multa da empresa com prazo dentro de 30 dias
    lancar_documento(
        &mut c,
        &doc(c1, "seguro", 2_000, "2026-10-01", None),
        hoje(),
    )
    .unwrap();
    let mut m = multa(c1, None, "2026-10-01", 6_000, "empresa");
    m.vencimento = Some("2026-11-05".into());
    guardar_multa(&mut c, &m, hoje()).unwrap();
    let p = painel(&c, hoje()).unwrap();
    assert_eq!(p.por_pagar, 8_000);
    assert_eq!(p.por_pagar_vencido, 2_000);
    assert_eq!(p.por_pagar_proximo, 6_000);
    assert_eq!(p.por_pagar_dias, 30);
    let f = FiltroRelatorio {
        de: "2026-01-01".into(),
        ate: "2026-12-31".into(),
        carro_ids: None,
        motorista_id: None,
    };
    let r = relatorio(&c, &f, hoje()).unwrap();
    assert_eq!(r.resumo.por_pagar, 8_000);
    // com um limiar de 60 dias, a multa e nada mais (o IUC de 2027 continua fora)
    guardar_limiares(
        &c,
        &Limiares {
            km: 1_000,
            dias: 60,
        },
    )
    .unwrap();
    assert_eq!(painel(&c, hoje()).unwrap().por_pagar, 8_000);
}
