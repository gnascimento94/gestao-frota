use crate::erro::{Erro, Res};
use chrono::{DateTime, Duration, Utc};
use std::path::{Path, PathBuf};

/// Um bloqueio sem renovação há mais do que isto (estritamente) é considerado abandonado.
const VALIDADE_SEGUNDOS: i64 = 120;

/// Bloqueio entre PCs: ficheiro `pc\nRFC3339` na pasta partilhada.
/// A renovação periódica (30 s) é da responsabilidade de quem o usa.
pub struct Lock {
    caminho: PathBuf,
    pc: String,
}

enum Estado {
    Ausente,
    /// Ficheiro ilegível por conteúdo (não UTF-8 / sem o formato `pc\nRFC3339`).
    /// É tratado como abandonado (será sobrescrito): bloquear para sempre por
    /// causa de lixo seria pior do que o risco mínimo de uma escrita simultânea.
    Lixo,
    Valido(String, DateTime<Utc>),
    /// Erro de E/S ao ler (não é "não existe"): não sabemos de quem é.
    Desconhecido,
}

fn ler(caminho: &Path) -> Estado {
    let texto = match std::fs::read_to_string(caminho) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Estado::Ausente,
        Err(e) if e.kind() == std::io::ErrorKind::InvalidData => return Estado::Lixo,
        Err(_) => return Estado::Desconhecido,
    };
    let Some((pc, hora)) = texto.split_once('\n') else { return Estado::Lixo };
    match DateTime::parse_from_rfc3339(hora.trim()) {
        Ok(h) => Estado::Valido(pc.to_string(), h.with_timezone(&Utc)),
        Err(_) => Estado::Lixo,
    }
}

/// Fresco: até 120 s de idade; no futuro (relógios dessincronizados) só até 120 s à frente.
fn fresco(agora: DateTime<Utc>, hora: DateTime<Utc>) -> bool {
    let validade = Duration::seconds(VALIDADE_SEGUNDOS);
    let d = agora - hora;
    d <= validade && d >= -validade
}

fn bloqueado(pc: impl Into<String>) -> Erro {
    Erro { codigo: "bloqueado".into(), mensagem: pc.into() }
}

/// Falha se o lock pertence a outro PC e está fresco (ou não se consegue ler).
fn verificar(caminho: &Path, pc: &str, agora: DateTime<Utc>) -> Result<(), Erro> {
    match ler(caminho) {
        Estado::Valido(outro, hora) if outro != pc && fresco(agora, hora) => Err(bloqueado(outro)),
        Estado::Desconhecido => Err(bloqueado("desconhecido")),
        _ => Ok(()),
    }
}

/// Escrita atómica: ficheiro temporário + rename.
fn escrever(caminho: &Path, pc: &str, agora: DateTime<Utc>) -> Res<()> {
    let mut s = caminho.as_os_str().to_owned();
    s.push(".tmp");
    let tmp = PathBuf::from(s);
    let r = std::fs::write(&tmp, format!("{pc}\n{}", agora.to_rfc3339()))
        .and_then(|_| std::fs::rename(&tmp, caminho));
    if r.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    Ok(r?)
}

pub fn adquirir(caminho: &Path, pc: &str, agora: DateTime<Utc>) -> Result<Lock, Erro> {
    verificar(caminho, pc, agora)?;
    escrever(caminho, pc, agora)?;
    Ok(Lock { caminho: caminho.to_path_buf(), pc: pc.to_string() })
}

impl Lock {
    /// Renova a hora. Se outro PC assumiu o lock entretanto, devolve `bloqueado`
    /// (mensagem = esse PC) e não escreve nada.
    pub fn renovar(&self, agora: DateTime<Utc>) -> Res<()> {
        verificar(&self.caminho, &self.pc, agora)?;
        escrever(&self.caminho, &self.pc, agora)
    }

    /// Apaga o ficheiro só se ainda pertencer a este PC.
    pub fn libertar(self) -> Res<()> {
        if let Estado::Valido(pc, _) = ler(&self.caminho) {
            if pc == self.pc {
                match std::fs::remove_file(&self.caminho) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(e.into()),
                }
            }
        }
        Ok(())
    }
}
#[cfg(test)]
mod testes {
    use super::*;
    use chrono::{Duration, TimeZone, Utc};

