use crate::erro::{Erro, Res};

fn invalida() -> Erro {
    Erro {
        codigo: "matricula_invalida".into(),
        mensagem: "Matrícula inválida. Exemplo: AA-12-BB".into(),
    }
}

/// Normaliza uma matrícula portuguesa para `XX-XX-XX`.
pub fn normalizar_matricula(s: &str) -> Res<String> {
    let limpa: Vec<char> = s
        .chars()
        .filter(|c| !c.is_whitespace() && !matches!(c, '-' | '.'))
        .map(|c| c.to_ascii_uppercase())
        .collect();
    if limpa.len() != 6 || !limpa.iter().all(|c| c.is_ascii_alphanumeric()) {
        return Err(invalida());
    }
    let mut letras = 0;
    let mut digitos = 0;
    for par in limpa.chunks(2) {
        if par.iter().all(|c| c.is_ascii_alphabetic()) {
            letras += 1;
        } else if par.iter().all(|c| c.is_ascii_digit()) {
            digitos += 1;
        } else {
            return Err(invalida());
        }
    }
    if letras == 0 || digitos == 0 {
        return Err(invalida());
    }
    Ok(limpa
        .chunks(2)
        .map(|p| p.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join("-"))
}
#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn matriculas_equivalentes() {
        for s in ["aa12bb", "AA 12 BB", "aa-12-bb", " AA.12.BB "] {
            assert_eq!(normalizar_matricula(s).unwrap(), "AA-12-BB");
        }
    }

    #[test]
    fn formatos_antigos() {
        assert_eq!(normalizar_matricula("12-34-ab").unwrap(), "12-34-AB");
        assert_eq!(normalizar_matricula("12ab34").unwrap(), "12-AB-34");
    }

    #[test]
    fn matricula_invalida() {
        for s in ["", "ABC", "AB-12-B", "AB-CD-EF", "12-34-56", "AÁ-12-BB", "AA/12/BB"] {
            let e = normalizar_matricula(s).unwrap_err();
            assert_eq!(e.codigo, "matricula_invalida");
            assert_eq!(e.mensagem, "Matrícula inválida. Exemplo: AA-12-BB");
        }
    }
}