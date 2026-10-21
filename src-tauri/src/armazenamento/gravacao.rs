use crate::erro::{Erro, Res};
use crate::migracoes::migrar;
use chrono::NaiveDateTime;
use rusqlite::{Connection, OpenFlags};
use std::io::Read;
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

/// Código de `preparar_trabalho` quando o `banco` existe mas está danificado.
pub const CODIGO_CORROMPIDO: &str = "corrompido";
/// Código de `preparar_trabalho` quando o `banco` está vazio ou ainda não foi
/// descarregado pelo Google Drive (0 bytes, ilegível, cabeçalho a zeros).
pub const CODIGO_BANCO_VAZIO: &str = "banco_vazio";

fn corrompido() -> Erro {
    Erro {
        campo: None,
        codigo: CODIGO_CORROMPIDO.into(),
        mensagem: "O ficheiro de dados parece estar danificado. Nada foi alterado; é possível restaurar uma cópia de segurança.".into(),
    }
}

fn banco_vazio() -> Erro {
    Erro {
        campo: None,
        codigo: CODIGO_BANCO_VAZIO.into(),
        mensagem: "O ficheiro de dados está vazio ou incompleto. Confirme que a pasta Frota está marcada como \"Disponível offline\" no Google Drive e espere que a sincronização termine. Nada foi alterado.".into(),
    }
}

fn banco_orfao(banco: &Path, orfaos: &[PathBuf]) -> Erro {
    let nomes: Vec<String> = orfaos
        .iter()
        .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect();
    Erro {
        campo: None,
        codigo: CODIGO_BANCO_ORFAO.into(),
        mensagem: format!(
            "O ficheiro de dados ({}) não está na pasta, mas existem ficheiros dele ({}): uma operação anterior ficou a meio. Não foi criada uma base vazia. Veja em Backup (pasta versoes) se há um ficheiro posto de parte e reponha-o como frota.db, ou peça ajuda.",
            banco.display(),
            nomes.join(", ")
        ),
    }
}

fn gravacao_falhou(e: impl std::fmt::Display) -> Erro {
    Erro {
        campo: None,
        codigo: "gravacao_falhou".into(),
        mensagem: format!(
            "Não foi possível guardar na pasta de dados. Os dados estão seguros neste computador; vou tentar outra vez. ({e})"
        ),
    }
}

fn existe(p: &Path) -> Res<bool> {
    Ok(p.try_exists()?)
}

