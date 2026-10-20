use crate::erro::{Erro, Res};
use crate::migracoes::migrar;
use rusqlite::{Connection, OpenFlags};
use std::path::{Path, PathBuf};

pub struct Caminhos {
    pub dados: PathBuf,
    pub banco: PathBuf,
    pub trabalho: PathBuf,
    pub pendente: PathBuf,
}

impl Caminhos {
    pub fn new(raiz: &Path, local: &Path) -> Self {
        let dados = raiz.join("dados");
        Caminhos {
            banco: dados.join("frota.db"),
            dados,
            trabalho: local.join("trabalho.db"),
            pendente: local.join("pendente"),
        }
    }
}

fn corrompido() -> Erro {
    Erro {
        codigo: "corrompido".into(),
        mensagem: "O ficheiro de dados parece estar danificado. Nada foi alterado; é possível restaurar uma cópia de segurança.".into(),
    }
}

fn banco_vazio() -> Erro {
    Erro {
        codigo: "corrompido".into(),
        mensagem: "O ficheiro de dados está vazio ou incompleto. Confirme que a pasta Frota está marcada como \"Disponível offline\" no Google Drive e que a sincronização terminou. Nada foi alterado.".into(),
    }
}

fn gravacao_falhou(e: impl std::fmt::Display) -> Erro {
    Erro {
        codigo: "gravacao_falhou".into(),
        mensagem: format!(
            "Não foi possível guardar na pasta de dados. Os dados estão seguros neste computador; vou tentar outra vez. ({e})"
        ),
    }
}

fn existe(p: &Path) -> Res<bool> {
    Ok(p.try_exists()?)
}

fn com_sufixo(p: &Path, sufixo: &str) -> PathBuf {
    let mut s = p.as_os_str().to_owned();
    s.push(sufixo);
    PathBuf::from(s)
}

fn remover_se_existe(p: &Path) -> Res<()> {
    match std::fs::remove_file(p) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// Remove a cópia de trabalho e os ficheiros auxiliares do SQLite.
fn limpar_trabalho(trabalho: &Path, incluir_principal: bool) -> Res<()> {
    for s in ["-journal", "-wal", "-shm"] {
        remover_se_existe(&com_sufixo(trabalho, s))?;
    }
    if incluir_principal {
        remover_se_existe(trabalho)?;
    }
    Ok(())
}

fn codigo_corrompido(f: &rusqlite::ffi::Error) -> bool {
    matches!(f.code, rusqlite::ErrorCode::NotADatabase | rusqlite::ErrorCode::DatabaseCorrupt)
}

fn integridade_ok(conn: &Connection) -> Res<bool> {
    match conn.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0)) {
        Ok(r) => Ok(r == "ok"),
        Err(rusqlite::Error::SqliteFailure(f, _)) if codigo_corrompido(&f) => Ok(false),
        Err(e) => Err(e.into()),
    }
}

/// Verifica o banco partilhado sem o modificar.
fn banco_integro(banco: &Path) -> Res<bool> {
    match Connection::open_with_flags(banco, OpenFlags::SQLITE_OPEN_READ_ONLY) {
        Ok(conn) => integridade_ok(&conn),
        Err(rusqlite::Error::SqliteFailure(f, _)) if codigo_corrompido(&f) => Ok(false),
        Err(e) => Err(e.into()),
    }
}

/// Cria o marcador de alterações por gravar (idempotente).
pub fn marcar_pendente(c: &Caminhos) -> Res<()> {
    std::fs::write(&c.pendente, b"").map_err(gravacao_falhou)
}

pub fn preparar_trabalho(c: &Caminhos) -> Res<Connection> {
    std::fs::create_dir_all(&c.dados)?;
    if let Some(local) = c.trabalho.parent() {
        std::fs::create_dir_all(local)?;
    }
    let pendente = existe(&c.pendente)? && existe(&c.trabalho)?;
    if !pendente {
        if existe(&c.banco)? {
            if std::fs::metadata(&c.banco)?.len() == 0 {
                return Err(banco_vazio());
            }
            if !banco_integro(&c.banco)? {
                return Err(corrompido());
            }
            limpar_trabalho(&c.trabalho, false)?;
            std::fs::copy(&c.banco, &c.trabalho)?;
        } else {
            limpar_trabalho(&c.trabalho, true)?;
        }
    }
    let mut conn = match Connection::open(&c.trabalho) {
        Ok(conn) => conn,
        Err(rusqlite::Error::SqliteFailure(f, _)) if codigo_corrompido(&f) => {
            return Err(corrompido())
        }
        Err(e) => return Err(e.into()),
    };
    if !integridade_ok(&conn)? {
        return Err(corrompido());
    }
    conn.execute_batch("PRAGMA foreign_keys=ON;")?;
    conn.query_row("PRAGMA journal_mode=DELETE", [], |_| Ok(()))?;
    migrar(&mut conn)?;
    if pendente {
        // Falha aqui não impede abrir; o marcador permanece.
        let _ = gravar(&conn, c);
    }
    Ok(conn)
}

