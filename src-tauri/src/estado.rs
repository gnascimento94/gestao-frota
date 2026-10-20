//! Estado da aplicação: arranque, gravação automática (com atraso), bloqueio
//! entre PCs, modo só de consulta e fecho.
//!
//! Só a camada de comandos lê o relógio; aqui `agora`/`hoje` chegam por parâmetro.
//!
//! Ordem dos mutexes (para não haver deadlocks): `conn` → `lock` → restantes.
//!
//! **Nunca gravar por cima de dados gravados noutro PC.** Depois de cada
//! gravação própria guarda-se a assinatura do `banco` (tamanho, data de
//! modificação e os primeiros 100 bytes) em `<local>\assinatura`. Antes de cada
//! gravação (e no arranque, quando há `pendente`), se o `banco` existir e a
//! assinatura for diferente, houve uma gravação noutro PC: em vez de gravar, a
//! cópia de trabalho é guardada como versão (`copia_manual`) e o `pendente`
//! removido. Sem ficheiro de assinatura (primeira vez) não há conflito.

use crate::armazenamento::gravacao::{self, Caminhos};
use crate::armazenamento::lock::{self, Lock};
use crate::armazenamento::versoes::{self, Versao};
use crate::erro::{Erro, Res};
use chrono::{DateTime, Duration, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::UNIX_EPOCH;

/// Segundos desde a última alteração antes de gravar.
const ATRASO_GRAVAR: i64 = 2;
/// Segundos entre tentativas depois de uma gravação falhada.
const REPETIR_APOS_FALHA: i64 = 60;
/// Segundos entre renovações do lock.
const RENOVAR_LOCK: i64 = 30;
/// Intervalo sem renovar a partir do qual o lock pode ter sido assumido por
/// outro PC (p.ex. depois de o PC estar suspenso).
const LOCK_ABANDONADO: i64 = 120;

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
    /// Aviso do arranque (alterações deste PC guardadas em Backup por conflito).
    /// Fica até ser dispensado (`dispensar_aviso`); as gravações não o apagam.
    pub aviso_arranque: Option<String>,
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
    /// Ver `EstadoArmazenamento::aviso_arranque`.
    aviso_arranque: Mutex<Option<String>>,
    sujo: AtomicBool,
    raiz: PathBuf,
    pc: String,
    /// `<local>\assinatura`: assinatura do `banco` na última gravação própria.
    assinatura: PathBuf,
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
    Erro { campo: None, codigo: codigo.into(), mensagem: mensagem.into() }
}

fn erro_so_consulta() -> Erro {
    erro("so_consulta", "Aberto só para consulta — não é possível alterar dados.")
}

fn sem_dados() -> Erro {
    erro("sem_dados", "Os dados não estão abertos.")
}

fn msg_conflito(agora: NaiveDateTime) -> String {
    format!(
        "Os dados foram alterados noutro PC. As alterações feitas neste PC foram guardadas em Backup (cópia de {}). Feche e volte a abrir o sistema.",
        agora.format("%d/%m %H:%M")
    )
}

