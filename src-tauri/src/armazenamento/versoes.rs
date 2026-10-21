use crate::armazenamento::gravacao::{Caminhos, gravar, integridade_ok, mover_base, nome_livre};
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
/// `frota_danificado_AAAA-MM-DD_HHMM.db`: o `banco` danificado (falha a
/// verificação de integridade) posto de lado antes de um restauro no arranque.
pub const PREFIXO_DANIFICADO: &str = "frota_danificado_";
/// `frota_posto_de_parte_AAAA-MM-DD_HHMM.db`: o `banco` vazio ou ilegível (p.ex.
/// ainda não descarregado pelo Google Drive) posto de lado antes de um restauro
/// no arranque. Pode vir a conter dados bons.
pub const PREFIXO_POSTO_DE_PARTE: &str = "frota_posto_de_parte_";
/// `frota_legado_AAAA-MM-DD_HHMM.db`: alterações por gravar da pasta local
/// antiga, comprovadamente desta pasta de dados, que não puderam ser migradas.
pub const PREFIXO_LEGADO: &str = "frota_legado_";

/// Ficheiros postos de lado: nunca são podados nem contam para as 30. Só são
/// restauráveis se passarem a verificação (ver `Versao::restauravel`).
const PREFIXOS_POSTOS_DE_LADO: [&str; 3] =
    [PREFIXO_DANIFICADO, PREFIXO_POSTO_DE_PARTE, PREFIXO_LEGADO];

