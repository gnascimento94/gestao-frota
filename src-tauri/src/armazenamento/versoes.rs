use crate::armazenamento::gravacao::{Caminhos, gravar, integridade_ok};
use crate::erro::{Erro, Res};
use crate::migracoes::{MIGRACOES, migrar};
use chrono::{DateTime, Local, NaiveDate, NaiveDateTime};
use rusqlite::{Connection, OpenFlags};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const MAX_DIARIAS: usize = 30;
const MAX_ANTES_DA: usize = 5;
const PREFIXO_DATADA: &str = "frota_2";
const PREFIXO_ANTES: &str = "frota_antes-da-";

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Versao {
    pub nome: String,
    pub data: String,
    pub tamanho: u64,
    pub carros: i64,
    pub lancamentos: i64,
}

fn existe(p: &Path) -> Res<bool> {
    Ok(p.try_exists()?)
}

fn pasta(c: &Caminhos) -> Res<PathBuf> {
    let p = c.dados.join("versoes");
    std::fs::create_dir_all(&p)?;
    Ok(p)
}

fn eh_datada(nome: &str) -> bool {
    nome.starts_with(PREFIXO_DATADA) && nome.ends_with(".db")
}

fn eh_antes(nome: &str) -> bool {
    nome.starts_with(PREFIXO_ANTES) && nome.ends_with(".db")
}

/// Ficheiros da pasta que satisfazem `filtro`: (nome, caminho, modificação).
fn ficheiros(dir: &Path, filtro: fn(&str) -> bool) -> Res<Vec<(String, PathBuf, SystemTime)>> {
    let mut v = Vec::new();
    for e in std::fs::read_dir(dir)? {
        let e = e?;
        let nome = e.file_name().to_string_lossy().into_owned();
        if filtro(&nome) && e.file_type()?.is_file() {
            let mtime = e.metadata()?.modified()?;
            v.push((nome, e.path(), mtime));
        }
    }
    Ok(v)
}

fn tmp_de(destino: &Path) -> PathBuf {
    let mut s = destino.as_os_str().to_owned();
    s.push(".tmp");
    PathBuf::from(s)
}

/// Copia o ficheiro `banco` (sempre substituído atomicamente, logo consistente).
fn copiar_banco(c: &Caminhos, destino: &Path) -> Res<()> {
    let tmp = tmp_de(destino);
    let r = std::fs::copy(&c.banco, &tmp).and_then(|_| std::fs::rename(&tmp, destino));
    if r.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    r?;
    Ok(())
}

/// Instantâneo consistente da ligação atual (VACUUM INTO), publicado por rename.
fn snapshot(conn: &Connection, destino: &Path) -> Res<()> {
    let tmp = tmp_de(destino);
    let _ = std::fs::remove_file(&tmp);
    let r = conn
        .execute("VACUUM INTO ?1", [tmp.to_string_lossy().into_owned()])
        .map_err(Erro::from)
        .and_then(|_| std::fs::rename(&tmp, destino).map_err(Erro::from));
    if r.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    r
}

/// `AAAA-MM-DD`, opcionalmente `_HHMM` e `-N` (sufixo anti-colisão) -> (data, hhmm, n).
/// Qualquer nome fora deste formato (incluindo não-ASCII) devolve `None`.
fn parse_datada(nome: &str) -> Option<(String, String, u32)> {
    if !nome.is_ascii() {
        return None;
    }
    let stem = nome.strip_prefix("frota_")?.strip_suffix(".db")?;
    let b = stem.as_bytes();
    if b.len() < 10 {
        return None;
    }
    let padrao_data = |i: usize| match i {
        4 | 7 => b[i] == b'-',
        _ => b[i].is_ascii_digit(),
    };
    if !(0..10).all(padrao_data) {
        return None;
    }
    let data = stem[..10].to_string();
    let resto = &stem[10..];
    if resto.is_empty() {
        return Some((data, String::new(), 0));
    }
    let resto = resto.strip_prefix('_')?;
    if resto.len() < 4 || !resto[..4].bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let hhmm = resto[..4].to_string();
    let n = match &resto[4..] {
        "" => 0,
        s => {
            let d = s.strip_prefix('-')?;
            if d.is_empty() || !d.bytes().all(|c| c.is_ascii_digit()) {
                return None;
            }
            d.parse().ok()?
        }
    };
    Some((data, hhmm, n))
}

