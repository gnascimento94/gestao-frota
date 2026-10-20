//! Atualização automática a partir das Releases do GitHub (spec §3).
//!
//! O workflow `.github/workflows/release.yml` publica, em cada tag `vX.Y.Z`, o
//! `Frota.exe` e um `latest.json` `{versao, notas, url, sha256, assinatura}`.
//!
//! **Formato das chaves e da assinatura** (o que o `npx tauri signer` escreve):
//! - chave pública (`frota.key.pub`): base64 do ficheiro de chave pública minisign
//!   (`untrusted comment: …\nRW…`);
//! - assinatura (`Frota.exe.sig`, copiada tal e qual para `assinatura`): base64 do
//!   ficheiro de assinatura minisign (comentário, assinatura, `trusted comment:`
//!   com `version:X.Y.Z`, assinatura global).
//!
//! Em ambos os casos descodifica-se o base64 para texto e passa-se o texto a
//! `minisign_verify` (`PublicKey::decode` / `Signature::decode`). Os testes usam
//! um par de chaves de teste e passam pelo mesmo `validar`.
//!
//! Os dados (`dados\`) nunca são tocados aqui: só se trocam os executáveis.

use crate::erro::{Erro, Res};
use base64::Engine;
use minisign_verify::{PublicKey, Signature};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

pub const URL_LATEST: &str =
    "https://github.com/gnascimento94/gestao-frota/releases/latest/download/latest.json";

/// Conteúdo de `%USERPROFILE%\.tauri\frota.key.pub` (a chave privada nunca está no repositório).
pub const CHAVE_PUBLICA: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IERBQzVGMEM5MTQyNEMzQkYKUldTL3d5UVV5ZkRGMmhaUVllTzBBaVo2WW5KRGtrOC9QUTFablNYTkhFcDY4dHRzMlBqK21FZVIK";

/// Nome do executável descarregado, ao lado do atual.
pub const NOME_NOVO: &str = "Frota.new.exe";
/// Nome do executável anterior depois da troca (apagado no arranque seguinte).
pub const NOME_OLD: &str = "Frota.old.exe";

/// Tempo máximo para obter o `latest.json` (nunca atrasa o arranque mais do que isto).
const TIMEOUT_LATEST: Duration = Duration::from_secs(5);
/// Tempo máximo para descarregar o executável.
const TIMEOUT_DOWNLOAD: Duration = Duration::from_secs(10 * 60);
const TIMEOUT_LIGACAO: Duration = Duration::from_secs(15);
/// Tamanho máximo aceite para o executável.
const MAX_EXE: u64 = 200 * 1024 * 1024;

#[derive(Debug, Clone, Deserialize)]
pub struct Latest {
    pub versao: String,
    pub notas: String,
    pub url: String,
    pub sha256: String,
    pub assinatura: String,
}

fn erro(codigo: &str, mensagem: impl Into<String>) -> Erro {
    Erro { campo: None, codigo: codigo.into(), mensagem: mensagem.into() }
}

fn invalida() -> Erro {
    erro(
        "atualizacao_invalida",
        "A atualização descarregada não passou a verificação de segurança. Continua a versão atual.",
    )
}

/// `Some` só se `json` for um `latest.json` válido com versão (semver) estritamente maior.
pub fn nova_versao(atual: &str, json: &str) -> Option<Latest> {
    let l: Latest = serde_json::from_str(json).ok()?;
    let nova = semver::Version::parse(l.versao.trim()).ok()?;
    let atual = semver::Version::parse(atual.trim()).ok()?;
    (nova > atual).then_some(l)
}

/// Base64 → texto (formato do `tauri signer`).
fn texto_b64(s: &str) -> Option<String> {
    let b = base64::engine::general_purpose::STANDARD.decode(s.trim()).ok()?;
    String::from_utf8(b).ok()
}

fn chave(s: &str) -> Option<PublicKey> {
    PublicKey::decode(&texto_b64(s)?).ok()
}

fn sha256_hex(b: &[u8]) -> String {
    Sha256::digest(b).iter().map(|x| format!("{x:02x}")).collect()
}

/// Versão a que a assinatura está ligada (`version:` do comentário de confiança).
fn versao_assinada(s: &Signature) -> Option<&str> {
    s.trusted_comment()
        .split('\t')
        .find_map(|campo| campo.strip_prefix("version:"))
}