/// `frota_guardada_<original sem "frota_" nem ".db">__<nome>.db`: uma cópia a
/// que o utilizador deu um nome. O nome e a proteção vão no próprio ficheiro
/// (sincronizam juntos): nenhuma poda a reconhece, logo nunca é apagada
/// automaticamente nem conta para as 30 / 5.
const PREFIXO_GUARDADA: &str = "frota_guardada_";
const MAX_ROTULO: usize = 60;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Versao {
    pub nome: String,
    pub data: String,
    pub tamanho: u64,
    pub carros: i64,
    pub lancamentos: i64,
    /// A UI pode oferecer o restauro. Cópias diárias/manuais/antes-da: sempre
    /// (o restauro verifica-as). Ficheiros postos de lado: só se passarem a
    /// verificação de integridade e de esquema.
    pub restauravel: bool,
    /// Nome dado pelo utilizador (a cópia deixa de ser apagada automaticamente).
    pub rotulo: Option<String>,
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

fn eh_posto_de_lado(nome: &str) -> bool {
    nome.ends_with(".db") && PREFIXOS_POSTOS_DE_LADO.iter().any(|p| nome.starts_with(p))
}

/// Cópia diária/manual, antes-da ou posta de lado (sem nome dado).
fn eh_reconhecida(nome: &str) -> bool {
    parse_datada(nome).is_some() || eh_antes(nome) || eh_posto_de_lado(nome)
}

/// `frota_guardada_<orig>__<nome>.db` -> (nome do ficheiro original, nome dado).
fn partes_guardada(nome: &str) -> Option<(String, &str)> {
    let resto = nome.strip_prefix(PREFIXO_GUARDADA)?.strip_suffix(".db")?;
    let (orig, rotulo) = resto.split_once("__")?;
    let original = format!("frota_{orig}.db");
    (!rotulo.is_empty() && eh_reconhecida(&original)).then_some((original, rotulo))
}

fn eh_guardada(nome: &str) -> bool {
    partes_guardada(nome).is_some()
}

/// Nome do ficheiro antes de lhe darem um nome (o próprio, se não tem).
fn original_de(nome: &str) -> String {
    partes_guardada(nome).map_or_else(|| nome.to_string(), |(o, _)| o)
}

/// A poda é uma arrumação: um erro aqui fica no registo e nunca transforma uma
/// cópia ou um restauro bem-sucedidos num erro.
fn podar_sem_falhar(r: Res<()>) {
    if let Err(e) = r {
        log::warn!("não foi possível podar as versões antigas: {}", e.mensagem);
    }
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
/// As cópias com nome (`frota_guardada_*`) não são datadas: ficam de fora.
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
    podar_sem_falhar(podar_datadas(&dir, &[&nome]));
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
    podar_sem_falhar(podar_antes(&dir, &[&nome]));
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
    podar_sem_falhar(podar_datadas(&dir, &[&nome]));
    Ok(nome)
}

/// Antes de um restauro no arranque: move o `banco` atual para `versoes` (nunca
/// sobrescreve): `frota_danificado_AAAA-MM-DD_HHMM.db` se está danificado,
/// `frota_posto_de_parte_AAAA-MM-DD_HHMM.db` se está vazio/ilegível (`vazio`).
/// Sem `banco` não há nada a guardar (`None`). Devolve o nome criado. Um erro
/// garante que o `banco` ficou no lugar, salvo `CODIGO_MOVER_PARCIAL`.
pub fn guardar_danificado(c: &Caminhos, agora: NaiveDateTime, vazio: bool) -> Res<Option<String>> {
    if !existe(&c.banco)? {
        return Ok(None);
    }
    let dir = pasta(c)?;
    let prefixo = if vazio {
        PREFIXO_POSTO_DE_PARTE
    } else {
        PREFIXO_DANIFICADO
    };
    let base = format!("{prefixo}{}", agora.format("%Y-%m-%d_%H%M"));
    let destino = nome_livre(&dir, &base)?;
    mover_base(&c.banco, &destino)?;
    Ok(destino
        .file_name()
        .map(|n| n.to_string_lossy().into_owned()))
}

/// Alterações da pasta local antiga comprovadamente desta pasta de dados:
/// snapshot de `conn` em `versoes\frota_legado_AAAA-MM-DD_HHMM.db` (nunca
/// sobrescreve nem é podado). Devolve o nome criado.
pub fn copia_legado(conn: &Connection, c: &Caminhos, agora: NaiveDateTime) -> Res<String> {
    let dir = pasta(c)?;
    let base = format!("{PREFIXO_LEGADO}{}", agora.format("%Y-%m-%d_%H%M"));
    let destino = nome_livre(&dir, &base)?;
    snapshot(conn, &destino)?;
    Ok(destino
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default())
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
    let datado = PREFIXOS_POSTOS_DE_LADO
        .iter()
        .find_map(|p| nome.strip_prefix(p))
        .map(|resto| format!("frota_{resto}"));
    if let Some((d, h, _)) = parse_datada(datado.as_deref().unwrap_or(nome)) {
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
    let filtro = |n: &str| eh_datada(n) || eh_antes(n) || eh_posto_de_lado(n) || eh_guardada(n);
    for (nome, p, mtime) in ficheiros(&dir, filtro)? {
        let (carros, lancamentos) = contagens(&p);
        let original = original_de(&nome);
        let restauravel = !eh_posto_de_lado(&original) || verificar(&p).is_ok();
        v.push(Versao {
            data: data_de(&original, mtime),
            tamanho: std::fs::metadata(&p)?.len(),
            rotulo: partes_guardada(&nome).map(|(_, r)| r.to_string()),
            nome,
            carros,
            lancamentos,
            restauravel,
        });
    }
    v.sort_by(|a, b| b.data.cmp(&a.data).then_with(|| b.nome.cmp(&a.nome)));
    Ok(v)
}

/// Nome de ficheiro de uma versão que existe em `dir` (nunca sai da pasta).
fn origem_valida(dir: &Path, nome: &str) -> Res<PathBuf> {
    if nome.is_empty()
        || nome.contains(['/', '\\'])
        || nome.contains("..")
        || !(eh_reconhecida(nome) || eh_guardada(nome))
    {
        return Err(versao_invalida());
    }
    let p = dir.join(nome);
    if !existe(&p)? {
        return Err(versao_invalida());
    }
    Ok(p)
}

fn erro_rotulo(codigo: &str, mensagem: String) -> Erro {
    Erro {
        campo: Some("rotulo".into()),
        codigo: codigo.into(),
        mensagem,
    }
}

/// Espaços colapsados; recusa o que não pode ir num nome de ficheiro do Windows.
fn rotulo_valido(rotulo: &str) -> Res<String> {
    let r = rotulo.split_whitespace().collect::<Vec<_>>().join(" ");
    if r.chars().count() > MAX_ROTULO {
        return Err(erro_rotulo(
            "rotulo_longo",
            format!("O nome pode ter no máximo {MAX_ROTULO} caracteres."),
        ));
    }
    // ".." e um ponto final dariam "..db": o nome do ficheiro seria recusado.
    if r.contains("..")
        || r.ends_with('.')
        || r.chars().any(|ch| {
            ch.is_control()
                || r#"<>:"/\|?*"#.contains(ch)
                // carateres invisíveis/de direção (p.ex. RTL override)
                || matches!(ch, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}')
        })
    {
        return Err(erro_rotulo(
            "rotulo_invalido",
            r#"O nome não pode ter os caracteres < > : " / \ | ? * nem ".." nem terminar em ponto."#
                .into(),
        ));
    }
    Ok(r)
}

/// Dá, muda ou tira (`rotulo` vazio) o nome de uma versão, mudando o nome do
/// ficheiro (`frota_guardada_*`). Com nome, a versão nunca é apagada
/// automaticamente. Devolve o novo nome do ficheiro.
pub fn nomear(c: &Caminhos, nome: &str, rotulo: &str) -> Res<String> {
    let dir = pasta(c)?;
    let origem = origem_valida(&dir, nome)?;
    let rotulo = rotulo_valido(rotulo)?;
    let original = original_de(nome);
    let destino = if rotulo.is_empty() {
        original.clone()
    } else {
        let stem = original
            .strip_prefix("frota_")
            .and_then(|s| s.strip_suffix(".db"))
            .ok_or_else(versao_invalida)?;
        format!("{PREFIXO_GUARDADA}{stem}__{rotulo}.db")
    };
    if destino == nome {
        return Ok(destino);
    }
    // `rename` no Windows substitui o destino: nunca por cima de outra cópia
    // (salvo o próprio ficheiro, numa mudança só de maiúsculas).
    // Caminhos acima de 260 caracteres falham no SQLite (contagens/restauro).
    if dir.join(&destino).as_os_str().len() > 250 {
        return Err(erro_rotulo(
            "rotulo_longo",
            "O nome é demasiado longo para esta pasta. Escolha um nome mais curto.".into(),
        ));
    }
    if destino.to_lowercase() != nome.to_lowercase() && existe(&dir.join(&destino))? {
        return Err(erro_rotulo(
            "nome_ocupado",
            format!("Já existe uma cópia com o nome de ficheiro «{destino}». Nada foi alterado."),
        ));
    }
    std::fs::rename(origem, dir.join(&destino))?;
    Ok(destino)
}

/// Apaga uma versão. Não se desfaz.
pub fn apagar(c: &Caminhos, nome: &str) -> Res<()> {
    let dir = pasta(c)?;
    let p = origem_valida(&dir, nome)?;
    std::fs::remove_file(p)?;
    Ok(())
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

/// Abre `p` só para leitura e verifica-o: integridade e esquema (não mais
/// recente do que esta aplicação). Devolve a ligação aberta.
fn verificar(p: &Path) -> Res<Connection> {
    // 0 bytes abre como uma base vazia válida: não é uma versão com dados.
    if std::fs::metadata(p)?.len() == 0 {
        return Err(corrompida());
    }
    let conn = match Connection::open_with_flags(p, OpenFlags::SQLITE_OPEN_READ_ONLY) {
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
    if !integridade_ok(&conn)? {
        return Err(corrompida());
    }
    // versão criada por uma app mais recente: não a carregamos
    let uv: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if uv < 0 {
        return Err(corrompida());
    }
    if uv > MIGRACOES.len() as i64 {
        return Err(Erro { campo: None,
            codigo: "versao_mais_recente".into(),
            mensagem: "Esta versão foi criada por uma versão mais recente da aplicação. Nada foi alterado.".into(),
        });
    }
    Ok(conn)
}

/// Repõe uma versão: verifica-a, guarda uma cópia manual do estado atual
/// (se `copia_seguranca`; `false` só quando `conn` não tem dados reais, p.ex. a
/// ligação vazia usada depois de um arranque com o banco corrompido),
/// carrega-a em `conn`, migra e grava. Devolve o aviso de `gravar` (gravado mas
/// o marcador ficou). A poda das versões antigas é feita sem falhar.
pub fn restaurar(
    conn: &mut Connection,
    c: &Caminhos,
    nome: &str,
    copia_seguranca: bool,
    agora: NaiveDateTime,
) -> Res<Option<String>> {
    let dir = pasta(c)?;
    let origem = origem_valida(&dir, nome)?;
    // 1. verificar antes de tocar em qualquer coisa (também os postos de lado:
    // um `frota_posto_de_parte_*` pode ter dados bons)
    let origem_conn = verificar(&origem)?;
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
    let aviso = gravar(conn, c)?;
    // 4. só agora se poda, protegendo a cópia de segurança e a versão usada
    podar_sem_falhar(match &seguranca {
        Some(s) => podar_datadas(&dir, &[s, nome]),
        None => podar_datadas(&dir, &[nome]),
    });
    podar_sem_falhar(podar_antes(&dir, &[nome]));
    Ok(aviso)
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

    #[test]
    fn danificado_fica_listado_nao_e_podado_nem_restauravel() {
        let (_t, c, mut conn) = cenario();
        for i in 0..30 {
            copia_diaria(&c, dia(i)).unwrap();
        }
        std::fs::write(&c.banco, vec![0xABu8; 4096]).unwrap();
        let t = dia(40).and_hms_opt(9, 5, 0).unwrap();
        let nome = guardar_danificado(&c, t, false).unwrap().unwrap();
        assert_eq!(nome, "frota_danificado_2026-02-10_0905.db");
        assert!(!c.banco.exists());
        // segunda vez no mesmo minuto: sufixo, nunca sobrescreve
        std::fs::write(&c.banco, vec![0xABu8; 4096]).unwrap();
        assert_eq!(
            guardar_danificado(&c, t, false).unwrap().unwrap(),
            "frota_danificado_2026-02-10_0905-2.db"
        );
        // sem banco: nada a guardar
        assert_eq!(guardar_danificado(&c, t, false).unwrap(), None);
        // não conta para as 30 nem é apagado pela poda
        copia_diaria(&c, dia(41)).unwrap_or(false);
        copia_manual(&conn, &c, dia(42).and_hms_opt(8, 0, 0).unwrap()).unwrap();
        assert_eq!(nomes(&c, "frota_2").len(), 30);
        assert_eq!(nomes(&c, "frota_danificado_").len(), 2);
        let l = listar(&c).unwrap();
        let d = l.iter().find(|v| v.nome == nome).unwrap();
        assert_eq!(d.data, "2026-02-10 09:05");
        assert_eq!((d.carros, d.lancamentos), (-1, -1));
        assert!(!d.restauravel);
        // não passa a verificação: recusado sem cópia de segurança nem alterações
        let antes = nomes(&c, "frota_2").len();
        let e = restaurar(&mut conn, &c, &nome, true, t).unwrap_err();
        assert_eq!(e.codigo, "corrompido");
        assert_eq!(nomes(&c, "frota_2").len(), antes);
        // as cópias normais continuam restauráveis
        assert!(
            l.iter()
                .filter(|v| v.nome.starts_with("frota_2"))
                .all(|v| v.restauravel)
        );
    }

    /// v0.1.1: um ficheiro posto de parte (banco vazio no arranque) que entretanto
    /// tem dados bons é listado como restaurável e restaura com cópia de segurança.
    #[test]
    fn posto_de_parte_com_dados_bons_e_restauravel() {
        let (_t, c, mut conn) = cenario();
        inserir_carro(&conn, "AA-00-AA");
        inserir_carro(&conn, "BB-11-BB");
        gravar(&conn, &c).unwrap();
        let t = dia(0).and_hms_opt(9, 0, 0).unwrap();
        // o "vazio" era um ficheiro ainda por descarregar: depois de posto de parte tem os dados
        let nome = guardar_danificado(&c, t, true).unwrap().unwrap();
        assert_eq!(nome, "frota_posto_de_parte_2026-01-01_0900.db");
        assert!(!c.banco.exists());
        // um posto de parte ilegível no mesmo minuto não é restaurável
        std::fs::write(&c.banco, b"").unwrap();
        let ilegivel = guardar_danificado(&c, t, true).unwrap().unwrap();
        assert_eq!(ilegivel, "frota_posto_de_parte_2026-01-01_0900-2.db");
        let l = listar(&c).unwrap();
        let bom = l.iter().find(|v| v.nome == nome).unwrap();
        assert!(bom.restauravel);
        assert_eq!(bom.data, "2026-01-01 09:00");
        assert_eq!(bom.carros, 2);
        assert!(!l.iter().find(|v| v.nome == ilegivel).unwrap().restauravel);
        // estado atual com 1 carro; restaurar o posto de parte (2 carros)
        conn.execute("DELETE FROM carros WHERE matricula = 'BB-11-BB'", [])
            .unwrap();
        let t2 = dia(0).and_hms_opt(10, 0, 0).unwrap();
        restaurar(&mut conn, &c, &nome, true, t2).unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM carros", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 2);
        // a cópia de segurança do estado anterior foi feita
        let seg = listar(&c)
            .unwrap()
            .into_iter()
            .find(|v| v.nome == "frota_2026-01-01_1000.db")
            .unwrap();
        assert_eq!(seg.carros, 1);
        // o posto de parte continua lá (não é podado nem consumido)
        assert!(c.dados.join("versoes").join(&nome).exists());
        let e = restaurar(&mut conn, &c, &ilegivel, true, t2).unwrap_err();
        assert_eq!(e.codigo, "corrompido");
    }

    #[test]
    fn danificado_que_passa_a_verificacao_e_restauravel() {
        let (_t, c, conn) = cenario();
        inserir_carro(&conn, "AA-00-AA");
        gravar(&conn, &c).unwrap();
        let t = dia(0).and_hms_opt(9, 0, 0).unwrap();
        let nome = guardar_danificado(&c, t, false).unwrap().unwrap();
        let v = listar(&c).unwrap();
        assert!(v.iter().find(|x| x.nome == nome).unwrap().restauravel);
        let mut vazia = Connection::open_in_memory().unwrap();
        restaurar(&mut vazia, &c, &nome, false, t).unwrap();
        assert!(c.banco.exists());
    }

    #[test]
    fn legado_e_listado_restauravel_e_nunca_podado() {
        let (_t, c, conn) = cenario();
        inserir_carro(&conn, "AA-00-AA");
        let t = dia(0).and_hms_opt(9, 0, 0).unwrap();
        let nome = copia_legado(&conn, &c, t).unwrap();
        assert_eq!(nome, "frota_legado_2026-01-01_0900.db");
        assert_eq!(
            copia_legado(&conn, &c, t).unwrap(),
            "frota_legado_2026-01-01_0900-2.db"
        );
        for i in 0..31 {
            copia_diaria(&c, dia(i + 1)).unwrap();
        }
        assert_eq!(nomes(&c, "frota_2").len(), 30);
        assert_eq!(nomes(&c, "frota_legado_").len(), 2);
        let l = listar(&c).unwrap();
        let v = l.iter().find(|v| v.nome == nome).unwrap();
        assert!(v.restauravel);
        assert_eq!((v.carros, v.data.as_str()), (1, "2026-01-01 09:00"));
    }

    #[test]
    fn nomear_muda_o_ficheiro_e_tirar_o_nome_repoe_o_original() {
        let (_t, c, _conn) = cenario();
        copia_diaria(&c, dia(0)).unwrap();
        let novo = nomear(&c, "frota_2026-01-01.db", "  Antes da   limpeza ").unwrap();
        assert_eq!(novo, "frota_guardada_2026-01-01__Antes da limpeza.db");
        let l = listar(&c).unwrap();
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].nome, novo);
        assert_eq!(l[0].rotulo.as_deref(), Some("Antes da limpeza"));
        assert_eq!(l[0].data, "2026-01-01");
        assert!(l[0].restauravel);
        // renomear e tirar o nome
        let outro = nomear(&c, &novo, "Fecho do ano").unwrap();
        assert_eq!(outro, "frota_guardada_2026-01-01__Fecho do ano.db");
        assert_eq!(nomear(&c, &outro, "  ").unwrap(), "frota_2026-01-01.db");
        let l = listar(&c).unwrap();
        assert_eq!(
            (l[0].nome.as_str(), l[0].rotulo.as_deref()),
            ("frota_2026-01-01.db", None)
        );
        // tirar o nome a uma cópia sem nome não faz nada
        assert_eq!(
            nomear(&c, "frota_2026-01-01.db", "").unwrap(),
            "frota_2026-01-01.db"
        );
    }

    #[test]
    fn nomear_recusa_nomes_invalidos_e_versoes_inexistentes() {
        let (_t, c, _conn) = cenario();
        copia_diaria(&c, dia(0)).unwrap();
        let e = nomear(&c, "frota_2026-01-01.db", &"a".repeat(61)).unwrap_err();
        assert_eq!(
            (e.codigo.as_str(), e.campo.as_deref()),
            ("rotulo_longo", Some("rotulo"))
        );
        for mau in [
            "a/b",
            "a\\b",
            "x:y",
            "a..b",
            "fim.",
            "a?",
            "tab\u{7}",
            "a\u{202E}b",
        ] {
            let e = nomear(&c, "frota_2026-01-01.db", mau).unwrap_err();
            assert_eq!(e.codigo, "rotulo_invalido", "{mau}");
        }
        assert_eq!(nomes(&c, "frota_"), vec!["frota_2026-01-01.db"]);
        let novo = nomear(&c, "frota_2026-01-01.db", &"á".repeat(60)).unwrap();
        for nome in ["frota_2026-01-02.db", "..\\frota.db", "frota.db", ""] {
            assert_eq!(
                nomear(&c, nome, "x").unwrap_err().codigo,
                "versao_invalida",
                "{nome}"
            );
        }
        assert!(c.dados.join("versoes").join(novo).exists());
    }

    #[test]
    fn tirar_o_nome_nunca_substitui_outra_copia() {
        let (_t, c, _conn) = cenario();
        copia_diaria(&c, dia(0)).unwrap();
        let novo = nomear(&c, "frota_2026-01-01.db", "Guardar").unwrap();
        // entretanto foi criada outra cópia diária do mesmo dia
        copia_diaria(&c, dia(0)).unwrap();
        let e = nomear(&c, &novo, "").unwrap_err();
        assert_eq!(e.codigo, "nome_ocupado");
        assert_eq!(
            nomes(&c, "frota_"),
            vec![
                "frota_2026-01-01.db",
                "frota_guardada_2026-01-01__Guardar.db"
            ]
        );
    }

    #[test]
    fn copias_com_nome_nunca_sao_podadas_nem_contam() {
        let (_t, c, _conn) = cenario();
        for i in 0..30 {
            copia_diaria(&c, dia(i)).unwrap();
        }
        nomear(&c, "frota_2026-01-01.db", "Guardar").unwrap();
        for i in 30..35 {
            copia_diaria(&c, dia(i)).unwrap();
        }
        assert_eq!(nomes(&c, "frota_2").len(), 30);
        assert_eq!(
            nomes(&c, PREFIXO_GUARDADA),
            vec!["frota_guardada_2026-01-01__Guardar.db"]
        );
        // o mesmo nas "antes da versão"
        for i in 1..=5u64 {
            copia_antes_de(&c, &format!("1.0.{i}")).unwrap();
        }
        nomear(&c, "frota_antes-da-v1.0.1.db", "Primeira").unwrap();
        for i in 6..=9u64 {
            copia_antes_de(&c, &format!("1.0.{i}")).unwrap();
        }
        assert_eq!(nomes(&c, "frota_antes-da-").len(), 5);
        assert!(
            nomes(&c, PREFIXO_GUARDADA)
                .contains(&"frota_guardada_antes-da-v1.0.1__Primeira.db".to_string())
        );
        let l = listar(&c).unwrap();
        let p = l
            .iter()
            .find(|v| v.rotulo.as_deref() == Some("Primeira"))
            .unwrap();
        assert!(p.restauravel);
    }

    #[test]
    fn copia_com_nome_restaura_e_posto_de_lado_com_nome_so_se_verificar() {
        let (_t, c, mut conn) = cenario();
        inserir_carro(&conn, "AA-00-AA");
        let t = dia(0).and_hms_opt(10, 0, 0).unwrap();
        let manual = copia_manual(&conn, &c, t).unwrap();
        let guardada = nomear(&c, &manual, "Um carro").unwrap();
        assert_eq!(listar(&c).unwrap()[0].data, "2026-01-01 10:00");
        inserir_carro(&conn, "BB-11-BB");
        restaurar(
            &mut conn,
            &c,
            &guardada,
            true,
            dia(0).and_hms_opt(11, 0, 0).unwrap(),
        )
        .unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM carros", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
        // um danificado com nome continua a não ser restaurável
        std::fs::write(&c.banco, vec![0xABu8; 4096]).unwrap();
        let dan = guardar_danificado(&c, t, false).unwrap().unwrap();
        let dan = nomear(&c, &dan, "Estragado").unwrap();
        let l = listar(&c).unwrap();
        let v = l.iter().find(|v| v.nome == dan).unwrap();
        assert!(!v.restauravel);
        assert_eq!(v.data, "2026-01-01 10:00");
    }

    #[test]
    fn apagar_remove_so_a_copia_escolhida() {
        let (_t, c, _conn) = cenario();
        copia_diaria(&c, dia(0)).unwrap();
        copia_diaria(&c, dia(1)).unwrap();
        let a = nomear(&c, "frota_2026-01-01.db", "A").unwrap();
        apagar(&c, &a).unwrap();
        apagar(&c, "frota_2026-01-02.db").unwrap();
        assert!(listar(&c).unwrap().is_empty());
        // nomes perigosos, inexistentes ou de outros ficheiros: recusados
        std::fs::write(c.dados.join("versoes").join("frota_guardada_x__y.db"), b"x").unwrap();
        for nome in [
            "frota_2026-01-01.db",
            "..\\frota.db",
            "../dados/frota.db",
            "frota.db",
            "frota_guardada_x__y.db",
            "",
        ] {
            assert_eq!(
                apagar(&c, nome).unwrap_err().codigo,
                "versao_invalida",
                "{nome}"
            );
        }
        assert!(c.banco.exists());
    }

    /// v0.1.1: se o journal não se consegue mover, o banco volta ao lugar.
    #[cfg(windows)]
    #[test]
    fn guardar_danificado_com_journal_preso_deixa_o_banco_no_lugar() {
        use crate::armazenamento::gravacao::com_sufixo;
        use crate::armazenamento::gravacao::testes::trancar;
        let (_t, c, _conn) = cenario();
        std::fs::write(&c.banco, vec![0xABu8; 4096]).unwrap();
        let journal = com_sufixo(&c.banco, "-journal");
        std::fs::write(&journal, b"j").unwrap();
        let tranca = trancar(&journal); // sem partilha de apagamento: o rename falha
        let r = guardar_danificado(&c, dia(0).and_hms_opt(9, 0, 0).unwrap(), false);
        drop(tranca);
        let e = r.unwrap_err();
        assert_eq!(e.codigo, "io");
        assert_eq!(std::fs::read(&c.banco).unwrap(), vec![0xABu8; 4096]);
        assert!(journal.exists());
        assert!(nomes(&c, "frota_danificado_").is_empty());
    }

    /// I5: uma poda que falha não transforma a cópia/restauro num erro.
    #[cfg(windows)]
    #[test]
    fn poda_que_falha_nao_e_erro() {
        use crate::armazenamento::gravacao::testes::trancar;
        let (_t, c, mut conn) = cenario();
        for i in 0..30 {
            copia_diaria(&c, dia(i)).unwrap();
        }
        let velha = c.dados.join("versoes").join("frota_2026-01-01.db");
        let tranca = trancar(&velha); // em uso: não se apaga
        let r1 = copia_diaria(&c, dia(30));
        let r2 = copia_manual(&conn, &c, dia(31).and_hms_opt(10, 0, 0).unwrap());
        let r3 = restaurar(
            &mut conn,
            &c,
            "frota_2026-01-05.db",
            true,
            dia(31).and_hms_opt(11, 0, 0).unwrap(),
        );
        drop(tranca);
        assert!(r1.unwrap());
        assert_eq!(r2.unwrap(), "frota_2026-02-01_1000.db");
        r3.unwrap();
        assert!(velha.exists());
    }
}