/// Chave de ordenação cronológica pelo nome.
fn chave(nome: &str) -> (String, String, u32) {
    parse_datada(nome).unwrap_or_else(|| (nome.to_string(), String::new(), 0))
}

/// Mantém no máximo 30 ficheiros datados (diários + manuais), apagando os mais
/// antigos pelo nome (a data está no nome). Nunca apaga os nomes em `proteger`.
fn podar_datadas(dir: &Path, proteger: &[&str]) -> Res<()> {
    let mut v = ficheiros(dir, eh_datada)?;
    v.sort_by(|a, b| chave(&a.0).cmp(&chave(&b.0)));
    let mut excesso = v.len().saturating_sub(MAX_DIARIAS);
    for (nome, p, _) in v {
        if excesso == 0 {
            break;
        }
        if proteger.contains(&nome.as_str()) {
            continue;
        }
        std::fs::remove_file(p)?;
        excesso -= 1;
    }
    Ok(())
}

/// Mantém no máximo 5 cópias "antes da versão", pela data de modificação.
fn podar_antes(dir: &Path, proteger: &[&str]) -> Res<()> {
    let mut v = ficheiros(dir, eh_antes)?;
    v.sort_by(|a, b| a.2.cmp(&b.2).then_with(|| a.0.cmp(&b.0)));
    let mut excesso = v.len().saturating_sub(MAX_ANTES_DA);
    for (nome, p, _) in v {
        if excesso == 0 {
            break;
        }
        if proteger.contains(&nome.as_str()) {
            continue;
        }
        std::fs::remove_file(p)?;
        excesso -= 1;
    }
    Ok(())
}

/// Cria `frota_AAAA-MM-DD.db` se ainda não existe. Sem banco partilhado não há
/// nada a copiar (devolve `false`).
pub fn copia_diaria(c: &Caminhos, hoje: NaiveDate) -> Res<bool> {
    let dir = pasta(c)?;
    let nome = format!("frota_{}.db", hoje.format("%Y-%m-%d"));
    let destino = dir.join(&nome);
    if existe(&destino)? || !existe(&c.banco)? {
        return Ok(false);
    }
    copiar_banco(c, &destino)?;
    podar_datadas(&dir, &[&nome])?;
    Ok(true)
}

/// `frota_antes-da-v{versao}.db`. Nunca substitui: a primeira cópia anterior à
/// atualização é a limpa, por isso se já existe não se copia nada.
pub fn copia_antes_de(c: &Caminhos, versao: &str) -> Res<()> {
    if versao.is_empty() || versao.contains(['/', '\\']) || versao.contains("..") {
        return Err(versao_invalida());
    }
    let dir = pasta(c)?;
    let nome = format!("{PREFIXO_ANTES}v{versao}.db");
    let destino = dir.join(&nome);
    if existe(&destino)? {
        return Ok(());
    }
    copiar_banco(c, &destino)?;
    podar_antes(&dir, &[&nome])?;
    Ok(())
}

/// Snapshot do estado atual, sem podar. Nunca sobrescreve: se o nome do minuto
/// já existe usa `-2`, `-3`, ... Devolve o nome criado.
fn copia_manual_sem_poda(conn: &Connection, dir: &Path, agora: NaiveDateTime) -> Res<String> {
    let base = format!("frota_{}", agora.format("%Y-%m-%d_%H%M"));
    let mut nome = format!("{base}.db");
    let mut n = 2;
    while existe(&dir.join(&nome))? {
        nome = format!("{base}-{n}.db");
        n += 1;
    }
    snapshot(conn, &dir.join(&nome))?;
    Ok(nome)
}