fn msg_conflito_sem_copia(e: &Erro) -> String {
    format!(
        "Os dados foram alterados noutro PC. Não foi possível guardar uma cópia das alterações deste PC ({}); continuam neste computador. Feche e volte a abrir o sistema.",
        e.mensagem
    )
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

/// Tamanho, data de modificação (ns) e primeiros 100 bytes (hex) do ficheiro;
/// `None` se não existir ou não se conseguir ler.
fn assinatura_de(p: &Path) -> Option<String> {
    let m = std::fs::metadata(p).ok()?;
    let mtime = m.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_nanos();
    let mut cab = Vec::with_capacity(100);
    std::fs::File::open(p)
        .ok()?
        .take(100)
        .read_to_end(&mut cab)
        .ok()?;
    let hex: String = cab.iter().map(|b| format!("{b:02x}")).collect();
    Some(format!("{}\n{mtime}\n{hex}", m.len()))
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
            aviso_arranque: Mutex::new(None),
            sujo: AtomicBool::new(false),
            raiz: raiz.to_path_buf(),
            pc: pc.to_string(),
            assinatura: local.join("assinatura"),
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
        let a = e.iniciar(&mut trava(&e.conn), agora);
        *trava(&e.arranque) = a.clone();
        (e, a)
    }

    fn caminho_lock(&self) -> PathBuf {
        self.caminhos.dados.join("frota.lock")
    }

    fn pendente(&self) -> bool {
        self.caminhos.pendente.try_exists().unwrap_or(false)
    }

    // ---------- assinatura do banco ----------

    /// Guarda a assinatura atual do `banco` (escrita atómica). Se falhar, apaga a
    /// antiga: sem assinatura não há conflito, com uma desatualizada haveria um
    /// conflito falso.
    fn registar_assinatura(&self) {
        let ok = assinatura_de(&self.caminhos.banco).is_some_and(|s| {
            let mut tmp = self.assinatura.as_os_str().to_owned();
            tmp.push(".tmp");
            let tmp = PathBuf::from(tmp);
            let r = std::fs::write(&tmp, s).and_then(|_| std::fs::rename(&tmp, &self.assinatura));
            if r.is_err() {
                let _ = std::fs::remove_file(&tmp);
            }
            r.is_ok()
        });
        if !ok {
            let _ = std::fs::remove_file(&self.assinatura);
        }
    }

    /// O `banco` foi gravado por outro PC desde a nossa última gravação?
    fn alterado_fora(&self) -> bool {
        let Ok(guardada) = std::fs::read_to_string(&self.assinatura) else {
            return false;
        };
        assinatura_de(&self.caminhos.banco).is_some_and(|atual| atual != guardada)
    }

    // ---------- arranque ----------

    /// Requer: quem chama segura `conn` (passada em `slot`, vazia).
    fn iniciar(&self, slot: &mut Option<Connection>, agora: NaiveDateTime) -> Arranque {
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
        // Alterações pendentes feitas sobre um banco que entretanto outro PC
        // gravou: guardam-se como versão e segue-se a partir do banco.
        let mut aviso = None;
        if self.pendente() && c.trabalho.try_exists().unwrap_or(false) && self.alterado_fora() {
            match self.preservar_trabalho(agora) {
                Ok(()) => aviso = Some(msg_conflito(agora)),
                Err(e) => {
                    let _ = l.libertar();
                    return Arranque::SemPasta { mensagem: msg_conflito_sem_copia(&e) };
                }
            }
        }
        match gravacao::preparar_trabalho(c) {
            Ok(conn) => {
                self.instalar(slot, conn, l, agora);
                if aviso.is_some() {
                    *trava(&self.aviso_arranque) = aviso;
                }
                Arranque::Ok
            }
            Err(e) => {
                if aviso.is_some() {
                    *trava(&self.aviso_arranque) = aviso;
                }
                let _ = l.libertar();
                match e.codigo.as_str() {
                    "corrompido" => Arranque::Corrompido,
                    "versao_mais_recente" => Arranque::VersaoMaisRecente,
                    _ => Arranque::SemPasta { mensagem: e.mensagem },
                }
            }
        }
    }

    /// Guarda `trabalho.db` como versão e remove o `pendente` (arranque).
    fn preservar_trabalho(&self, agora: NaiveDateTime) -> Res<()> {
        let conn = Connection::open(&self.caminhos.trabalho)?;
        versoes::copia_manual(&conn, &self.caminhos, agora)?;
        drop(conn);
        std::fs::remove_file(&self.caminhos.pendente)?;
        Ok(())
    }

    /// Passa a trabalhar sobre `conn` com o lock `l` (modo normal).
    fn instalar(&self, slot: &mut Option<Connection>, conn: Connection, l: Lock, agora: NaiveDateTime) {
        *slot = Some(conn);
        *trava(&self.lock) = Some(l);
        *trava(&self.ultima_renovacao) = Some(agora);
        self.so_consulta.store(false, Ordering::SeqCst);
        if self.pendente() {
            // A gravação pendente falhou durante o arranque: volta a ser tentada.
            // A assinatura guardada continua a ser a do banco em que o trabalho se baseia.
            *trava(&self.ultima_alteracao) = Some(agora);
            self.sujo.store(true, Ordering::SeqCst);
        } else {
            // trabalho == banco
            self.registar_assinatura();
        }
    }

    pub fn arranque(&self) -> Arranque {
        trava(&self.arranque).clone()
    }

    /// "Tentar de novo": depois de um arranque `Bloqueado` ou `SemPasta` (sem
    /// dados abertos, nem em consulta) volta a tentar arrancar; nos outros casos
    /// devolve o estado atual.
    pub fn tentar_arranque(&self, agora: NaiveDateTime) -> Arranque {
        let mut g = trava(&self.conn);
        let atual = self.arranque();
        let repetir = g.is_none()
            && !self.fechado.load(Ordering::SeqCst)
            && matches!(atual, Arranque::Bloqueado { .. } | Arranque::SemPasta { .. });
        if !repetir {
            return atual;
        }
        let a = self.iniciar(&mut g, agora);
        *trava(&self.arranque) = a.clone();
        a
    }

    /// O utilizador fechou o aviso do arranque.
    pub fn dispensar_aviso(&self) {
        *trava(&self.aviso_arranque) = None;
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

    // ---------- alterações ----------

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
    /// de `f` e, se `f` correr bem, `alterado` regista a alteração para gravar.
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

    // ---------- conflitos ----------

    /// Guarda o estado atual de `conn` como versão e remove o `pendente`.
    /// Devolve o nome da versão.
    fn salvaguardar(&self, conn: &Connection, agora: NaiveDateTime) -> Res<String> {
        let nome = versoes::copia_manual(conn, &self.caminhos, agora)?;
        match std::fs::remove_file(&self.caminhos.pendente) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        self.sujo.store(false, Ordering::SeqCst);
        Ok(nome)
    }

    /// Outro PC gravou o `banco`: não gravar; guardar o trabalho como versão,
    /// passar a consulta e largar o lock. Devolve o erro a mostrar.
    /// Requer: quem chama segura `conn` e NÃO segura `lock`.
    fn entrar_em_conflito(&self, conn: &Connection, agora: NaiveDateTime) -> Erro {
        self.so_consulta.store(true, Ordering::SeqCst);
        let msg = match self.salvaguardar(conn, agora) {
            Ok(_) => msg_conflito(agora),
            Err(e) => msg_conflito_sem_copia(&e),
        };
        *trava(&self.erro_gravacao) = Some(msg.clone());
        if let Some(l) = trava(&self.lock).take() {
            let _ = l.libertar(); // só apaga se ainda for nosso
        }
        erro("conflito", msg)
    }

    // ---------- gravação automática ----------

    /// Chamado a cada segundo. Segura `conn` durante a renovação e a gravação.
    pub fn tick(&self, agora: NaiveDateTime) {
        if self.fechado.load(Ordering::SeqCst) {
            return;
        }
        let g = trava(&self.conn);
        let Some(conn) = g.as_ref() else { return };
        self.renovar_lock(conn, agora);
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
            let _ = self.gravar_com(conn, agora);
        }
    }

    fn renovar_lock(&self, conn: &Connection, agora: NaiveDateTime) {
        let ultima = *trava(&self.ultima_renovacao);
        if ultima.is_some_and(|r| agora - r < Duration::seconds(RENOVAR_LOCK)) {
            return;
        }
        let suspenso = ultima.is_some_and(|r| agora - r >= Duration::seconds(LOCK_ABANDONADO));
        let mut g = trava(&self.lock);
        let Some(l) = g.as_ref() else { return };
        *trava(&self.ultima_renovacao) = Some(agora);
        if suspenso && self.alterado_fora() {
            drop(g);
            self.entrar_em_conflito(conn, agora);
            return;
        }
        match l.renovar(para_utc(agora)) {
            // "desconhecido" = lock ilegível e recente (p.ex. falha momentânea
            // do Drive): tenta-se outra vez na próxima renovação.
            Err(e) if e.codigo == "bloqueado" && e.mensagem != "desconhecido" => {
                self.so_consulta.store(true, Ordering::SeqCst);
                *g = None; // já não é nosso: não o libertar
                drop(g);
                let por_gravar = self.sujo.load(Ordering::SeqCst) || self.pendente();
                let msg = match por_gravar.then(|| self.salvaguardar(conn, agora)) {
                    Some(Ok(_)) => format!(
                        "O sistema foi aberto noutro PC ({}). As alterações feitas neste PC foram guardadas em Backup (cópia de {}). Feche e volte a abrir o sistema.",
                        e.mensagem,
                        agora.format("%d/%m %H:%M")
                    ),
                    _ => format!(
                        "O sistema foi aberto noutro PC ({}). Os dados deste PC continuam guardados localmente.",
                        e.mensagem
                    ),
                };
                *trava(&self.erro_gravacao) = Some(msg);
            }
            _ => {}
        }
    }

    /// Grava `conn` no `banco`, salvo em consulta ou se outro PC o alterou.
    /// Requer: quem chama segura `conn` e NÃO segura `lock`.
    fn gravar_com(&self, conn: &Connection, agora: NaiveDateTime) -> Res<()> {
        if self.so_consulta.load(Ordering::SeqCst) {
            return Ok(());
        }
        if self.alterado_fora() {
            return Err(self.entrar_em_conflito(conn, agora));
        }
        match gravacao::gravar(conn, &self.caminhos) {
            Ok(()) => {
                self.registar_assinatura();
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

    /// Grava (se houver alterações e o banco não tiver sido alterado noutro PC),
    /// faz a cópia diária e liberta o lock. Só atua na primeira chamada.
    pub fn fechar(&self, hoje: NaiveDate, agora: NaiveDateTime) -> Res<()> {
        if self.fechado.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        let mut gc = trava(&self.conn);
        let mut primeiro: Option<Erro> = None;
        if let Some(conn) = gc.as_ref()
            && !self.so_consulta.load(Ordering::SeqCst)
            && self.sujo.load(Ordering::SeqCst)
            && let Err(e) = self.gravar_com(conn, agora)
        {
            primeiro.get_or_insert(e);
        }
        let consulta = self.so_consulta.load(Ordering::SeqCst);
        let mut gl = trava(&self.lock);
        if !consulta
            && gl.is_some()
            && let Err(e) = versoes::copia_diaria(&self.caminhos, hoje)
        {
            primeiro.get_or_insert(e);
        }
        if let Some(l) = gl.take()
            && let Err(e) = l.libertar()
        {
            primeiro.get_or_insert(e);
        }
        drop(gl);
        *gc = None;
        match &primeiro {
            None => log::info!("fechar: gravado, cópia diária, lock libertado"),
            Some(e) => log::warn!("fechar: concluído com erro: {}", e.mensagem),
        }
        primeiro.map_or(Ok(()), Err)
    }

    pub fn armazenamento(&self) -> EstadoArmazenamento {
        EstadoArmazenamento {
            pasta: self.raiz.display().to_string(),
            no_google_drive: no_google_drive(&self.raiz),
            ultima_gravacao: trava(&self.ultima_gravacao)
                .map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string()),
            erro: trava(&self.erro_gravacao).clone(),
            aviso_arranque: trava(&self.aviso_arranque).clone(),
            so_consulta: self.so_consulta.load(Ordering::SeqCst),
            versao_app: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    // ---------- versões ----------

    /// Cópia manual do estado atual (com a mesma verificação de conflito).
    pub fn fazer_copia(&self, agora: NaiveDateTime) -> Res<()> {
        if self.so_consulta.load(Ordering::SeqCst) {
            return Err(erro_so_consulta());
        }
        let g = trava(&self.conn);
        let conn = g.as_ref().ok_or_else(sem_dados)?;
        if self.alterado_fora() {
            return Err(self.entrar_em_conflito(conn, agora));
        }
        versoes::copia_manual(conn, &self.caminhos, agora).map(|_| ())
    }

    pub fn listar_versoes(&self) -> Res<Vec<Versao>> {
        versoes::listar(&self.caminhos)
    }

    /// Repõe uma versão. Em modo normal carrega-a na ligação aberta (com cópia de
    /// segurança do estado atual). Depois de um arranque `Corrompido` (sem
    /// ligação): adquire o lock, restaura para uma ligação vazia em memória (sem
    /// cópia de segurança, que seria de uma base vazia), o que grava `banco`, e
    /// continua o arranque normal com `preparar_trabalho`.
    pub fn restaurar_versao(&self, nome: &str, agora: NaiveDateTime) -> Res<()> {
        if self.so_consulta.load(Ordering::SeqCst) {
            return Err(erro_so_consulta());
        }
        let mut g = trava(&self.conn);
        if let Some(conn) = g.as_mut() {
            if self.alterado_fora() {
                return Err(self.entrar_em_conflito(conn, agora));
            }
            self.marcar();
            let r = versoes::restaurar(conn, &self.caminhos, nome, true, agora);
            if !self.pendente() {
                // `restaurar` gravou o banco (o pendente só sai depois de gravar).
                self.registar_assinatura();
            }
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
            versoes::restaurar(&mut mem, &self.caminhos, nome, false, agora)?;
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
            Some("O sistema foi aberto noutro PC (PC-X). As alterações feitas neste PC foram guardadas em Backup (cópia de 20/10 12:00). Feche e volte a abrir o sistema.")
        );
        // a alteração por gravar ficou numa versão e o pendente foi removido
        assert_eq!(versao_com(&raiz, "AA-00-AA").as_deref(), Some("frota_2026-10-20_1200.db"));
        assert!(!e.caminhos().pendente.exists());
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
        // sem "cópia de segurança" de uma base vazia
        let nomes: Vec<_> = e.listar_versoes().unwrap().into_iter().map(|v| v.nome).collect();
        assert_eq!(nomes, vec!["frota_2026-10-20.db"]);
        assert!(raiz.join("dados").join("frota.lock").exists());
        assert_eq!(carros_no_banco(&banco), 1);
        inserir(&e, s(21), "BB-11-BB").unwrap();
        e.tick(s(24));
        assert_eq!(carros_no_banco(&banco), 2);
    }

    /// Versão (em `dados\versoes`) que contém a matrícula.
    fn versao_com(raiz: &Path, matricula: &str) -> Option<String> {
        std::fs::read_dir(raiz.join("dados").join("versoes"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| {
                let c = Connection::open_with_flags(p, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
                c.query_row(
                    "SELECT COUNT(*) FROM carros WHERE matricula = ?1",
                    [matricula],
                    |r| r.get::<_, i64>(0),
                )
                .unwrap()
                    > 0
            })
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
    }

    fn tem(p: &Path, matricula: &str) -> bool {
        let c = Connection::open_with_flags(p, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        c.query_row(
            "SELECT COUNT(*) FROM carros WHERE matricula = ?1",
            [matricula],
            |r| r.get::<_, i64>(0),
        )
        .unwrap()
            > 0
    }

    const MSG_CONFLITO: &str = "Os dados foram alterados noutro PC. As alterações feitas neste PC foram guardadas em Backup (cópia de ";

    #[test]
    fn perda_do_lock_nao_sobrescreve_dados_do_outro_pc() {
        let (_t, raiz, local) = cenario();
        let banco = raiz.join("dados").join("frota.db");
        let (a, _) = Estado::arrancar(&raiz, &local.join("a"), "PC-A", t0());
        inserir(&a, t0(), "A1").unwrap();
        a.tick(s(3));
        inserir(&a, s(10), "A2").unwrap(); // por gravar
        // PC-A deixa de renovar; PC-X assume o lock abandonado, grava e fecha
        let (x, ax) = Estado::arrancar(&raiz, &local.join("x"), "PC-X", s(200));
        assert_eq!(ax, Arranque::Ok);
        inserir(&x, s(200), "X1").unwrap();
        x.fechar(s(201).date(), s(201)).unwrap();
        assert!(tem(&banco, "X1"));
        drop(a); // PC-A "crasha" com a alteração pendente
        let (a2, r) = Estado::arrancar(&raiz, &local.join("a"), "PC-A", s(300));
        assert_eq!(r, Arranque::Ok);
        // o banco continua com os dados do PC-X
        assert!(tem(&banco, "A1") && tem(&banco, "X1") && !tem(&banco, "A2"));
        // a alteração do PC-A ficou numa versão
        assert_eq!(versao_com(&raiz, "A2").as_deref(), Some("frota_2026-10-20_1205.db"));
        let est = a2.armazenamento();
        assert_eq!(
            est.aviso_arranque.as_deref(),
            Some(format!("{MSG_CONFLITO}20/10 12:05). Feche e volte a abrir o sistema.").as_str())
        );
        assert_eq!(est.erro, None);
        assert!(!est.so_consulta);
        assert!(!local.join("a").join("pendente").exists());
        // trabalha a partir do banco do PC-X
        let n: i64 = a2
            .ler(|c| Ok(c.query_row("SELECT COUNT(*) FROM carros", [], |r| r.get(0))?))
            .unwrap();
        assert_eq!(n, 2);
    }

    #[test]
    fn aviso_de_arranque_fica_ate_ser_dispensado() {
        let (_t, raiz, local) = cenario();
        let (a, _) = Estado::arrancar(&raiz, &local.join("a"), "PC-A", t0());
        inserir(&a, t0(), "A1").unwrap();
        a.tick(s(3));
        inserir(&a, s(10), "A2").unwrap(); // por gravar
        let (x, _) = Estado::arrancar(&raiz, &local.join("x"), "PC-X", s(200));
        inserir(&x, s(200), "X1").unwrap();
        x.fechar(s(201).date(), s(201)).unwrap();
        drop(a);
        let (a2, _) = Estado::arrancar(&raiz, &local.join("a"), "PC-A", s(300));
        let esperado = format!("{MSG_CONFLITO}20/10 12:05). Feche e volte a abrir o sistema.");
        assert_eq!(a2.armazenamento().aviso_arranque.as_deref(), Some(esperado.as_str()));
        // uma gravação bem-sucedida não apaga o aviso
        inserir(&a2, s(301), "A3").unwrap();
        a2.tick(s(304));
        assert!(tem(&raiz.join("dados").join("frota.db"), "A3"));
        let est = a2.armazenamento();
        assert_eq!(est.erro, None);
        assert_eq!(est.aviso_arranque.as_deref(), Some(esperado.as_str()));
        // só `dispensar_aviso` o apaga
        a2.dispensar_aviso();
        assert_eq!(a2.armazenamento().aviso_arranque, None);
    }

    #[test]
    fn tentar_de_novo_depois_de_bloqueado() {
        let (_t, raiz, local) = cenario();
        let (x, _) = Estado::arrancar(&raiz, &local.join("x"), "PC-X", t0());
        let (e, a) = Estado::arrancar(&raiz, &local.join("y"), "PC-Y", s(5));
        assert_eq!(a, Arranque::Bloqueado { pc: "PC-X".into() });
        // ainda bloqueado
        assert_eq!(e.tentar_arranque(s(6)), Arranque::Bloqueado { pc: "PC-X".into() });
        x.fechar(s(7).date(), s(7)).unwrap();
        assert_eq!(e.tentar_arranque(s(8)), Arranque::Ok);
        assert_eq!(e.arranque(), Arranque::Ok);
        inserir(&e, s(9), "AA-00-AA").unwrap();
        // já aberto: não volta a arrancar
        assert_eq!(e.tentar_arranque(s(10)), Arranque::Ok);
        let n: i64 = e
            .ler(|c| Ok(c.query_row("SELECT COUNT(*) FROM carros", [], |r| r.get(0))?))
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn tentar_de_novo_em_consulta_nao_arranca() {
        let (_t, raiz, local) = cenario();
        {
            let (x, _) = Estado::arrancar(&raiz, &local.join("x0"), "PC-X", t0());
            inserir(&x, t0(), "AA-00-AA").unwrap();
            x.tick(s(3));
        }
        let (e, _) = Estado::arrancar(&raiz, &local.join("y"), "PC-Y", s(5));
        e.abrir_consulta().unwrap();
        assert_eq!(e.tentar_arranque(s(6)), Arranque::Bloqueado { pc: "PC-X".into() });
        assert!(e.armazenamento().so_consulta);
    }

    #[test]
    fn retoma_apos_suspensao_deteta_alteracao_externa() {
        let (_t, raiz, local) = cenario();
        let banco = raiz.join("dados").join("frota.db");
        let (a, _) = Estado::arrancar(&raiz, &local.join("a"), "PC-A", t0());
        inserir(&a, t0(), "A1").unwrap();
        a.tick(s(3));
        inserir(&a, s(10), "A2").unwrap();
        // PC-A suspenso: sem ticks. PC-X abre, grava e fecha.
        let (x, _) = Estado::arrancar(&raiz, &local.join("x"), "PC-X", s(200));
        inserir(&x, s(200), "X1").unwrap();
        x.fechar(s(201).date(), s(201)).unwrap();
        a.tick(s(205));
        let est = a.armazenamento();
        assert!(est.so_consulta);
        assert_eq!(
            est.erro.as_deref(),
            Some(format!("{MSG_CONFLITO}20/10 12:03). Feche e volte a abrir o sistema.").as_str())
        );
        assert!(tem(&banco, "X1") && !tem(&banco, "A2"));
        assert_eq!(versao_com(&raiz, "A2").as_deref(), Some("frota_2026-10-20_1203.db"));
        assert!(!local.join("a").join("pendente").exists());
        assert_eq!(inserir(&a, s(206), "A3").unwrap_err().codigo, "so_consulta");
        a.fechar(s(207).date(), s(207)).unwrap();
        assert!(!tem(&banco, "A2"));
    }

    #[test]
    fn sem_alteracao_externa_grava_normalmente() {
        let (_t, raiz, local) = cenario();
        let banco = raiz.join("dados").join("frota.db");
        let (a, _) = Estado::arrancar(&raiz, &local, "PC-A", t0());
        inserir(&a, t0(), "A1").unwrap();
        a.tick(s(3));
        inserir(&a, s(10), "A2").unwrap();
        a.tick(s(13));
        assert_eq!(carros_no_banco(&banco), 2);
        // retoma depois de uma pausa longa sem alterações externas
        a.tick(s(200));
        inserir(&a, s(201), "A3").unwrap();
        a.tick(s(204));
        let est = a.armazenamento();
        assert!(!est.so_consulta);
        assert_eq!(est.erro, None);
        assert_eq!(carros_no_banco(&banco), 3);
        // pendente no arranque sem alteração externa: grava o trabalho
        inserir(&a, s(205), "A4").unwrap();
        drop(a);
        let (a2, r) = Estado::arrancar(&raiz, &local, "PC-A", s(210));
        assert_eq!(r, Arranque::Ok);
        assert_eq!(a2.armazenamento().erro, None);
        assert_eq!(carros_no_banco(&banco), 4);
    }

    #[test]
    fn versao_mais_recente_liberta_lock_e_permite_consulta() {
        let (_t, raiz, local) = cenario();
        let dados = raiz.join("dados");
        std::fs::create_dir_all(&dados).unwrap();
        let c = Connection::open(dados.join("frota.db")).unwrap();
        c.execute_batch("CREATE TABLE x(a); PRAGMA user_version = 99;").unwrap();
        drop(c);
        let (e, a) = Estado::arrancar(&raiz, &local, "PC-A", t0());
        assert_eq!(a, Arranque::VersaoMaisRecente);
        assert!(!dados.join("frota.lock").exists());
        e.abrir_consulta().unwrap();
        assert!(e.armazenamento().so_consulta);
    }

    #[test]
    fn pasta_inacessivel_da_sem_pasta() {
        let (t, _raiz, local) = cenario();
        let raiz = t.path().join("ficheiro");
        std::fs::write(&raiz, b"nao sou uma pasta").unwrap();
        let (_e, a) = Estado::arrancar(&raiz, &local, "PC-A", t0());
        assert!(matches!(a, Arranque::SemPasta { .. }), "{a:?}");
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
