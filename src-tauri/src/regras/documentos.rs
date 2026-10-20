use chrono::{Months, NaiveDate};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NovoDocumento {
    pub carro_id: i64,
    pub tipo: String,
    pub descricao: String,
    /// Cêntimos.
    pub valor: i64,
    pub vencimento: NaiveDate,
    pub anual: bool,
    pub gerado_de_id: i64,
}

/// Documento do ano seguinte: vencimento + 12 meses (29-02 -> 28-02).
pub fn proximo_anual(
    id: i64,
    carro_id: i64,
    tipo: &str,
    descricao: &str,
    valor: i64,
    vencimento: NaiveDate,
) -> NovoDocumento {
    NovoDocumento {
        carro_id,
        tipo: tipo.to_string(),
        descricao: descricao.to_string(),
        valor,
        // +12 meses só falha se ultrapassar o intervalo de NaiveDate (ano ~262000),
        // o que não acontece com datas de vencimento reais.
        vencimento: vencimento
            .checked_add_months(Months::new(12))
            .expect("vencimento + 12 meses dentro do intervalo de datas"),
        anual: true,
        gerado_de_id: id,
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn d(a: i32, m: u32, dia: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(a, m, dia).unwrap()
    }

    #[test]
    fn anual_gera_ano_seguinte() {
        let n = proximo_anual(7, 3, "IUC", "IUC 2026", 15_000, d(2026, 5, 31));
        assert_eq!(n.vencimento, d(2027, 5, 31));
        assert_eq!(n.gerado_de_id, 7);
        assert_eq!(n.carro_id, 3);
        assert_eq!(n.tipo, "IUC");
        assert_eq!(n.valor, 15_000);
        assert!(n.anual);
    }

    #[test]
    fn anual_29_fevereiro() {
        let n = proximo_anual(1, 1, "Seguro", "", 100, d(2028, 2, 29));
        assert_eq!(n.vencimento, d(2029, 2, 28));
    }
}