/// Cópia manual do estado ATUAL da ligação (pode ser mais recente que `banco`).
/// Devolve o nome do ficheiro criado.
pub fn copia_manual(conn: &Connection, c: &Caminhos, agora: NaiveDateTime) -> Res<String> {
    let dir = pasta(c)?;
    let nome = copia_manual_sem_poda(conn, &dir, agora)?;
    podar_datadas(&dir, &[&nome])?;
    Ok(nome)
}

fn contar(conn: &Connection, sql: &str) -> rusqlite::Result<i64> {
    conn.query_row(sql, [], |r| r.get(0))
}

fn contagens(p: &Path) -> (i64, i64) {
    let r: rusqlite::Result<(i64, i64)> = (|| {
        let conn = Connection::open_with_flags(p, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let carros = contar(&conn, "SELECT COUNT(*) FROM carros")?;
        let l = contar(
            &conn,
            "SELECT (SELECT COUNT(*) FROM manutencoes) + (SELECT COUNT(*) FROM pneus) \
             + (SELECT COUNT(*) FROM documentos) + (SELECT COUNT(*) FROM multas) \
             + (SELECT COUNT(*) FROM leituras_km WHERE origem = 'manual')",
        )?;
        Ok((carros, l))
    })();
    r.unwrap_or((-1, -1))
}

/// `AAAA-MM-DD` (diária), `AAAA-MM-DD HH:MM` (manual) ou a modificação (antes-da).
fn data_de(nome: &str, mtime: SystemTime) -> String {
    if let Some((d, h, _)) = parse_datada(nome) {
        return if h.is_empty() {
            d
        } else {
            format!("{d} {}:{}", &h[..2], &h[2..])
        };
    }
    DateTime::<Local>::from(mtime)
        .format("%Y-%m-%d %H:%M")
        .to_string()
}

pub fn listar(c: &Caminhos) -> Res<Vec<Versao>> {
    let dir = pasta(c)?;
    let mut v: Vec<Versao> = Vec::new();
    for (nome, p, mtime) in ficheiros(&dir, |n| eh_datada(n) || eh_antes(n))? {
        let (carros, lancamentos) = contagens(&p);
        v.push(Versao {
            data: data_de(&nome, mtime),
            tamanho: std::fs::metadata(&p)?.len(),
            nome,
            carros,
            lancamentos,
        });
    }
    v.sort_by(|a, b| b.data.cmp(&a.data).then_with(|| b.nome.cmp(&a.nome)));
    Ok(v)
}

fn versao_invalida() -> Erro {
    Erro {
        campo: None,
        codigo: "versao_invalida".into(),
        mensagem: "Versão inválida.".into(),
    }
}

fn corrompida() -> Erro {
    Erro {
        campo: None,
        codigo: "corrompido".into(),
        mensagem: "A versão escolhida parece estar danificada. Nada foi alterado.".into(),
    }
}

/// Repõe uma versão: verifica-a, guarda uma cópia manual do estado atual
/// (se `copia_seguranca`; `false` só quando `conn` não tem dados reais, p.ex. a
/// ligação vazia usada depois de um arranque com o banco corrompido),
/// carrega-a em `conn`, migra e grava.
pub fn restaurar(
    conn: &mut Connection,
    c: &Caminhos,
    nome: &str,
    copia_seguranca: bool,
    agora: NaiveDateTime,
) -> Res<()> {
    if nome.is_empty()
        || nome.contains(['/', '\\'])
        || nome.contains("..")
        || !(parse_datada(nome).is_some() || eh_antes(nome))
    {
        return Err(versao_invalida());
    }
    let dir = pasta(c)?;
    let origem = dir.join(nome);
    if !existe(&origem)? {
        return Err(versao_invalida());
    }
    // 1. verificar antes de tocar em qualquer coisa
    let origem_conn = match Connection::open_with_flags(&origem, OpenFlags::SQLITE_OPEN_READ_ONLY) {
        Ok(x) => x,
        Err(rusqlite::Error::SqliteFailure(f, _))
            if matches!(
                f.code,
                rusqlite::ErrorCode::NotADatabase | rusqlite::ErrorCode::DatabaseCorrupt
            ) =>
        {
            return Err(corrompida());
        }
        Err(e) => return Err(e.into()),
    };
    if !integridade_ok(&origem_conn)? {
        return Err(corrompida());
    }
    // versão criada por uma app mais recente: não a carregamos
    let uv: i64 = origem_conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if uv > MIGRACOES.len() as i64 {
        return Err(Erro { campo: None,
            codigo: "versao_mais_recente".into(),
            mensagem: "Esta versão foi criada por uma versão mais recente da aplicação. Nada foi alterado.".into(),
        });
    }
    // 2. cópia de segurança do estado atual (a poda só acontece no fim)
    let seguranca = if copia_seguranca {
        Some(copia_manual_sem_poda(conn, &dir, agora)?)
    } else {
        None
    };
    // 3. carregar a versão na ligação (API de backup)
    {
        let b = rusqlite::backup::Backup::new(&origem_conn, conn)?;
        b.run_to_completion(64, Duration::ZERO, None)?;
    }
    drop(origem_conn);
    if !integridade_ok(conn)? {
        return Err(Erro {
            campo: None,
            codigo: "corrompido".into(),
            mensagem: "A restauração não passou a verificação de integridade.".into(),
        });
    }
    conn.execute_batch("PRAGMA foreign_keys=ON;")?;
    migrar(conn)?;
    gravar(conn, c)?;
    // 4. só agora se poda, protegendo a cópia de segurança e a versão usada
    match &seguranca {
        Some(s) => podar_datadas(&dir, &[s, nome])?,
        None => podar_datadas(&dir, &[nome])?,
    }
    podar_antes(&dir, &[nome])?;
    Ok(())
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::armazenamento::gravacao::{gravar, preparar_trabalho};
    use chrono::NaiveDate;
    use std::fs::FileTimes;
    use std::time::{Duration, SystemTime};

    fn cenario() -> (tempfile::TempDir, Caminhos, Connection) {
        let t = tempfile::tempdir().unwrap();
        let c = Caminhos::new(&t.path().join("raiz"), &t.path().join("local"));
        let conn = preparar_trabalho(&c).unwrap();
        gravar(&conn, &c).unwrap();
        (t, c, conn)
    }

    fn inserir_carro(conn: &Connection, matricula: &str) {
        conn.execute(
            "INSERT INTO carros (matricula, marca, modelo, km_inicial, estado) VALUES (?1, 'Fiat', 'Ducato', 0, 'ativo')",
            [matricula],
        )
        .unwrap();
    }

    fn dia(n: u64) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 1, 1).unwrap() + chrono::Days::new(n)
    }

    fn nomes(c: &Caminhos, prefixo: &str) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(c.dados.join("versoes"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with(prefixo))
            .collect();
        v.sort();
        v
    }

    #[test]
    fn copia_diaria_so_uma_por_dia() {
        let (_t, c, _conn) = cenario();
        assert!(copia_diaria(&c, dia(0)).unwrap());
        assert!(!copia_diaria(&c, dia(0)).unwrap());
        assert_eq!(nomes(&c, "frota_2"), vec!["frota_2026-01-01.db"]);
    }

    #[test]
    fn guarda_so_30_diarias() {
        let (_t, c, _conn) = cenario();
        for i in 0..32 {
            assert!(copia_diaria(&c, dia(i)).unwrap());
        }
        let n = nomes(&c, "frota_2");
        assert_eq!(n.len(), 30);
        assert_eq!(n[0], "frota_2026-01-03.db");
        assert_eq!(n[29], "frota_2026-02-01.db");
    }

    #[test]
    fn guarda_so_5_antes_da() {
        let (_t, c, _conn) = cenario();
        let base = SystemTime::now() - Duration::from_secs(1000);
        for i in 1..=6u64 {
            copia_antes_de(&c, &format!("1.0.{i}")).unwrap();
            // mtime crescente e determinístico
            let f = std::fs::OpenOptions::new()
                .write(true)
                .open(
                    c.dados
                        .join("versoes")
                        .join(format!("frota_antes-da-v1.0.{i}.db")),
                )
                .unwrap();
            f.set_times(FileTimes::new().set_modified(base + Duration::from_secs(i * 10)))
                .unwrap();
        }
        let n = nomes(&c, "frota_antes-da-");
        assert_eq!(n.len(), 5);
        assert!(!n.contains(&"frota_antes-da-v1.0.1.db".to_string()));
    }

    #[test]
    fn antes_da_nova_nunca_e_podada_mesmo_com_mtime_antigo() {
        let (_t, c, _conn) = cenario();
        // as 5 existentes ficam com mtime no futuro: a nova será a mais antiga
        let futuro = SystemTime::now() + Duration::from_secs(10_000);
        for i in 1..=5u64 {
            copia_antes_de(&c, &format!("1.0.{i}")).unwrap();
            let f = std::fs::OpenOptions::new()
                .write(true)
                .open(
                    c.dados
                        .join("versoes")
                        .join(format!("frota_antes-da-v1.0.{i}.db")),
                )
                .unwrap();
            f.set_modified(futuro + Duration::from_secs(i)).unwrap();
        }
        copia_antes_de(&c, "1.0.6").unwrap();
        let n = nomes(&c, "frota_antes-da-");
        assert_eq!(n.len(), 5);
        assert!(n.contains(&"frota_antes-da-v1.0.6.db".to_string()));
    }

    #[test]
    fn manual_conta_para_as_30_e_nunca_apaga_a_propria() {
        let (_t, c, conn) = cenario();
        for i in 0..30 {
            copia_diaria(&c, dia(i)).unwrap();
        }
        // uma cópia manual com data mais antiga que todas as diárias
        let velho = NaiveDate::from_ymd_opt(2020, 1, 1)
            .unwrap()
            .and_hms_opt(9, 5, 0)
            .unwrap();
        copia_manual(&conn, &c, velho).unwrap();
        let n = nomes(&c, "frota_2");
        assert_eq!(n.len(), 30);
        assert!(n.contains(&"frota_2020-01-01_0905.db".to_string()));
    }

    #[test]
    fn copia_manual_usa_o_estado_atual_da_ligacao() {
        let (_t, c, conn) = cenario();
        inserir_carro(&conn, "AA-00-AA");
        let t = dia(0).and_hms_opt(15, 30, 0).unwrap();
        copia_manual(&conn, &c, t).unwrap();
        let l = listar(&c).unwrap();
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].nome, "frota_2026-01-01_1530.db");
        assert_eq!(l[0].data, "2026-01-01 15:30");
        assert_eq!(l[0].carros, 1);
        assert!(l[0].tamanho > 0);
    }

    #[test]
    fn listar_ordena_recente_primeiro_conta_lancamentos_e_tolera_corrompida() {
        let (_t, c, conn) = cenario();
        inserir_carro(&conn, "AA-00-AA");
        conn.execute(
            "INSERT INTO leituras_km (carro_id, data, km, origem) VALUES (1, '2026-01-01', 10, 'manual')",
            [],
        )
        .unwrap();
        gravar(&conn, &c).unwrap();
        copia_diaria(&c, dia(0)).unwrap();
        copia_diaria(&c, dia(1)).unwrap();
        std::fs::write(
            c.dados.join("versoes").join("frota_2025-12-31.db"),
            vec![0xABu8; 4096],
        )
        .unwrap();
        let l = listar(&c).unwrap();
        let ns: Vec<_> = l.iter().map(|v| v.nome.as_str()).collect();
        assert_eq!(
            ns,
            vec![
                "frota_2026-01-02.db",
                "frota_2026-01-01.db",
                "frota_2025-12-31.db"
            ]
        );
        assert_eq!(l[0].data, "2026-01-02");
        assert_eq!((l[0].carros, l[0].lancamentos), (1, 1));
        assert_eq!((l[2].carros, l[2].lancamentos), (-1, -1));
    }

    #[test]
    fn restaurar_faz_copia_do_atual_e_repoe() {
        let (_t, c, mut conn) = cenario();
        inserir_carro(&conn, "AA-00-AA");
        let t1 = dia(0).and_hms_opt(10, 0, 0).unwrap();
        copia_manual(&conn, &c, t1).unwrap();
        inserir_carro(&conn, "BB-11-BB");
        let t2 = dia(0).and_hms_opt(11, 0, 0).unwrap();
        restaurar(&mut conn, &c, "frota_2026-01-01_1000.db", true, t2).unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM carros", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
        let seg = listar(&c)
            .unwrap()
            .into_iter()
            .find(|v| v.nome == "frota_2026-01-01_1100.db")
            .unwrap();
        assert_eq!(seg.carros, 2);
        // e foi gravado no banco partilhado
        let b = Connection::open(&c.banco).unwrap();
        let n: i64 = b
            .query_row("SELECT COUNT(*) FROM carros", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
        assert!(!c.pendente.exists());
    }

    #[test]
    fn restaurar_sem_copia_de_seguranca_nao_cria_versao() {
        let (_t, c, conn) = cenario();
        inserir_carro(&conn, "AA-00-AA");
        let t1 = dia(0).and_hms_opt(10, 0, 0).unwrap();
        assert_eq!(
            copia_manual(&conn, &c, t1).unwrap(),
            "frota_2026-01-01_1000.db"
        );
        let mut vazia = Connection::open_in_memory().unwrap();
        let t2 = dia(0).and_hms_opt(11, 0, 0).unwrap();
        restaurar(&mut vazia, &c, "frota_2026-01-01_1000.db", false, t2).unwrap();
        let ns: Vec<_> = listar(&c).unwrap().into_iter().map(|v| v.nome).collect();
        assert_eq!(ns, vec!["frota_2026-01-01_1000.db"]);
        let n: i64 = vazia
            .query_row("SELECT COUNT(*) FROM carros", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn duas_restauracoes_no_mesmo_minuto_nao_perdem_o_estado() {
        let (_t, c, mut conn) = cenario();
        inserir_carro(&conn, "AA-00-AA");
        copia_manual(&conn, &c, dia(0).and_hms_opt(10, 0, 0).unwrap()).unwrap(); // X: 1 carro
        inserir_carro(&conn, "BB-11-BB");
        copia_manual(&conn, &c, dia(0).and_hms_opt(10, 1, 0).unwrap()).unwrap(); // Y: 2 carros
        inserir_carro(&conn, "CC-22-CC"); // estado B: 3 carros
        let t1 = dia(0).and_hms_opt(11, 0, 10).unwrap();
        let t2 = dia(0).and_hms_opt(11, 0, 40).unwrap();
        restaurar(&mut conn, &c, "frota_2026-01-01_1000.db", true, t1).unwrap();
        restaurar(&mut conn, &c, "frota_2026-01-01_1001.db", true, t2).unwrap();
        let l = listar(&c).unwrap();
        let a = l
            .iter()
            .find(|v| v.nome == "frota_2026-01-01_1100.db")
            .unwrap();
        let b = l
            .iter()
            .find(|v| v.nome == "frota_2026-01-01_1100-2.db")
            .unwrap();
        assert_eq!(a.carros, 3); // estado B preservado
        assert_eq!(b.carros, 1); // estado após a 1.ª restauração
        assert_eq!(b.data, "2026-01-01 11:00");
        // e o sufixo é restaurável
        restaurar(
            &mut conn,
            &c,
            "frota_2026-01-01_1100-2.db",
            true,
            dia(0).and_hms_opt(11, 5, 0).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn restaurar_a_mais_antiga_com_30_copias() {
        let (_t, c, mut conn) = cenario();
        inserir_carro(&conn, "AA-00-AA");
        gravar(&conn, &c).unwrap();
        for i in 0..30 {
            copia_diaria(&c, dia(i)).unwrap();
        }
        inserir_carro(&conn, "BB-11-BB");
        restaurar(
            &mut conn,
            &c,
            "frota_2026-01-01.db",
            true,
            dia(40).and_hms_opt(9, 0, 0).unwrap(),
        )
        .unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM carros", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
        let ns = nomes(&c, "frota_2");
        assert_eq!(ns.len(), 30);
        assert!(ns.contains(&"frota_2026-01-01.db".to_string())); // a origem sobrevive
        assert!(ns.contains(&"frota_2026-02-10_0900.db".to_string())); // a segurança também
        assert!(!ns.contains(&"frota_2026-01-02.db".to_string())); // a mais antiga das outras sai
    }

    #[test]
    fn nomes_estranhos_nao_fazem_panico_no_listar() {
        let (_t, c, _conn) = cenario();
        copia_diaria(&c, dia(0)).unwrap();
        let v = c.dados.join("versoes");
        for n in [
            "frota_2é.db",
            "frota_2026-01-01_é.db",
            "frota_2.db",
            "frota_2026-01-0_1230.db",
            "frota_2026-01-01_12.db",
            "frota_2026-01-01_1230-x.db",
        ] {
            std::fs::write(v.join(n), b"x").unwrap();
        }
        let l = listar(&c).unwrap();
        assert_eq!(l.len(), 7);
    }

    #[test]
    fn restaurar_versao_de_esquema_mais_recente_e_recusada() {
        let (_t, c, mut conn) = cenario();
        inserir_carro(&conn, "AA-00-AA");
        copia_manual(&conn, &c, dia(0).and_hms_opt(10, 0, 0).unwrap()).unwrap();
        let p = c.dados.join("versoes").join("frota_2026-01-01_1000.db");
        Connection::open(&p)
            .unwrap()
            .execute_batch("PRAGMA user_version=99")
            .unwrap();
        let e = restaurar(
            &mut conn,
            &c,
            "frota_2026-01-01_1000.db",
            true,
            dia(1).and_hms_opt(10, 0, 0).unwrap(),
        )
        .unwrap_err();
        assert_eq!(e.codigo, "versao_mais_recente");
        assert_eq!(nomes(&c, "frota_2").len(), 1);
    }

    #[test]
    fn antes_da_existente_nao_e_substituida() {
        let (_t, c, conn) = cenario();
        copia_antes_de(&c, "1.0.0").unwrap();
        inserir_carro(&conn, "AA-00-AA");
        gravar(&conn, &c).unwrap();
        copia_antes_de(&c, "1.0.0").unwrap();
        let l = listar(&c).unwrap();
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].carros, 0);
    }

    #[test]
    fn restaurar_recusa_nomes_perigosos_sem_alterar_nada() {
        let (_t, c, mut conn) = cenario();
        inserir_carro(&conn, "AA-00-AA");
        let t = dia(0).and_hms_opt(10, 0, 0).unwrap();
        for nome in [
            "..\\frota.db",
            "../frota.db",
            "a/b.db",
            "",
            "..",
            "inexistente.db",
        ] {
            let e = restaurar(&mut conn, &c, nome, true, t).unwrap_err();
            assert_eq!(e.codigo, "versao_invalida", "{nome}");
        }
        assert!(listar(&c).unwrap().is_empty());
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM carros", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn restaurar_versao_corrompida_nao_toca_em_nada() {
        let (_t, c, mut conn) = cenario();
        inserir_carro(&conn, "AA-00-AA");
        std::fs::create_dir_all(c.dados.join("versoes")).unwrap();
        std::fs::write(
            c.dados.join("versoes").join("frota_2025-12-31.db"),
            vec![0xABu8; 4096],
        )
        .unwrap();
        let t = dia(0).and_hms_opt(10, 0, 0).unwrap();
        let e = restaurar(&mut conn, &c, "frota_2025-12-31.db", true, t).unwrap_err();
        assert_eq!(e.codigo, "corrompido");
        assert_eq!(nomes(&c, "frota_2").len(), 1); // sem cópia de segurança
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM carros", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
    }
}
