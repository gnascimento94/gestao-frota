//! Estado da aplicação: arranque, gravação automática (com atraso), bloqueio
//! entre PCs, modo só de consulta e fecho.
//!
//! Só a camada de comandos lê o relógio; aqui `agora`/`hoje` chegam por parâmetro.
//!
//! Ordem dos mutexes (para não haver deadlocks): `conn` → `lock` → restantes.

use crate::armazenamento::gravacao::{self, Caminhos};
use crate::armazenamento::lock::{self, Lock};
use crate::armazenamento::versoes::{self, Versao};
use crate::erro::{Erro, Res};
use chrono::{DateTime, Duration, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};

/// Segundos desde a última alteração antes de gravar.
const ATRASO_GRAVAR: i64 = 2;
/// Segundos entre tentativas depois de uma gravação falhada.
const REPETIR_APOS_FALHA: i64 = 60;
/// Segundos entre renovações do lock.
const RENOVAR_LOCK: i64 = 30;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "estado", rename_all = "camelCase")]
pub enum Arranque {
    Ok,
    Bloqueado { pc: String },
    Corrompido,
    VersaoMaisRecente,
    SemPasta { mensagem: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EstadoArmazenamento {
    pub pasta: String,
    pub no_google_drive: bool,
    /// ISO local (`AAAA-MM-DDTHH:MM:SS`).
    pub ultima_gravacao: Option<String>,
    pub erro: Option<String>,
    pub so_consulta: bool,
    pub versao_app: String,
}

pub struct Estado {
    conn: Mutex<Option<Connection>>,
    caminhos: Caminhos,
    lock: Mutex<Option<Lock>>,
    so_consulta: AtomicBool,
    ultima_gravacao: Mutex<Option<NaiveDateTime>>,
    erro_gravacao: Mutex<Option<String>>,
    sujo: AtomicBool,
    raiz: PathBuf,
    pc: String,
    arranque: Mutex<Arranque>,
    ultima_alteracao: Mutex<Option<NaiveDateTime>>,
    /// Hora da última gravação falhada (controla a repetição a cada 60 s).
    ultima_falha: Mutex<Option<NaiveDateTime>>,
    ultima_renovacao: Mutex<Option<NaiveDateTime>>,
    fechado: AtomicBool,
}

fn trava<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn erro(codigo: &str, mensagem: impl Into<String>) -> Erro {
    Erro { codigo: codigo.into(), mensagem: mensagem.into() }
}

fn erro_so_consulta() -> Erro {
    erro("so_consulta", "Aberto só para consulta — não é possível alterar dados.")
}

fn sem_dados() -> Erro {
    erro("sem_dados", "Os dados não estão abertos.")
}

/// Hora local -> UTC (o lock guarda RFC 3339).
pub(crate) fn para_utc(agora: NaiveDateTime) -> DateTime<Utc> {
    Local
        .from_local_datetime(&agora)
        .earliest()
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or_else(|| agora.and_utc())
}

/// A pasta parece estar dentro do Google Drive para computador?
pub fn no_google_drive(pasta: &Path) -> bool {
    let s = format!("{}\\", pasta.to_string_lossy().replace('/', "\\")).to_lowercase();
    [r"\meu disco\", r"\o meu disco\", r"\my drive\", r"\google drive\"]
        .iter()
        .any(|p| s.contains(p))
}

impl Estado {
    fn novo(raiz: &Path, local: &Path, pc: &str) -> Estado {
        Estado {
            conn: Mutex::new(None),
            caminhos: Caminhos::new(raiz, local),
            lock: Mutex::new(None),
            so_consulta: AtomicBool::new(false),
            ultima_gravacao: Mutex::new(None),
            erro_gravacao: Mutex::new(None),
            sujo: AtomicBool::new(false),
            raiz: raiz.to_path_buf(),
            pc: pc.to_string(),
            arranque: Mutex::new(Arranque::Ok),
            ultima_alteracao: Mutex::new(None),
            ultima_falha: Mutex::new(None),
            ultima_renovacao: Mutex::new(None),
            fechado: AtomicBool::new(false),
        }
    }

    /// Cria as pastas, adquire o lock e prepara a cópia de trabalho.
    pub fn arrancar(raiz: &Path, local: &Path, pc: &str, agora: NaiveDateTime) -> (Estado, Arranque) {
        let e = Estado::novo(raiz, local, pc);
        let a = e.iniciar(agora);
        *trava(&e.arranque) = a.clone();
        (e, a)
    }

    fn caminho_lock(&self) -> PathBuf {
        self.caminhos.dados.join("frota.lock")
    }

    fn iniciar(&self, agora: NaiveDateTime) -> Arranque {
        let c = &self.caminhos;
        let pastas = std::fs::create_dir_all(&c.dados).and_then(|_| match c.trabalho.parent() {
            Some(p) => std::fs::create_dir_all(p),
            None => Ok(()),
        });
        if let Err(e) = pastas {
            return Arranque::SemPasta {
                mensagem: format!(
                    "Não foi possível aceder à pasta de dados ({}): {e}",
                    c.dados.display()
                ),
            };
        }
        // O lock vem antes de tocar nos dados.
        let l = match lock::adquirir(&self.caminho_lock(), &self.pc, para_utc(agora)) {
            Ok(l) => l,
            Err(e) if e.codigo == "bloqueado" => return Arranque::Bloqueado { pc: e.mensagem },
            Err(e) => return Arranque::SemPasta { mensagem: e.mensagem },
        };
        match gravacao::preparar_trabalho(c) {
            Ok(conn) => {
                self.instalar(&mut trava(&self.conn), conn, l, agora);
                Arranque::Ok
            }
            Err(e) => {
                let _ = l.libertar();
                match e.codigo.as_str() {
                    "corrompido" => Arranque::Corrompido,
                    "versao_mais_recente" => Arranque::VersaoMaisRecente,
                    _ => Arranque::SemPasta { mensagem: e.mensagem },
                }
            }
        }
    }

    /// Passa a trabalhar sobre `conn` com o lock `l` (modo normal).
    fn instalar(&self, slot: &mut Option<Connection>, conn: Connection, l: Lock, agora: NaiveDateTime) {
        *slot = Some(conn);
        *trava(&self.lock) = Some(l);
        *trava(&self.ultima_renovacao) = Some(agora);
        self.so_consulta.store(false, Ordering::SeqCst);
        // Uma gravação pendente que falhou durante o arranque volta a ser tentada.
        if self.caminhos.pendente.try_exists().unwrap_or(false) {
            *trava(&self.ultima_alteracao) = Some(agora);
            self.sujo.store(true, Ordering::SeqCst);
        }
    }

    pub fn arranque(&self) -> Arranque {
        trava(&self.arranque).clone()
    }

    #[cfg(test)]
    pub fn caminhos(&self) -> &Caminhos {
        &self.caminhos
    }

    /// Abre `banco` só para leitura, sem lock. O conteúdo é copiado para memória
    /// (API de backup) e o ficheiro fechado logo a seguir, para não impedir o
    /// Google Drive de o substituir quando o outro PC grava.
    pub fn abrir_consulta(&self) -> Res<()> {
        let mut g = trava(&self.conn);
        if g.is_some() && !self.so_consulta.load(Ordering::SeqCst) {
            return Err(erro("ja_aberto", "Os dados já estão abertos."));
        }
        if !self.caminhos.banco.try_exists()? {
            return Err(erro("sem_dados", "Ainda não há dados guardados na pasta."));
        }
        let origem =
            Connection::open_with_flags(&self.caminhos.banco, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let mut mem = Connection::open_in_memory()?;
        {
            let b = rusqlite::backup::Backup::new(&origem, &mut mem)?;
            b.run_to_completion(64, std::time::Duration::ZERO, None)?;
        }
        drop(origem);
        mem.execute_batch("PRAGMA query_only=ON;")?;
        self.so_consulta.store(true, Ordering::SeqCst);
        *g = Some(mem);
        Ok(())
    }

    fn marcar(&self) {
        if let Err(e) = gravacao::marcar_pendente(&self.caminhos) {
            *trava(&self.erro_gravacao) = Some(e.mensagem);
        }
    }

    fn registar(&self, agora: NaiveDateTime) {
        *trava(&self.ultima_alteracao) = Some(agora);
        self.sujo.store(true, Ordering::SeqCst);
    }

    /// Houve uma alteração: cria já o marcador `pendente` (um crash antes da
    /// gravação não a perde) e marca `sujo`.
    pub fn alterado(&self, agora: NaiveDateTime) {
        self.marcar();
        self.registar(agora);
    }

    /// Leitura: segura a ligação só durante `f`.
    pub fn ler<T>(&self, f: impl FnOnce(&Connection) -> Res<T>) -> Res<T> {
        let g = trava(&self.conn);
        f(g.as_ref().ok_or_else(sem_dados)?)
    }

    /// Escrita: recusada em modo consulta; o marcador `pendente` é criado antes
    /// de `f` e, se `f` correr bem, a alteração fica registada para gravar.
    pub fn mutar<T>(&self, agora: NaiveDateTime, f: impl FnOnce(&mut Connection) -> Res<T>) -> Res<T> {
        if self.so_consulta.load(Ordering::SeqCst) {
            return Err(erro_so_consulta());
        }
        let mut g = trava(&self.conn);
        let conn = g.as_mut().ok_or_else(sem_dados)?;
        if self.so_consulta.load(Ordering::SeqCst) {
            return Err(erro_so_consulta());
        }
        self.marcar();
        let r = f(conn)?;
        self.alterado(agora);
        Ok(r)
    }

    /// Chamado a cada segundo.
    pub fn tick(&self, agora: NaiveDateTime) {
        if self.fechado.load(Ordering::SeqCst) {
            return;
        }
        self.renovar_lock(agora);
        if self.so_consulta.load(Ordering::SeqCst) || !self.sujo.load(Ordering::SeqCst) {
            return;
        }
        let falha = *trava(&self.ultima_falha);
        let pronto = match falha {
            Some(f) => agora - f >= Duration::seconds(REPETIR_APOS_FALHA),
            None => trava(&self.ultima_alteracao)
                .is_none_or(|a| agora - a >= Duration::seconds(ATRASO_GRAVAR)),
        };
        if pronto {
            let _ = self.gravar_agora(agora);
        }
    }

    fn renovar_lock(&self, agora: NaiveDateTime) {
        let devido = trava(&self.ultima_renovacao)
            .is_none_or(|r| agora - r >= Duration::seconds(RENOVAR_LOCK));
        if !devido {
            return;
        }
        let mut g = trava(&self.lock);
        let Some(l) = g.as_ref() else { return };
        *trava(&self.ultima_renovacao) = Some(agora);
        match l.renovar(para_utc(agora)) {
            // "desconhecido" = não se conseguiu ler o lock (p.ex. falha momentânea
            // do Drive): tenta-se outra vez na próxima renovação.
            Err(e) if e.codigo == "bloqueado" && e.mensagem != "desconhecido" => {
                self.so_consulta.store(true, Ordering::SeqCst);
                *g = None; // já não é nosso: não o libertar
                *trava(&self.erro_gravacao) = Some(format!(
                    "O sistema foi aberto noutro PC ({}). Os dados deste PC continuam guardados localmente.",
                    e.mensagem
                ));
            }
            _ => {}
        }
    }

    fn gravar_agora(&self, agora: NaiveDateTime) -> Res<()> {
        let g = trava(&self.conn);
        let Some(conn) = g.as_ref() else { return Ok(()) };
        if self.so_consulta.load(Ordering::SeqCst) {
            return Ok(());
        }
        match gravacao::gravar(conn, &self.caminhos) {
            Ok(()) => {
                self.sujo.store(false, Ordering::SeqCst);
                *trava(&self.ultima_gravacao) = Some(agora);
                *trava(&self.erro_gravacao) = None;
                *trava(&self.ultima_falha) = None;
                Ok(())
            }
            Err(e) => {
                *trava(&self.erro_gravacao) = Some(e.mensagem.clone());
                *trava(&self.ultima_falha) = Some(agora);
                Err(e)
            }
        }
    }

    /// Grava (se houver alterações), faz a cópia diária e liberta o lock.
    /// Só atua na primeira chamada.
    pub fn fechar(&self, hoje: NaiveDate, agora: NaiveDateTime) -> Res<()> {
        if self.fechado.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        let mut primeiro: Option<Erro> = None;
        let consulta = self.so_consulta.load(Ordering::SeqCst);
        if !consulta
            && self.sujo.load(Ordering::SeqCst)
            && let Err(e) = self.gravar_agora(agora)
        {
            primeiro.get_or_insert(e);
        }
        let mut g = trava(&self.lock);
        if !consulta
            && g.is_some()
            && let Err(e) = versoes::copia_diaria(&self.caminhos, hoje)
        {
            primeiro.get_or_insert(e);
        }
        if let Some(l) = g.take()
            && let Err(e) = l.libertar()
        {
            primeiro.get_or_insert(e);
        }
        drop(g);
        *trava(&self.conn) = None;
        primeiro.map_or(Ok(()), Err)
    }

    pub fn armazenamento(&self) -> EstadoArmazenamento {
        EstadoArmazenamento {
            pasta: self.raiz.display().to_string(),
            no_google_drive: no_google_drive(&self.raiz),
            ultima_gravacao: trava(&self.ultima_gravacao)
                .map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string()),
            erro: trava(&self.erro_gravacao).clone(),
            so_consulta: self.so_consulta.load(Ordering::SeqCst),
            versao_app: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    /// Cópia manual do estado atual.
    pub fn fazer_copia(&self, agora: NaiveDateTime) -> Res<()> {
        if self.so_consulta.load(Ordering::SeqCst) {
            return Err(erro_so_consulta());
        }
        self.ler(|c| versoes::copia_manual(c, &self.caminhos, agora))
    }

    pub fn listar_versoes(&self) -> Res<Vec<Versao>> {
        versoes::listar(&self.caminhos)
    }

    /// Repõe uma versão. Em modo normal carrega-a na ligação aberta. Depois de um
    /// arranque `Corrompido` (sem ligação): adquire o lock, restaura para uma
    /// ligação vazia em memória (que grava `banco`), e continua o arranque normal
    /// com `preparar_trabalho`.
    pub fn restaurar_versao(&self, nome: &str, agora: NaiveDateTime) -> Res<()> {
        if self.so_consulta.load(Ordering::SeqCst) {
            return Err(erro_so_consulta());
        }
        let mut g = trava(&self.conn);
        if let Some(conn) = g.as_mut() {
            self.marcar();
            let r = versoes::restaurar(conn, &self.caminhos, nome, agora);
            match &r {
                Ok(()) => {
                    self.sujo.store(false, Ordering::SeqCst);
                    *trava(&self.ultima_gravacao) = Some(agora);
                    *trava(&self.erro_gravacao) = None;
                    *trava(&self.ultima_falha) = None;
                }
                // A ligação pode ter mudado: gravar o que lá estiver.
                Err(_) => self.registar(agora),
            }
            return r;
        }
        if self.arranque() != Arranque::Corrompido {
            return Err(sem_dados());
        }
        let l = match lock::adquirir(&self.caminho_lock(), &self.pc, para_utc(agora)) {
            Ok(l) => l,
            Err(e) if e.codigo == "bloqueado" => {
                return Err(erro(
                    "bloqueado",
                    format!("O sistema está aberto no PC «{}». Feche-o lá primeiro.", e.mensagem),
                ));
            }
            Err(e) => return Err(e),
        };
        let r = (|| {
            let mut mem = Connection::open_in_memory()?;
            versoes::restaurar(&mut mem, &self.caminhos, nome, agora)?;
            drop(mem);
            gravacao::preparar_trabalho(&self.caminhos)
        })();
        match r {
            Ok(conn) => {
                self.instalar(&mut g, conn, l, agora);
                *trava(&self.ultima_gravacao) = Some(agora);
                *trava(&self.arranque) = Arranque::Ok;
                Ok(())
            }
            Err(e) => {
                let _ = l.libertar();
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::armazenamento::lock;
    use chrono::{Duration, NaiveDate};
    use rusqlite::{Connection, OpenFlags};
    use std::path::Path;

    fn t0() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 10, 20)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap()
    }

    fn s(n: i64) -> NaiveDateTime {
        t0() + Duration::seconds(n)
    }

    fn cenario() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
        let t = tempfile::tempdir().unwrap();
        let raiz = t.path().join("raiz");
        let local = t.path().join("local");
        (t, raiz, local)
    }

    fn inserir(e: &Estado, agora: NaiveDateTime, matricula: &str) -> Res<i64> {
        e.mutar(agora, |c| {
            c.execute(
                "INSERT INTO carros (matricula, estado) VALUES (?1, 'ativo')",
                [matricula],
            )?;
            Ok(c.last_insert_rowid())
        })
    }

    fn carros_no_banco(p: &Path) -> i64 {
        let c = Connection::open_with_flags(p, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        c.query_row("SELECT COUNT(*) FROM carros", [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn arranque_normal_e_gravacao_apos_alteracao() {
        let (_t, raiz, local) = cenario();
        let (e, a) = Estado::arrancar(&raiz, &local, "PC-A", t0());
        assert_eq!(a, Arranque::Ok);
        inserir(&e, t0(), "AA-00-AA").unwrap();
        e.tick(s(1));
        let banco = e.caminhos().banco.clone();
        assert!(!banco.exists() || carros_no_banco(&banco) == 0);
        e.tick(s(3));
        assert_eq!(carros_no_banco(&banco), 1);
        assert!(e.armazenamento().ultima_gravacao.is_some());
        assert!(!e.caminhos().pendente.exists());
    }

    #[test]
    fn arranque_bloqueado_por_outro_pc() {
        let (_t, raiz, local) = cenario();
        let dados = raiz.join("dados");
        std::fs::create_dir_all(&dados).unwrap();
        lock::adquirir(&dados.join("frota.lock"), "PC-X", para_utc(s(-10))).unwrap();
        let (e, a) = Estado::arrancar(&raiz, &local, "PC-Y", t0());
        assert_eq!(a, Arranque::Bloqueado { pc: "PC-X".into() });
        // não ficou com dados abertos nem tocou no lock do outro PC
        assert_eq!(inserir(&e, t0(), "AA-00-AA").unwrap_err().codigo, "sem_dados");
        assert!(std::fs::read_to_string(dados.join("frota.lock"))
            .unwrap()
            .starts_with("PC-X\n"));
    }

    #[test]
    fn modo_consulta_recusa_mutacoes() {
        let (_t, raiz, local) = cenario();
        // outro PC criou dados e está aberto
        {
            let (e, _) = Estado::arrancar(&raiz, &local.join("outro"), "PC-X", t0());
            inserir(&e, t0(), "AA-00-AA").unwrap();
            e.tick(s(3));
        }
        let (e, a) = Estado::arrancar(&raiz, &local, "PC-Y", s(5));
        assert_eq!(a, Arranque::Bloqueado { pc: "PC-X".into() });
        e.abrir_consulta().unwrap();
        assert!(e.armazenamento().so_consulta);
        let err = inserir(&e, s(6), "BB-11-BB").unwrap_err();
        assert_eq!(err.codigo, "so_consulta");
        assert_eq!(
            err.mensagem,
            "Aberto só para consulta — não é possível alterar dados."
        );
        // leituras funcionam
        let n: i64 = e
            .ler(|c| Ok(c.query_row("SELECT COUNT(*) FROM carros", [], |r| r.get(0))?))
            .unwrap();
        assert_eq!(n, 1);
        assert!(!e.caminhos().pendente.exists());
    }

    #[test]
    fn gravacao_falhada_fica_registada_e_recupera() {
        let (_t, raiz, local) = cenario();
        let (e, _) = Estado::arrancar(&raiz, &local, "PC-A", t0());
        inserir(&e, t0(), "AA-00-AA").unwrap();
        let dados = raiz.join("dados");
        let escondida = raiz.join("dados_fora");
        std::fs::rename(&dados, &escondida).unwrap();
        e.tick(s(3));
        let erro = e.armazenamento().erro;
        assert!(erro.is_some(), "esperava erro de gravação");
        assert!(e.caminhos().pendente.exists());
        std::fs::rename(&escondida, &dados).unwrap();
        // antes de 60 s não tenta outra vez
        e.tick(s(30));
        assert!(e.armazenamento().erro.is_some());
        e.tick(s(3 + 61));
        assert_eq!(e.armazenamento().erro, None);
        assert_eq!(carros_no_banco(&e.caminhos().banco), 1);
        assert!(!e.caminhos().pendente.exists());
    }

    #[test]
    fn fechar_grava_copia_diaria_e_liberta_lock() {
        let (_t, raiz, local) = cenario();
        let (e, _) = Estado::arrancar(&raiz, &local, "PC-A", t0());
        inserir(&e, t0(), "AA-00-AA").unwrap();
        let hoje = t0().date();
        e.fechar(hoje, s(1)).unwrap();
        let dados = raiz.join("dados");
        assert_eq!(carros_no_banco(&dados.join("frota.db")), 1);
        let diaria = dados.join("versoes").join("frota_2026-10-20.db");
        assert_eq!(carros_no_banco(&diaria), 1);
        assert!(!dados.join("frota.lock").exists());
        // segunda chamada não faz nada (e não falha)
        e.fechar(hoje, s(2)).unwrap();
        // outro PC pode abrir logo a seguir
        let (_e2, a) = Estado::arrancar(&raiz, &local.join("b"), "PC-B", s(3));
        assert_eq!(a, Arranque::Ok);
    }

    #[test]
    fn alteracao_marca_pendente_imediatamente() {
        let (_t, raiz, local) = cenario();
        let (e, _) = Estado::arrancar(&raiz, &local, "PC-A", t0());
        assert!(!e.caminhos().pendente.exists());
        inserir(&e, t0(), "AA-00-AA").unwrap();
        // sem tick: o marcador já existe, logo um crash agora não perde a alteração
        assert!(e.caminhos().pendente.exists());
        drop(e);
        let (e2, a) = Estado::arrancar(&raiz, &local, "PC-A", s(1));
        assert_eq!(a, Arranque::Ok);
        let n: i64 = e2
            .ler(|c| Ok(c.query_row("SELECT COUNT(*) FROM carros", [], |r| r.get(0))?))
            .unwrap();
        assert_eq!(n, 1);
        assert_eq!(carros_no_banco(&e2.caminhos().banco), 1);
    }

    #[test]
    fn renovacao_perdida_passa_a_consulta() {
        let (_t, raiz, local) = cenario();
        let (e, _) = Estado::arrancar(&raiz, &local, "PC-A", t0());
        e.tick(s(1));
        inserir(&e, s(10), "AA-00-AA").unwrap();
        // outro PC assumiu o lock (p.ex. este PC esteve suspenso)
        let lock_p = raiz.join("dados").join("frota.lock");
        std::fs::write(&lock_p, format!("PC-X\n{}", para_utc(s(25)).to_rfc3339())).unwrap();
        e.tick(s(31));
        let est = e.armazenamento();
        assert!(est.so_consulta);
        assert_eq!(
            est.erro.as_deref(),
            Some("O sistema foi aberto noutro PC (PC-X). Os dados deste PC continuam guardados localmente.")
        );
        // não grava por cima dos dados do outro PC
        e.tick(s(40));
        let banco = e.caminhos().banco.clone();
        assert!(!banco.exists() || carros_no_banco(&banco) == 0);
        assert_eq!(inserir(&e, s(41), "BB-11-BB").unwrap_err().codigo, "so_consulta");
        e.fechar(s(50).date(), s(50)).unwrap();
        assert!(std::fs::read_to_string(&lock_p).unwrap().starts_with("PC-X\n"));
        assert!(!banco.exists() || carros_no_banco(&banco) == 0);
    }

    #[test]
    fn corrompido_permite_restaurar_e_continua() {
        let (_t, raiz, local) = cenario();
        {
            let (e, _) = Estado::arrancar(&raiz, &local, "PC-A", t0());
            inserir(&e, t0(), "AA-00-AA").unwrap();
            e.fechar(t0().date(), s(1)).unwrap();
        }
        let banco = raiz.join("dados").join("frota.db");
        std::fs::write(&banco, vec![0xABu8; 4096]).unwrap();
        let (e, a) = Estado::arrancar(&raiz, &local, "PC-A", s(10));
        assert_eq!(a, Arranque::Corrompido);
        assert!(!raiz.join("dados").join("frota.lock").exists());
        let v = e.listar_versoes().unwrap();
        assert!(v.iter().any(|x| x.nome == "frota_2026-10-20.db"));
        e.restaurar_versao("frota_2026-10-20.db", s(20)).unwrap();
        assert_eq!(e.arranque(), Arranque::Ok);
        assert!(raiz.join("dados").join("frota.lock").exists());
        assert_eq!(carros_no_banco(&banco), 1);
        inserir(&e, s(21), "BB-11-BB").unwrap();
        e.tick(s(24));
        assert_eq!(carros_no_banco(&banco), 2);
    }

    #[test]
    fn deteta_google_drive() {
        assert!(no_google_drive(Path::new(r"G:\O meu disco\Frota")));
        assert!(no_google_drive(Path::new(r"G:\My Drive\Frota")));
        assert!(no_google_drive(Path::new(r"C:\Users\x\Google Drive\Frota")));
        assert!(no_google_drive(Path::new(r"g:\meu disco\frota")));
        assert!(!no_google_drive(Path::new(r"C:\Frota")));
    }
}