/// SHA-256 (hex, sem distinguir maiúsculas) e assinatura minisign de `bytes`,
/// ligada à versão anunciada. Qualquer falha → `atualizacao_invalida`.
pub fn validar(bytes: &[u8], l: &Latest, chave_publica: &str) -> Res<()> {
    if !sha256_hex(bytes).eq_ignore_ascii_case(l.sha256.trim()) {
        return Err(invalida());
    }
    let pk = chave(chave_publica).ok_or_else(invalida)?;
    let sig = texto_b64(&l.assinatura)
        .and_then(|t| Signature::decode(&t).ok())
        .ok_or_else(invalida)?;
    pk.verify(bytes, &sig, true).map_err(|_| invalida())?;
    // Impede que um binário antigo (assinado) seja anunciado como versão nova.
    if versao_assinada(&sig) != Some(l.versao.trim()) {
        return Err(invalida());
    }
    Ok(())
}

/// `exe` → `Frota.old.exe` (apaga o `.old` anterior), `novo` → `exe`. Se o
/// segundo rename falhar, desfaz o primeiro. O Windows deixa renomear o
/// executável em uso.
pub fn trocar_exe(exe: &Path, novo: &Path) -> Res<()> {
    let old = exe.with_file_name(NOME_OLD);
    match std::fs::remove_file(&old) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    std::fs::rename(exe, &old)?;
    if let Err(e) = std::fs::rename(novo, exe) {
        if let Err(e2) = std::fs::rename(&old, exe) {
            log::error!("atualização: não foi possível repor o executável: {e2}");
        }
        return Err(e.into());
    }
    Ok(())
}

/// Apaga `Frota.old.exe` (e um `Frota.new.exe` esquecido). Ignora erros.
pub fn limpar_old(exe: &Path) {
    let _ = std::fs::remove_file(exe.with_file_name(NOME_OLD));
    let _ = std::fs::remove_file(exe.with_file_name(NOME_NOVO));
}

// ---------- rede ----------

fn agente(global: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(global))
        .timeout_connect(Some(TIMEOUT_LIGACAO.min(global)))
        .build()
        .into()
}

/// `latest.json` da última Release (timeout 5 s); qualquer erro → `None`.
fn obter_latest() -> Option<String> {
    let mut r = agente(TIMEOUT_LATEST).get(URL_LATEST).call().ok()?;
    r.body_mut().with_config().limit(1024 * 1024).read_to_string().ok()
}

fn descarregar(url: &str) -> Res<Vec<u8>> {
    if !url.starts_with("https://") {
        return Err(invalida());
    }
    let falha = |e: ureq::Error| {
        erro("sem_ligacao", format!("Não foi possível descarregar a atualização ({e})."))
    };
    let mut r = agente(TIMEOUT_DOWNLOAD).get(url).call().map_err(falha)?;
    let mut bytes = Vec::new();
    r.body_mut()
        .with_config()
        .limit(MAX_EXE)
        .reader()
        .read_to_end(&mut bytes)
        .map_err(|e| erro("sem_ligacao", format!("Não foi possível descarregar a atualização ({e}).")))?;
    Ok(bytes)
}

/// Última versão encontrada por `verificar` (usada por `aplicar`).
static PENDENTE: Mutex<Option<Latest>> = Mutex::new(None);

/// Há versão mais recente do que `atual`? Fica guardada para `preparar`.
pub fn verificar(atual: &str) -> Option<Latest> {
    let l = nova_versao(atual, &obter_latest()?)?;
    *PENDENTE.lock().unwrap_or_else(|e| e.into_inner()) = Some(l.clone());
    Some(l)
}

