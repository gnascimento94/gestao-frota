use chrono::NaiveDate;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Leitura {
    pub data: NaiveDate,
    pub km: i64,
}

pub fn km_atual(km_inicial: i64, leituras: &[Leitura]) -> i64 {
    leituras.iter().map(|l| l.km).fold(km_inicial, i64::max)
}

/// Se `km` é inferior à maior leitura com data <= `data`, devolve essa leitura.
pub fn km_abaixo_do_anterior(leituras: &[Leitura], data: NaiveDate, km: i64) -> Option<i64> {
    let max = leituras.iter().filter(|l| l.data <= data).map(|l| l.km).max()?;
    (km < max).then_some(max)
}

#[cfg(test)]
mod testes {
    use super::*;

    fn d(m: u32, dia: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, m, dia).unwrap()
    }
    fn l(m: u32, dia: u32, km: i64) -> Leitura {
        Leitura { data: d(m, dia), km }
    }

    #[test]
    fn km_atual_sem_leituras_e_o_inicial() {
        assert_eq!(km_atual(50_000, &[]), 50_000);
    }

    #[test]
    fn km_atual_e_o_maximo() {
        let ls = [l(1, 1, 51_000), l(2, 1, 53_000), l(3, 1, 52_000)];
        assert_eq!(km_atual(50_000, &ls), 53_000);
    }

    #[test]
    fn km_abaixo_compara_com_leitura_anterior_por_data() {
        let ls = [l(1, 3, 60_000), l(1, 6, 70_000)];
        assert_eq!(km_abaixo_do_anterior(&ls, d(1, 4), 65_000), None);
        assert_eq!(km_abaixo_do_anterior(&ls, d(1, 7), 69_000), Some(70_000));
        assert_eq!(km_abaixo_do_anterior(&ls, d(1, 2), 1), None);
    }
}