pub(crate) fn com_sufixo(p: &Path, sufixo: &str) -> PathBuf {
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

/// Ficheiros auxiliares do SQLite ao lado de uma base.
const AUXILIARES: [&str; 3] = ["-journal", "-wal", "-shm"];

/// Remove a cópia de trabalho e os ficheiros auxiliares do SQLite.
pub(crate) fn limpar_trabalho(trabalho: &Path, incluir_principal: bool) -> Res<()> {
    for s in AUXILIARES {
        remover_se_existe(&com_sufixo(trabalho, s))?;
    }
    if incluir_principal {
        remover_se_existe(trabalho)?;
    }
    Ok(())
}

fn codigo_corrompido(f: &rusqlite::ffi::Error) -> bool {
    matches!(
        f.code,
        rusqlite::ErrorCode::NotADatabase | rusqlite::ErrorCode::DatabaseCorrupt
    )
}

pub(crate) fn integridade_ok(conn: &Connection) -> Res<bool> {
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

/// A cópia de trabalho abre e passa a verificação de integridade? (Abre em
/// leitura/escrita como o arranque normal, para um journal quente ser revertido.)
/// `Ok(false)` só quando o SQLite diz que está danificada; um erro de leitura
/// (ficheiro em uso, ocupado, sem acesso) é `Err` com `CODIGO_TRABALHO_IO`.
fn trabalho_integro(trabalho: &Path) -> Res<bool> {
    let r = match Connection::open(trabalho) {
        Ok(conn) => integridade_ok(&conn),
        Err(rusqlite::Error::SqliteFailure(f, _)) if codigo_corrompido(&f) => Ok(false),
        Err(e) => Err(e.into()),
    };
    r.map_err(|e| erro_leitura_trabalho(trabalho, &e.mensagem))
}

/// Código de `isolar_trabalho_danificado` quando a cópia de trabalho não se
/// consegue ler (não é o mesmo que estar danificada; tentar de novo pode bastar).
pub const CODIGO_TRABALHO_IO: &str = "trabalho_io";

fn erro_leitura_trabalho(trabalho: &Path, detalhe: &str) -> Erro {
    Erro {
        campo: None,
        codigo: CODIGO_TRABALHO_IO.into(),
        mensagem: format!(
            "Não foi possível ler a cópia local deste PC ({}): {detalhe}. Pode estar em uso por outro programa (p.ex. antivírus ou cópia de segurança). Nada foi alterado; tente de novo.",
            trabalho.display()
        ),
    }
}

/// Código de `mover_base` quando o ficheiro principal foi movido, um auxiliar
/// não, e não foi possível voltar a pôr o principal no lugar.
pub const CODIGO_MOVER_PARCIAL: &str = "mover_parcial";

/// Código de `abrir_trabalho` quando o `banco` não existe mas há ficheiros
/// auxiliares dele (uma operação anterior ficou a meio).
pub const CODIGO_BANCO_ORFAO: &str = "banco_orfao";

/// Estado do ficheiro `banco` partilhado (sem o modificar).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstadoBanco {
    Ausente,
    /// 0 bytes, ilegível (ficheiro só "online" do Google Drive) ou cabeçalho a
    /// zeros (sincronização a meio).
    Vazio,
    Danificado,
    Valido,
}

/// O conteúdo ainda não está disponível neste PC?
fn parece_vazio(banco: &Path) -> Res<bool> {
    if std::fs::metadata(banco)?.len() == 0 {
        return Ok(true);
    }
    let mut cab = Vec::with_capacity(100);
    match std::fs::File::open(banco).and_then(|f| f.take(100).read_to_end(&mut cab)) {
        Ok(_) => Ok(cab.iter().all(|b| *b == 0)),
        // Não se consegue ler o conteúdo (p.ex. ficheiro só "online"): ainda não chegou.
        Err(_) => Ok(true),
    }
}

pub fn estado_banco(banco: &Path) -> Res<EstadoBanco> {
    if !existe(banco)? {
        return Ok(EstadoBanco::Ausente);
    }
    if parece_vazio(banco)? {
        return Ok(EstadoBanco::Vazio);
    }
    Ok(if banco_integro(banco)? {
        EstadoBanco::Valido
    } else {
        EstadoBanco::Danificado
    })
}

/// Cria o marcador de alterações por gravar (idempotente: se já existe não se
/// escreve outra vez).
pub fn marcar_pendente(c: &Caminhos) -> Res<()> {
    if c.pendente.try_exists().unwrap_or(false) {
        return Ok(());
    }
    std::fs::write(&c.pendente, b"").map_err(gravacao_falhou)
}

/// Primeiro nome livre `<base>.db`, `<base>-2.db`, ... em `dir`.
pub(crate) fn nome_livre(dir: &Path, base: &str) -> Res<PathBuf> {
    let mut p = dir.join(format!("{base}.db"));
    let mut n = 2;
    while existe(&p)? {
        p = dir.join(format!("{base}-{n}.db"));
        n += 1;
    }
    Ok(p)
}

/// Move uma base (e os auxiliares do SQLite) para `destino` (por rename).
///
/// Tudo ou nada: se o principal foi movido mas um auxiliar falha, os já movidos
/// e o principal voltam ao lugar e devolve-se o erro original (nada mudou). Se
/// nem isso for possível, devolve `CODIGO_MOVER_PARCIAL` (nada é apagado; quem
/// chama não pode seguir como se o principal não existisse).
pub(crate) fn mover_base(origem: &Path, destino: &Path) -> Res<()> {
    mover_base_com(origem, destino, &|a, b| std::fs::rename(a, b))
}

type Renomear<'a> = &'a dyn Fn(&Path, &Path) -> std::io::Result<()>;

fn mover_base_com(origem: &Path, destino: &Path, renomear: Renomear) -> Res<()> {
    renomear(origem, destino)?;
    let mut movidos: Vec<&str> = Vec::new();
    let mut falha: Option<Erro> = None;
    for s in AUXILIARES {
        let aux = com_sufixo(origem, s);
        let r = match existe(&aux) {
            Ok(false) => continue,
            Ok(true) => renomear(&aux, &com_sufixo(destino, s)).map_err(Erro::from),
            Err(e) => Err(e),
        };
        match r {
            Ok(()) => movidos.push(s),
            Err(e) => {
                falha = Some(e);
                break;
            }
        }
    }
    let Some(e) = falha else { return Ok(()) };
    // Desfazer: auxiliares já movidos e o principal voltam ao lugar.
    let mut desfeito = true;
    for s in movidos.iter().rev() {
        if let Err(e2) = renomear(&com_sufixo(destino, s), &com_sufixo(origem, s)) {
            log::error!("não foi possível repor {}{s}: {e2}", origem.display());
            desfeito = false;
        }
    }
    if let Err(e2) = renomear(destino, origem) {
        log::error!(
            "não foi possível repor {} (ficou em {}): {e2}",
            origem.display(),
            destino.display()
        );
        desfeito = false;
    }
    if desfeito {
        return Err(e);
    }
    Err(Erro {
        campo: None,
        codigo: CODIGO_MOVER_PARCIAL.into(),
        mensagem: format!(
            "Não foi possível mover {} por completo ({}) nem voltar a pô-lo no lugar. O ficheiro pode ter ficado em {}. Nada foi apagado.",
            origem.display(),
            e.mensagem,
            destino.display()
        ),
    })
}

/// Auxiliares do SQLite que existem ao lado de `p` (sem o principal).
pub(crate) fn auxiliares_de(p: &Path) -> Res<Vec<PathBuf>> {
    let mut v = Vec::new();
    for s in AUXILIARES {
        let aux = com_sufixo(p, s);
        if existe(&aux)? {
            v.push(aux);
        }
    }
    Ok(v)
}

/// Alterações por gravar numa cópia de trabalho que já não abre (danificada neste
/// PC): não são dados partilhados danificados. A cópia é posta de lado em
/// `<local>\trabalho_danificado_AAAA-MM-DD_HHMM.db` (com os auxiliares), o
/// `pendente` é removido e o arranque segue a partir do `banco`. Devolve onde
/// ficou o ficheiro, ou `None` se não havia nada a fazer.
pub fn isolar_trabalho_danificado(c: &Caminhos, agora: NaiveDateTime) -> Res<Option<PathBuf>> {
    let ha = existe(&c.pendente)
        .and_then(|p| Ok(p && existe(&c.trabalho)?))
        .map_err(|e| erro_leitura_trabalho(&c.trabalho, &e.mensagem))?;
    // Só uma falha de integridade conta como danificada; erros de leitura sobem.
    if !ha || trabalho_integro(&c.trabalho)? {
        return Ok(None);
    }
    let local = c.trabalho.parent().unwrap_or(Path::new("."));
    let base = format!("trabalho_danificado_{}", agora.format("%Y-%m-%d_%H%M"));
    let destino = nome_livre(local, &base)?;
    mover_base(&c.trabalho, &destino)?;
    remover_se_existe(&c.pendente)?;
    log::warn!(
        "cópia de trabalho danificada com alterações por gravar: guardada em {}",
        destino.display()
    );
    Ok(Some(destino))
}

/// Resultado de abrir a cópia de trabalho.
pub struct Aberto {
    pub conn: Connection,
    /// Havia alterações pendentes e foram gravadas no `banco` durante a abertura.
    pub gravado: bool,
    /// Aviso de uma gravação que correu bem mas não removeu o marcador.
    pub aviso: Option<String>,
}

/// Abre a cópia de trabalho (ver `abrir_trabalho`).
pub fn preparar_trabalho(c: &Caminhos) -> Res<Connection> {
    abrir_trabalho(c).map(|a| a.conn)
}

pub fn abrir_trabalho(c: &Caminhos) -> Res<Aberto> {
    std::fs::create_dir_all(&c.dados)?;
    if let Some(local) = c.trabalho.parent() {
        std::fs::create_dir_all(local)?;
    }
    let pendente = existe(&c.pendente)? && existe(&c.trabalho)?;
    if !pendente {
        match estado_banco(&c.banco)? {
            EstadoBanco::Vazio => return Err(banco_vazio()),
            EstadoBanco::Danificado => return Err(corrompido()),
            EstadoBanco::Valido => {
                limpar_trabalho(&c.trabalho, false)?;
                std::fs::copy(&c.banco, &c.trabalho)?;
            }
            EstadoBanco::Ausente => {
                // Sem banco mas com auxiliares dele: uma operação anterior ficou a
                // meio e o ficheiro pode estar noutro sítio. Nunca criar uma base vazia.
                let orfaos = auxiliares_de(&c.banco)?;
                if !orfaos.is_empty() {
                    return Err(banco_orfao(&c.banco, &orfaos));
                }
                limpar_trabalho(&c.trabalho, true)?
            }
        }
    }
    let mut conn = match Connection::open(&c.trabalho) {
        Ok(conn) => conn,
        Err(rusqlite::Error::SqliteFailure(f, _)) if codigo_corrompido(&f) => {
            return Err(corrompido());
        }
        Err(e) => return Err(e.into()),
    };
    if !integridade_ok(&conn)? {
        return Err(corrompido());
    }
    conn.execute_batch("PRAGMA foreign_keys=ON;")?;
    conn.query_row("PRAGMA journal_mode=DELETE", [], |_| Ok(()))?;
    migrar(&mut conn)?;
    let (gravado, aviso) = if pendente {
        // Falha aqui não impede abrir; o marcador permanece.
        match gravar(&conn, c) {
            Ok(aviso) => (true, aviso),
            Err(_) => (false, None),
        }
    } else {
        (false, None)
    };
    Ok(Aberto {
        conn,
        gravado,
        aviso,
    })
}

fn escrever_tmp(conn: &Connection, tmp: &Path) -> Res<()> {
    if existe(tmp).map_err(|e| gravacao_falhou(e.mensagem))? {
        std::fs::remove_file(tmp).map_err(gravacao_falhou)?;
    }
    let destino = tmp.to_string_lossy().into_owned();
    conn.execute("VACUUM INTO ?1", [destino])
        .map_err(gravacao_falhou)?;
    // Flush para disco antes da substituição atómica.
    std::fs::OpenOptions::new()
        .write(true)
        .open(tmp)
        .and_then(|f| f.sync_all())
        .map_err(gravacao_falhou)?;
    // Nunca substituir um bom ficheiro por um instantâneo mau.
    let chk = Connection::open_with_flags(tmp, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(gravacao_falhou)?;
    let ok = integridade_ok(&chk).map_err(|e| gravacao_falhou(e.mensagem))?;
    drop(chk);
    if !ok {
        return Err(gravacao_falhou(
            "a cópia temporária falhou a verificação de integridade",
        ));
    }
    Ok(())
}

/// Grava `conn` no `banco` (tmp + substituição atómica) e remove o `pendente`.
///
/// `Err` só quando o `banco` NÃO foi substituído. Se a substituição correu bem e
/// só a remoção do marcador falhou, a gravação conta como feita: devolve
/// `Ok(Some(aviso))` (o marcador fica; a próxima gravação volta a tentar).
pub fn gravar(conn: &Connection, c: &Caminhos) -> Res<Option<String>> {
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
    match std::fs::remove_file(&c.pendente) {
        Ok(()) => Ok(None),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => {
            log::warn!("gravado, mas não foi possível remover o marcador pendente: {e}");
            Ok(Some(format!(
                "Os dados foram guardados na pasta, mas não foi possível apagar o marcador de alterações por gravar neste computador ({e}). Vou tentar outra vez na próxima gravação."
            )))
        }
    }
}

#[cfg(test)]
pub(crate) mod testes {
    use super::*;

    fn cenario() -> (tempfile::TempDir, Caminhos) {
        let t = tempfile::tempdir().unwrap();
        let c = Caminhos::new(&t.path().join("raiz"), &t.path().join("local"));
        (t, c)
    }

    fn agora() -> NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(2026, 10, 20)
            .unwrap()
            .and_hms_opt(9, 30, 0)
            .unwrap()
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
        conn.query_row("SELECT COUNT(*) FROM carros", [], |r| r.get(0))
            .unwrap()
    }

    /// Abre p sem partilha de apagamento: enquanto o ficheiro devolvido estiver
    /// aberto, o Windows recusa apagá-lo (simula um ficheiro em uso).
    #[cfg(windows)]
    pub(crate) fn trancar(p: &Path) -> std::fs::File {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_SHARE_READ_WRITE: u32 = 0x1 | 0x2;
        std::fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ_WRITE)
            .open(p)
            .unwrap()
    }

    #[test]
    fn primeira_abertura_cria_banco_vazio_migrado() {
        let (_t, c) = cenario();
        let conn = preparar_trabalho(&c).unwrap();
        assert!(c.dados.exists());
        assert!(c.trabalho.exists());
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM carros", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn gravar_substitui_banco_com_os_dados_novos() {
        let (_t, c) = cenario();
        let conn = preparar_trabalho(&c).unwrap();
        inserir_carro(&conn, "AA-00-AA");
        assert_eq!(gravar(&conn, &c).unwrap(), None);
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

    /// I4: a substituição correu bem, só o marcador não saiu -> não é falha.
    #[cfg(windows)]
    #[test]
    fn marcador_que_nao_sai_e_aviso_e_nao_falha() {
        let (_t, c) = cenario();
        let conn = preparar_trabalho(&c).unwrap();
        inserir_carro(&conn, "AA-00-AA");
        marcar_pendente(&c).unwrap();
        let tranca = trancar(&c.pendente); // em uso: não se consegue apagar
        let r = gravar(&conn, &c);
        drop(tranca);
        let aviso = r.unwrap().expect("aviso");
        assert!(aviso.contains("foram guardados"), "{aviso}");
        assert_eq!(contar(&c.banco), 1);
        assert!(c.pendente.exists());
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
        let a = abrir_trabalho(&c).unwrap();
        assert!(a.gravado);
        let n: i64 = a
            .conn
            .query_row("SELECT COUNT(*) FROM carros", [], |r| r.get(0))
            .unwrap();
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
        assert_eq!(e.codigo, CODIGO_CORROMPIDO);
        assert_eq!(std::fs::read(&c.banco).unwrap(), lixo);
        assert!(!c.trabalho.exists());
        assert_eq!(estado_banco(&c.banco).unwrap(), EstadoBanco::Danificado);
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
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM carros", [], |r| r.get(0))
            .unwrap();
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
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM carros", [], |r| r.get(0))
            .unwrap();
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
        assert_eq!(e.codigo, CODIGO_BANCO_VAZIO);
        assert!(e.mensagem.contains("Disponível offline"));
        assert_eq!(std::fs::metadata(&c.banco).unwrap().len(), 0);
        assert_eq!(contar(&c.trabalho), 1);
    }

    #[test]
    fn banco_com_cabecalho_a_zeros_conta_como_vazio() {
        let (_t, c) = cenario();
        std::fs::create_dir_all(&c.dados).unwrap();
        std::fs::write(&c.banco, vec![0u8; 8192]).unwrap();
        assert_eq!(estado_banco(&c.banco).unwrap(), EstadoBanco::Vazio);
        assert_eq!(
            preparar_trabalho(&c).err().unwrap().codigo,
            CODIGO_BANCO_VAZIO
        );
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
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM carros", [], |r| r.get(0))
            .unwrap();
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
        let a = abrir_trabalho(&c).unwrap();
        assert!(!a.gravado);
        let n: i64 = a
            .conn
            .query_row("SELECT COUNT(*) FROM carros", [], |r| r.get(0))
            .unwrap();
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
        assert_eq!(e.codigo, CODIGO_CORROMPIDO);
        assert_eq!(std::fs::read(&c.trabalho).unwrap(), lixo);
    }

    /// I1: o trabalho danificado vai para o lado (com o journal), o pendente sai e
    /// a abertura segue a partir do banco.
    #[test]
    fn trabalho_danificado_e_isolado_e_abre_do_banco() {
        let (_t, c) = cenario();
        let conn = preparar_trabalho(&c).unwrap();
        inserir_carro(&conn, "AA-00-AA");
        gravar(&conn, &c).unwrap();
        drop(conn);
        let lixo = vec![0xCDu8; 4096];
        std::fs::write(&c.trabalho, &lixo).unwrap();
        std::fs::write(com_sufixo(&c.trabalho, "-journal"), b"j").unwrap();
        std::fs::write(&c.pendente, b"").unwrap();
        let p = isolar_trabalho_danificado(&c, agora()).unwrap().unwrap();
        let local = c.trabalho.parent().unwrap();
        assert_eq!(p, local.join("trabalho_danificado_2026-10-20_0930.db"));
        assert_eq!(std::fs::read(&p).unwrap(), lixo);
        assert!(!c.pendente.exists());
        assert!(!com_sufixo(&c.trabalho, "-journal").exists());
        // segunda vez no mesmo minuto não sobrescreve
        std::fs::write(&c.trabalho, &lixo).unwrap();
        std::fs::write(&c.pendente, b"").unwrap();
        let p2 = isolar_trabalho_danificado(&c, agora()).unwrap().unwrap();
        assert_eq!(p2, local.join("trabalho_danificado_2026-10-20_0930-2.db"));
        let conn = preparar_trabalho(&c).unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM carros", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn trabalho_bom_ou_sem_pendente_nao_e_isolado() {
        let (_t, c) = cenario();
        let conn = preparar_trabalho(&c).unwrap();
        drop(conn);
        std::fs::write(&c.pendente, b"").unwrap();
        assert_eq!(isolar_trabalho_danificado(&c, agora()).unwrap(), None);
        std::fs::remove_file(&c.pendente).unwrap();
        std::fs::write(&c.trabalho, vec![0xCDu8; 4096]).unwrap();
        assert_eq!(isolar_trabalho_danificado(&c, agora()).unwrap(), None);
    }

    // ---------- v0.1.1: renomeações parciais ----------

    /// Base com journal e wal ao lado, em `dir`.
    fn base_com_auxiliares(dir: &Path) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        let p = dir.join("a.db");
        std::fs::write(&p, b"principal").unwrap();
        std::fs::write(com_sufixo(&p, "-journal"), b"j").unwrap();
        std::fs::write(com_sufixo(&p, "-wal"), b"w").unwrap();
        p
    }

    #[test]
    fn mover_base_com_auxiliar_que_falha_volta_tudo_ao_lugar() {
        let (t, _c) = cenario();
        let origem = base_com_auxiliares(&t.path().join("o"));
        let destino = t.path().join("d.db");
        // o journal move-se, o wal falha
        let falhar_wal = |a: &Path, b: &Path| {
            if a.to_string_lossy().ends_with("-wal") {
                Err(std::io::Error::other("wal em uso"))
            } else {
                std::fs::rename(a, b)
            }
        };
        let e = mover_base_com(&origem, &destino, &falhar_wal).unwrap_err();
        assert_eq!(e.codigo, "io");
        assert!(e.mensagem.contains("wal em uso"), "{}", e.mensagem);
        assert_eq!(std::fs::read(&origem).unwrap(), b"principal");
        assert!(com_sufixo(&origem, "-journal").exists());
        assert!(com_sufixo(&origem, "-wal").exists());
        assert!(!destino.exists());
        assert!(!com_sufixo(&destino, "-journal").exists());
    }

    #[test]
    fn mover_base_que_nao_consegue_desfazer_da_erro_parcial() {
        let (t, _c) = cenario();
        let origem = base_com_auxiliares(&t.path().join("o"));
        let destino = t.path().join("d.db");
        // o journal falha e o principal já não consegue voltar
        let falhar = |a: &Path, b: &Path| {
            if a.to_string_lossy().ends_with("-journal") || a == destino.as_path() {
                Err(std::io::Error::other("em uso"))
            } else {
                std::fs::rename(a, b)
            }
        };
        let e = mover_base_com(&origem, &destino, &falhar).unwrap_err();
        assert_eq!(e.codigo, CODIGO_MOVER_PARCIAL);
        assert!(
            e.mensagem.contains(&destino.display().to_string()),
            "{}",
            e.mensagem
        );
        // nada foi apagado
        assert_eq!(std::fs::read(&destino).unwrap(), b"principal");
        assert!(com_sufixo(&origem, "-journal").exists());
    }

    #[test]
    fn mover_base_normal_move_tudo() {
        let (t, _c) = cenario();
        let origem = base_com_auxiliares(&t.path().join("o"));
        let destino = t.path().join("d.db");
        mover_base(&origem, &destino).unwrap();
        assert!(!origem.exists() && destino.exists());
        assert!(com_sufixo(&destino, "-journal").exists());
        assert!(com_sufixo(&destino, "-wal").exists());
        assert!(!com_sufixo(&origem, "-journal").exists());
    }

    /// Depois de uma renomeação parcial do `banco`, o arranque nunca cria uma
    /// base vazia enquanto houver auxiliares dele.
    #[test]
    fn sem_banco_mas_com_auxiliares_dele_nao_cria_base_vazia() {
        let (_t, c) = cenario();
        std::fs::create_dir_all(&c.dados).unwrap();
        std::fs::write(com_sufixo(&c.banco, "-journal"), b"j").unwrap();
        let e = preparar_trabalho(&c).err().unwrap();
        assert_eq!(e.codigo, CODIGO_BANCO_ORFAO);
        assert!(e.mensagem.contains("frota.db-journal"), "{}", e.mensagem);
        assert!(!c.banco.exists());
        assert!(!c.trabalho.exists());
    }

    // ---------- v0.1.1: erro de leitura da cópia de trabalho ----------

    #[test]
    fn trabalho_ilegivel_por_io_nao_conta_como_danificado() {
        let (_t, c) = cenario();
        // um diretório no lugar do trabalho: o SQLite não o consegue abrir (não é "danificado")
        std::fs::create_dir_all(&c.trabalho).unwrap();
        std::fs::write(&c.pendente, b"").unwrap();
        let e = isolar_trabalho_danificado(&c, agora()).unwrap_err();
        assert_eq!(e.codigo, CODIGO_TRABALHO_IO);
        assert!(
            e.mensagem.contains("Não foi possível ler"),
            "{}",
            e.mensagem
        );
        assert!(!e.mensagem.contains("danificad"), "{}", e.mensagem);
        // nada foi mexido
        assert!(c.trabalho.is_dir());
        assert!(c.pendente.exists());
    }
}
