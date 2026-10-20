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

fn gravacao_falhou(e: impl std::fmt::Display) -> Erro {
    Erro {
        codigo: "gravacao_falhou".into(),
        mensagem: format!(
            "Não foi possível guardar na pasta de dados. Os dados estão seguros neste computador; vou tentar outra vez. ({e})"
        ),
    }
}

fn integridade_ok(conn: &Connection) -> Res<bool> {
    match conn.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0)) {
        Ok(r) => Ok(r == "ok"),
        Err(rusqlite::Error::SqliteFailure(f, _))
            if matches!(f.code, rusqlite::ErrorCode::NotADatabase | rusqlite::ErrorCode::DatabaseCorrupt) =>
        {
            Ok(false)
        }
        Err(e) => Err(e.into()),
    }
}

/// Verifica o banco partilhado sem o modificar.
fn banco_integro(banco: &Path) -> Res<bool> {
    match Connection::open_with_flags(banco, OpenFlags::SQLITE_OPEN_READ_ONLY) {
        Ok(conn) => integridade_ok(&conn),
        Err(rusqlite::Error::SqliteFailure(f, _))
            if matches!(f.code, rusqlite::ErrorCode::NotADatabase | rusqlite::ErrorCode::DatabaseCorrupt) =>
        {
            Ok(false)
        }
        Err(e) => Err(e.into()),
    }
}

pub fn preparar_trabalho(c: &Caminhos) -> Res<Connection> {
    std::fs::create_dir_all(&c.dados)?;
    if let Some(local) = c.trabalho.parent() {
        std::fs::create_dir_all(local)?;
    }
    let pendente = c.pendente.exists() && c.trabalho.exists();
    if !pendente && c.banco.exists() {
        if !banco_integro(&c.banco)? {
            return Err(corrompido());
        }
        std::fs::copy(&c.banco, &c.trabalho)?;
    }
    let mut conn = match Connection::open(&c.trabalho) {
        Ok(conn) => conn,
        Err(rusqlite::Error::SqliteFailure(f, _)) if f.code == rusqlite::ErrorCode::NotADatabase => {
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

pub fn gravar(conn: &Connection, c: &Caminhos) -> Res<()> {
    std::fs::write(&c.pendente, b"").map_err(gravacao_falhou)?;
    let mut tmp = c.banco.clone().into_os_string();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    if tmp.exists() {
        std::fs::remove_file(&tmp).map_err(gravacao_falhou)?;
    }
    let destino = tmp.to_string_lossy().into_owned();
    conn.execute("VACUUM INTO ?1", [destino]).map_err(gravacao_falhou)?;
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
}