    fn agora() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 20, 12, 0, 0).unwrap()
    }

    fn caminho() -> (tempfile::TempDir, std::path::PathBuf) {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("lock");
        (t, p)
    }

    #[test]
    fn lock_de_outro_pc_recente_bloqueia() {
        let (_t, p) = caminho();
        adquirir(&p, "PC-X", agora() - Duration::seconds(60)).unwrap();
        let e = adquirir(&p, "PC-Y", agora()).err().unwrap();
        assert_eq!(e.codigo, "bloqueado");
        assert_eq!(e.mensagem, "PC-X");
    }

    #[test]
    fn lock_abandonado_e_assumido() {
        let (_t, p) = caminho();
        adquirir(&p, "PC-X", agora() - Duration::seconds(121)).unwrap();
        adquirir(&p, "PC-Y", agora()).unwrap();
        assert!(std::fs::read_to_string(&p).unwrap().starts_with("PC-Y\n"));
    }

    #[test]
    fn lock_com_exatamente_120s_ainda_bloqueia() {
        let (_t, p) = caminho();
        adquirir(&p, "PC-X", agora() - Duration::seconds(120)).unwrap();
        assert!(adquirir(&p, "PC-Y", agora()).is_err());
    }

    #[test]
    fn lock_do_mesmo_pc_e_assumido() {
        let (_t, p) = caminho();
        adquirir(&p, "PC-X", agora()).unwrap();
        adquirir(&p, "PC-X", agora()).unwrap();
    }

    #[test]
    fn lock_ilegivel_e_tratado_como_abandonado() {
        let (_t, p) = caminho();
        std::fs::write(&p, b"\xff\xfelixo sem formato").unwrap();
        adquirir(&p, "PC-Y", agora()).unwrap();
    }

    #[test]
    fn renovar_atualiza_a_hora() {
        let (_t, p) = caminho();
        let l = adquirir(&p, "PC-X", agora() - Duration::seconds(100)).unwrap();
        l.renovar(agora()).unwrap();
        // 100 s depois da renovação ainda está fresco para o outro PC
        assert!(adquirir(&p, "PC-Y", agora() + Duration::seconds(100)).is_err());
    }

    #[test]
    fn libertar_apaga_so_se_ainda_for_nosso() {
        let (_t, p) = caminho();
        let l = adquirir(&p, "PC-X", agora()).unwrap();
        l.libertar().unwrap();
        assert!(!p.exists());

        let l = adquirir(&p, "PC-X", agora()).unwrap();
        // outro PC assumiu entretanto
        std::fs::write(&p, format!("PC-Y\n{}", agora().to_rfc3339())).unwrap();
        l.libertar().unwrap();
        assert!(p.exists());
    }

    #[test]
    fn renovar_nao_rouba_lock_de_outro_pc() {
        let (_t, p) = caminho();
        let l = adquirir(&p, "PC-X", agora() - Duration::seconds(200)).unwrap();
        // PC-Y assumiu o lock abandonado
        adquirir(&p, "PC-Y", agora() - Duration::seconds(10)).unwrap();
        let e = l.renovar(agora()).unwrap_err();
        assert_eq!(e.codigo, "bloqueado");
        assert_eq!(e.mensagem, "PC-Y");
        assert!(std::fs::read_to_string(&p).unwrap().starts_with("PC-Y\n"));
    }

    #[test]
    fn renovar_depois_de_apagado_volta_a_escrever() {
        let (_t, p) = caminho();
        let l = adquirir(&p, "PC-X", agora()).unwrap();
        std::fs::remove_file(&p).unwrap();
        l.renovar(agora()).unwrap();
        assert!(p.exists());
    }

    #[test]
    fn relogio_adiantado_so_conta_ate_120s() {
        let (_t, p) = caminho();
        adquirir(&p, "PC-X", agora() + Duration::seconds(100)).unwrap();
        assert!(adquirir(&p, "PC-Y", agora()).is_err());
        adquirir(&p, "PC-X", agora() + Duration::seconds(130)).unwrap();
        adquirir(&p, "PC-Y", agora()).unwrap();
    }

    #[test]
    fn erro_de_leitura_nao_e_tratado_como_abandonado() {
        let (_t, p) = caminho();
        std::fs::create_dir(&p).unwrap(); // ler como ficheiro falha (não é NotFound nem lixo)
        let e = adquirir(&p, "PC-Y", agora()).err().unwrap();
        assert_eq!(e.codigo, "bloqueado");
        assert_eq!(e.mensagem, "desconhecido");
    }

    #[test]
    fn escrita_nao_deixa_tmp() {
        let (t, p) = caminho();
        adquirir(&p, "PC-X", agora()).unwrap();
        assert_eq!(std::fs::read_dir(t.path()).unwrap().count(), 1);
    }
}