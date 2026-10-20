use crate::erro::{Erro, Res};
use chrono::{DateTime, Utc};
use std::path::{Path, PathBuf};

/// Um bloqueio sem renovação há mais do que isto (estritamente) é considerado abandonado.
const VALIDADE_SEGUNDOS: i64 = 120;

/// Bloqueio entre PCs: ficheiro `pc\nRFC3339` na pasta partilhada.
/// A renovação periódica (30 s) é da responsabilidade de quem o usa.
pub struct Lock {
    caminho: PathBuf,
    pc: String,
}

/// Lê (pc, hora). Ficheiro inexistente, ilegível ou mal formado -> `None`.
/// Um lock ilegível/garbled é tratado como abandonado (será sobrescrito):
/// bloquear para sempre por causa de lixo seria pior do que o risco, mínimo,
/// de dois PCs a escreverem ao mesmo tempo.
fn ler(caminho: &Path) -> Option<(String, DateTime<Utc>)> {
    let texto = std::fs::read_to_string(caminho).ok()?;
    let (pc, hora) = texto.split_once('\n')?;
    let hora = DateTime::parse_from_rfc3339(hora.trim()).ok()?.with_timezone(&Utc);
    Some((pc.to_string(), hora))
}

fn escrever(caminho: &Path, pc: &str, agora: DateTime<Utc>) -> Res<()> {
    std::fs::write(caminho, format!("{pc}\n{}", agora.to_rfc3339()))?;
    Ok(())
}

pub fn adquirir(caminho: &Path, pc: &str, agora: DateTime<Utc>) -> Result<Lock, Erro> {
    if let Some((outro, hora)) = ler(caminho) {
        let fresco = (agora - hora).num_seconds() <= VALIDADE_SEGUNDOS;
        if outro != pc && fresco {
            return Err(Erro { codigo: "bloqueado".into(), mensagem: outro });
        }
    }
    escrever(caminho, pc, agora)?;
    Ok(Lock { caminho: caminho.to_path_buf(), pc: pc.to_string() })
}

impl Lock {
    pub fn renovar(&self, agora: DateTime<Utc>) -> Res<()> {
        escrever(&self.caminho, &self.pc, agora)
    }

    /// Apaga o ficheiro só se ainda pertencer a este PC.
    pub fn libertar(self) -> Res<()> {
        if let Some((pc, _)) = ler(&self.caminho) {
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
}