fn escrever_tmp(conn: &Connection, tmp: &Path) -> Res<()> {
    if existe(tmp).map_err(|e| gravacao_falhou(e.mensagem))? {
        std::fs::remove_file(tmp).map_err(gravacao_falhou)?;
    }
    let destino = tmp.to_string_lossy().into_owned();
    conn.execute("VACUUM INTO ?1", [destino]).map_err(gravacao_falhou)?;
    // Flush para disco antes da substituição atómica.
    std::fs::OpenOptions::new()
        .write(true)
        .open(tmp)
        .and_then(|f| f.sync_all())
        .map_err(gravacao_falhou)?;
    // Nunca substituir um bom ficheiro por um instantâneo mau.
    let chk = Connection::open_with_flags(tmp, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(gravacao_falhou)?;
    let ok = integridade_ok(&chk).map_err(|e| gravacao_falhou(e.mensagem))?;
    drop(chk);
    if !ok {
        return Err(gravacao_falhou("a cópia temporária falhou a verificação de integridade"));
    }
    Ok(())
}

pub fn gravar(conn: &Connection, c: &Caminhos) -> Res<()> {
    marcar_pendente(c)?;
    let tmp = com_sufixo(&c.banco, ".tmp");
    if let Err(e) = escrever_tmp(conn, &tmp) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    if let Err(e) = std::fs::rename(&tmp, &c.banco) {
        let _ = std::fs::remove_file(&tmp);
        return Err(gravacao_falhou(e));
    }
    std::fs::remove_file(&c.pendente).map_err(gravacao_falhou)?;
    Ok(())
}

#[cfg(test)]
mod testes {
    use super::*;

    fn cenario() -> (tempfile::TempDir, Caminhos) {
        let t = tempfile::tempdir().unwrap();
        let c = Caminhos::new(&t.path().join("raiz"), &t.path().join("local"));
        (t, c)
    }

    fn inserir_carro(conn: &Connection, matricula: &str) {
        conn.execute(
            "INSERT INTO carros (matricula, marca, modelo, km_inicial, estado) VALUES (?1, 'Fiat', 'Ducato', 0, 'ativo')",
            [matricula],
        )
        .unwrap();
    }

    fn contar(p: &Path) -> i64 {
        let conn = Connection::open_with_flags(p, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        conn.query_row("SELECT COUNT(*) FROM carros", [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn primeira_abertura_cria_banco_vazio_migrado() {
        let (_t, c) = cenario();
        let conn = preparar_trabalho(&c).unwrap();
        assert!(c.dados.exists());
        assert!(c.trabalho.exists());
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM carros", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn gravar_substitui_banco_com_os_dados_novos() {
        let (_t, c) = cenario();
        let conn = preparar_trabalho(&c).unwrap();
        inserir_carro(&conn, "AA-00-AA");
        gravar(&conn, &c).unwrap();
        assert_eq!(contar(&c.banco), 1);
        assert!(!c.pendente.exists());
    }

    #[test]
    fn falha_ao_gravar_mantem_banco_antigo_e_marca_pendente() {
        let (t, c) = cenario();
        let conn = preparar_trabalho(&c).unwrap();
        inserir_carro(&conn, "AA-00-AA");
        gravar(&conn, &c).unwrap();
        inserir_carro(&conn, "BB-11-BB");
        let mau = Caminhos {
            dados: c.dados.clone(),
            banco: t.path().join("inexistente").join("frota.db"),
            trabalho: c.trabalho.clone(),
            pendente: c.pendente.clone(),
        };
        let e = gravar(&conn, &mau).unwrap_err();
        assert_eq!(e.codigo, "gravacao_falhou");
        assert!(c.pendente.exists());
        assert_eq!(contar(&c.banco), 1);
    }

    #[test]
    fn arranque_com_pendente_usa_trabalho_e_grava_primeiro() {
        let (_t, c) = cenario();
        let conn = preparar_trabalho(&c).unwrap();
        inserir_carro(&conn, "AA-00-AA");
        gravar(&conn, &c).unwrap();
        inserir_carro(&conn, "BB-11-BB");
        drop(conn);
        std::fs::write(&c.pendente, b"").unwrap();
        let conn = preparar_trabalho(&c).unwrap();
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM carros", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 2);
        assert_eq!(contar(&c.banco), 2);
        assert!(!c.pendente.exists());
    }

    #[test]
    fn banco_corrompido_e_detetado() {
        let (_t, c) = cenario();
        std::fs::create_dir_all(&c.dados).unwrap();
        let lixo = vec![0xABu8; 4096];
        std::fs::write(&c.banco, &lixo).unwrap();
        let e = preparar_trabalho(&c).err().unwrap();
        assert_eq!(e.codigo, "corrompido");
        assert_eq!(std::fs::read(&c.banco).unwrap(), lixo);
        assert!(!c.trabalho.exists());
    }

    #[test]
    fn journal_antigo_nao_contamina_copia_nova() {
        let (_t, c) = cenario();
        let conn = preparar_trabalho(&c).unwrap();
        inserir_carro(&conn, "AA-00-AA");
        gravar(&conn, &c).unwrap();
        inserir_carro(&conn, "BB-11-BB");
        drop(conn);
        let journal = com_sufixo(&c.trabalho, "-journal");
        std::fs::write(&journal, b"lixo").unwrap();
        let conn = preparar_trabalho(&c).unwrap();
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM carros", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
        assert!(!journal.exists());
    }

    #[test]
    fn sem_banco_e_sem_pendente_cria_vazio_mesmo_com_trabalho_antigo() {
        let (_t, c) = cenario();
        let conn = preparar_trabalho(&c).unwrap();
        inserir_carro(&conn, "AA-00-AA");
        drop(conn);
        let journal = com_sufixo(&c.trabalho, "-journal");
        std::fs::write(&journal, b"lixo").unwrap();
        assert!(!c.banco.exists() && !c.pendente.exists());
        let conn = preparar_trabalho(&c).unwrap();
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM carros", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0);
        assert!(!journal.exists());
    }

    #[test]
    fn banco_vazio_de_zero_bytes_e_recusado() {
        let (_t, c) = cenario();
        let conn = preparar_trabalho(&c).unwrap();
        inserir_carro(&conn, "AA-00-AA");
        drop(conn);
        std::fs::write(&c.banco, b"").unwrap();
        let e = preparar_trabalho(&c).err().unwrap();
        assert_eq!(e.codigo, "corrompido");
        assert!(e.mensagem.contains("Disponível offline"));
        assert_eq!(std::fs::metadata(&c.banco).unwrap().len(), 0);
        assert_eq!(contar(&c.trabalho), 1);
    }

    #[test]
    fn marcar_pendente_faz_o_arranque_preferir_o_trabalho() {
        let (_t, c) = cenario();
        let conn = preparar_trabalho(&c).unwrap();
        inserir_carro(&conn, "AA-00-AA");
        gravar(&conn, &c).unwrap();
        inserir_carro(&conn, "BB-11-BB");
        marcar_pendente(&c).unwrap();
        marcar_pendente(&c).unwrap();
        drop(conn);
        let conn = preparar_trabalho(&c).unwrap();
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM carros", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 2);
        assert_eq!(contar(&c.banco), 2);
        assert!(!c.pendente.exists());
    }

    #[test]
    fn arranque_com_pendente_e_gravacao_falhada_devolve_conexao_e_mantem_marcador() {
        let (_t, c) = cenario();
        let conn = preparar_trabalho(&c).unwrap();
        inserir_carro(&conn, "AA-00-AA");
        drop(conn);
        // um diretório no lugar do .tmp faz o VACUUM/remoção falhar
        std::fs::create_dir_all(com_sufixo(&c.banco, ".tmp")).unwrap();
        std::fs::write(&c.pendente, b"").unwrap();
        let conn = preparar_trabalho(&c).unwrap();
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM carros", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
        assert!(c.pendente.exists());
    }

    #[test]
    fn trabalho_corrompido_com_pendente_e_detetado_e_intacto() {
        let (_t, c) = cenario();
        std::fs::create_dir_all(c.trabalho.parent().unwrap()).unwrap();
        let lixo = vec![0xCDu8; 4096];
        std::fs::write(&c.trabalho, &lixo).unwrap();
        std::fs::write(&c.pendente, b"").unwrap();
        let e = preparar_trabalho(&c).err().unwrap();
        assert_eq!(e.codigo, "corrompido");
        assert_eq!(std::fs::read(&c.trabalho).unwrap(), lixo);
    }
}