/// Descarrega a versão encontrada por `verificar` (ou volta a procurar) para
/// `Frota.new.exe` ao lado de `exe` e valida-a. Em falha apaga `Frota.new.exe`.
/// Devolve a versão e o caminho do novo executável.
pub fn preparar(exe: &Path, atual: &str) -> Res<(Latest, PathBuf)> {
    let guardada = PENDENTE.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let l = guardada
        .or_else(|| verificar(atual))
        .ok_or_else(|| erro("sem_atualizacao", "Não foi possível obter a nova versão."))?;
    let novo = exe.with_file_name(NOME_NOVO);
    let r = descarregar(&l.url).and_then(|bytes| {
        std::fs::write(&novo, &bytes)?;
        validar(&bytes, &l, CHAVE_PUBLICA)
    });
    match r {
        Ok(()) => Ok((l, novo)),
        Err(e) => {
            let _ = std::fs::remove_file(&novo);
            Err(e)
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// Par de chaves só de teste (geradas com `npx tauri signer generate`).
    const CHAVE_TESTE: &str = include_str!("../../tests/fixtures/teste.key.pub");
    const OUTRA_CHAVE_TESTE: &str = include_str!("../../tests/fixtures/outra-teste.key.pub");
    /// Ficheiro assinado com `npx tauri signer sign --app-version 9.9.9` e a chave de teste.
    const CONTEUDO: &[u8] = include_bytes!("../../tests/fixtures/conteudo-teste.bin");
    const ASSINATURA: &str = include_str!("../../tests/fixtures/conteudo-teste.bin.sig");

    fn json(versao: &str) -> &'static str {
        Box::leak(
            format!(
                r#"{{"versao":"{versao}","notas":"Notas","url":"https://github.com/x/y/releases/download/v{versao}/Frota.exe","sha256":"00","assinatura":"x"}}"#
            )
            .into_boxed_str(),
        )
    }

    fn latest_valido() -> Latest {
        Latest {
            versao: "9.9.9".into(),
            notas: String::new(),
            url: "https://github.com/x/y/releases/download/v9.9.9/Frota.exe".into(),
            sha256: sha256_hex(CONTEUDO),
            assinatura: ASSINATURA.trim().into(),
        }
    }

    #[test]
    fn deteta_versao_maior() {
        assert!(nova_versao("1.2.0", json("1.3.0")).is_some());
        assert!(nova_versao("1.3.0", json("1.3.0")).is_none());
        assert!(nova_versao("1.3.0", json("1.2.9")).is_none());
        assert!(nova_versao("1.0.0", "lixo").is_none());
        assert_eq!(nova_versao("1.2.0", json("1.10.0")).unwrap().versao, "1.10.0");
        assert!(nova_versao("1.0.0", json("nao-e-versao")).is_none());
    }

    #[test]
    fn sha_errado_e_recusado() {
        let mut l = latest_valido();
        l.sha256 = sha256_hex(b"outro conteudo");
        let e = validar(CONTEUDO, &l, CHAVE_TESTE).unwrap_err();
        assert_eq!(e.codigo, "atualizacao_invalida");
        // conteúdo adulterado com o SHA do original
        let e = validar(b"adulterado", &latest_valido(), CHAVE_TESTE).unwrap_err();
        assert_eq!(e.codigo, "atualizacao_invalida");
    }

    #[test]
    fn assinatura_de_outra_chave_e_recusada() {
        let e = validar(CONTEUDO, &latest_valido(), OUTRA_CHAVE_TESTE).unwrap_err();
        assert_eq!(e.codigo, "atualizacao_invalida");
        // assinatura ilegível
        let mut l = latest_valido();
        l.assinatura = "lixo".into();
        assert_eq!(validar(CONTEUDO, &l, CHAVE_TESTE).unwrap_err().codigo, "atualizacao_invalida");
    }

    #[test]
    fn assinatura_valida_e_aceite() {
        validar(CONTEUDO, &latest_valido(), CHAVE_TESTE).unwrap();
        // SHA em maiúsculas também é aceite
        let mut l = latest_valido();
        l.sha256 = l.sha256.to_uppercase();
        validar(CONTEUDO, &l, CHAVE_TESTE).unwrap();
    }

    #[test]
    fn assinatura_de_outra_versao_e_recusada() {
        // a assinatura foi feita para a 9.9.9: um latest.json que anuncie outra
        // versão com o mesmo binário (p.ex. um binário antigo) é recusado
        let mut l = latest_valido();
        l.versao = "10.0.0".into();
        assert_eq!(validar(CONTEUDO, &l, CHAVE_TESTE).unwrap_err().codigo, "atualizacao_invalida");
    }

    #[test]
    fn chave_publica_embutida_e_valida() {
        assert!(chave(CHAVE_PUBLICA).is_some());
    }

    #[test]
    fn troca_exe_mantem_o_antigo_como_old() {
        let t = tempfile::tempdir().unwrap();
        let exe = t.path().join("Frota.exe");
        let novo = t.path().join("Frota.new.exe");
        let old = t.path().join("Frota.old.exe");
        std::fs::write(&exe, "v1").unwrap();
        std::fs::write(&novo, "v2").unwrap();
        std::fs::write(&old, "v0").unwrap(); // de uma atualização anterior
        trocar_exe(&exe, &novo).unwrap();
        assert_eq!(std::fs::read_to_string(&exe).unwrap(), "v2");
        assert_eq!(std::fs::read_to_string(&old).unwrap(), "v1");
        assert!(!novo.exists());
        limpar_old(&exe);
        assert!(!old.exists());
        assert_eq!(std::fs::read_to_string(&exe).unwrap(), "v2");
    }

    #[test]
    fn troca_falhada_repoe_o_exe() {
        let t = tempfile::tempdir().unwrap();
        let exe = t.path().join("Frota.exe");
        let novo = t.path().join("Frota.new.exe"); // não existe: o 2.º rename falha
        std::fs::write(&exe, "v1").unwrap();
        assert!(trocar_exe(&exe, &novo).is_err());
        assert_eq!(std::fs::read_to_string(&exe).unwrap(), "v1");
        assert!(!t.path().join("Frota.old.exe").exists());
    }

    #[test]
    fn limpar_old_sem_ficheiros_nao_falha() {
        let t = tempfile::tempdir().unwrap();
        limpar_old(&t.path().join("Frota.exe"));
    }
}
