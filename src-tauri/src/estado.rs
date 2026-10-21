//! Estado da aplicação: arranque, gravação automática (com atraso), bloqueio
//! entre PCs, modo só de consulta e fecho.
//!
//! Só a camada de comandos lê o relógio; aqui `agora`/`hoje` chegam por parâmetro.
//!
//! Ordem dos mutexes (para não haver deadlocks): `conn` → `lock` → restantes.
//!
//! **Nunca gravar por cima de dados gravados noutro PC.** Depois de cada
//! gravação própria guarda-se a assinatura do `banco` (tamanho e SHA-256 do
//! ficheiro inteiro, formato `v2`) em `<local>\assinatura`. Antes de cada
//! gravação (e no arranque, quando há `pendente`), se o `banco` existir e a
//! assinatura for diferente, houve uma gravação noutro PC: em vez de gravar, a
//! cópia de trabalho é guardada como versão (`copia_manual`) e o `pendente`
//! removido. Sem ficheiro de assinatura (primeira vez) ou com uma assinatura de
//! formato antigo não há conflito. A data de modificação não entra: o Google
//! Drive pode mexer-lhe sem mudar o conteúdo.

use crate::armazenamento::gravacao::{self, Aberto, Caminhos, EstadoBanco};
use crate::armazenamento::lock::{self, Lock};
use crate::armazenamento::versoes::{self, Versao};
use crate::erro::{Erro, Res};
use chrono::{DateTime, Duration, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use sha2::{Digest, Sha256};
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
/// Primeira linha do ficheiro de assinatura no formato atual.
const ASSINATURA_V2: &str = "v2";

/// Porque é que o `banco` não abriu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MotivoCorrompido {
    /// 0 bytes, ilegível ou incompleto: o Google Drive ainda não o descarregou
    /// (ficheiro só "online" ou sincronização a meio). Normalmente basta esperar.
    Vazio,
    /// Existe mas falha a verificação de integridade.
    Danificado,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "estado", rename_all = "camelCase")]
pub enum Arranque {
    Ok,
    Bloqueado {
        pc: String,
    },
    Corrompido {
        motivo: MotivoCorrompido,
        mensagem: String,
    },
    VersaoMaisRecente,
    SemPasta {
        mensagem: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EstadoArmazenamento {
    pub pasta: String,
    pub no_google_drive: bool,
    /// ISO local (`AAAA-MM-DDTHH:MM:SS`).
    pub ultima_gravacao: Option<String>,
    pub erro: Option<String>,
    /// Nota informativa (não é erro): a última gravação correu bem, mas algo
    /// secundário falhou (p.ex. o marcador `pendente` não saiu). Substituída a
    /// cada gravação bem-sucedida.
    pub nota_gravacao: Option<String>,
    /// Aviso do arranque (alterações deste PC guardadas em Backup por conflito,
    /// cópia local danificada, restauro). Fica até ser dispensado
    /// (`dispensar_aviso`); as gravações não o apagam.
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
    /// Ver `EstadoArmazenamento::nota_gravacao`.
    nota_gravacao: Mutex<Option<String>>,
    /// Ver `EstadoArmazenamento::aviso_arranque`.
    aviso_arranque: Mutex<Option<String>>,
    sujo: AtomicBool,
    raiz: PathBuf,
    /// `no_google_drive(raiz)`, calculado uma vez (a pasta não muda; evita ir ao
    /// disco a cada consulta do estado).
    no_drive: std::sync::OnceLock<bool>,
    pc: String,
    /// `<local>\assinatura`: assinatura do `banco` na última gravação própria.
    assinatura: PathBuf,
    /// Pasta local antiga (`%LOCALAPPDATA%\Frota`, partilhada por todas as pastas
    /// de dados) de onde se migram `trabalho.db`/`pendente`/`assinatura`.
    legado: Option<PathBuf>,
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
    Erro {
        campo: None,
        codigo: codigo.into(),
        mensagem: mensagem.into(),
    }
}

fn erro_so_consulta() -> Erro {
    erro(
        "so_consulta",
        "Aberto só para consulta — não é possível alterar dados.",
    )
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

fn msg_trabalho_danificado(p: &Path) -> String {
    format!(
        "A cópia local deste PC estava danificada: as alterações feitas aqui que ainda não tinham sido guardadas na pasta não puderam ser recuperadas. Os dados foram abertos a partir da pasta partilhada. O ficheiro danificado ficou em {}.",
        p.display()
    )
}

const MSG_BANCO_VOLTOU: &str = "O ficheiro de dados voltou a estar legível (provavelmente a sincronização do Google Drive terminou). Não foi restaurada nenhuma cópia: os dados foram abertos normalmente.";

/// Hora local -> UTC (o lock guarda RFC 3339).
pub(crate) fn para_utc(agora: NaiveDateTime) -> DateTime<Utc> {
    Local
        .from_local_datetime(&agora)
        .earliest()
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or_else(|| agora.and_utc())
}

/// A pasta parece estar dentro do Google Drive para computador? Pelo nome da
/// pasta do Drive (pt-PT, pt-BR, en, es) ou, em qualquer idioma, pela pasta
/// `.shortcut-targets-by-id` que o Drive cria na raiz da unidade virtual.
pub fn no_google_drive(pasta: &Path) -> bool {
    let s = format!("{}\\", pasta.to_string_lossy().replace('/', "\\")).to_lowercase();
    let pelo_nome = [
        r"\meu disco\",
        r"\o meu disco\",
        r"\meu drive\",
        r"\my drive\",
        r"\mi unidad\",
        r"\google drive\",
    ]
    .iter()
    .any(|p| s.contains(p));
    pelo_nome || pasta.ancestors().last().is_some_and(unidade_do_drive)
}

/// `unidade` (p.ex. `G:\`) é a unidade virtual do Google Drive para computador?
fn unidade_do_drive(unidade: &Path) -> bool {
    unidade.join(".shortcut-targets-by-id").is_dir()
}

/// `v2`, tamanho e SHA-256 (hex) do ficheiro inteiro; `None` se não existir ou
/// não se conseguir ler.
fn assinatura_de(p: &Path) -> Option<String> {
    let mut f = std::fs::File::open(p).ok()?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut tamanho: u64 = 0;
    loop {
        let n = f.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        tamanho += n as u64;
        h.update(&buf[..n]);
    }
    let hex: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
    Some(format!("{ASSINATURA_V2}\n{tamanho}\n{hex}"))
}

/// Assinatura no formato antigo (tamanho, mtime em ns, 100 bytes em hex). Só
/// serve para reconhecer, na migração, que a pasta local antiga pertence a este
/// `banco`.
fn assinatura_antiga_de(p: &Path) -> Option<String> {
    let m = std::fs::metadata(p).ok()?;
    let mtime = m
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_nanos();
    let mut cab = Vec::with_capacity(100);
    std::fs::File::open(p)
        .ok()?
        .take(100)
        .read_to_end(&mut cab)
        .ok()?;
    let hex: String = cab.iter().map(|b| format!("{b:02x}")).collect();
    Some(format!("{}\n{mtime}\n{hex}", m.len()))
}

/// Pasta local da pasta de dados `raiz`: `<base>\<12 hex do SHA-256 do caminho
/// canónico, em minúsculas>`. Cada pasta de dados (p.ex. a real e uma de
/// demonstração) tem a sua cópia de trabalho, `pendente` e `assinatura`.
pub fn pasta_local(base: &Path, raiz: &Path) -> PathBuf {
    let canonico = std::fs::canonicalize(raiz).unwrap_or_else(|_| raiz.to_path_buf());
    let texto = canonico.to_string_lossy().to_lowercase();
    let h = Sha256::digest(texto.as_bytes());
    let hex: String = h.iter().take(6).map(|b| format!("{b:02x}")).collect();
    base.join(hex)
}

fn mapear_erro_abertura(e: Erro) -> Arranque {
    match e.codigo.as_str() {
        gravacao::CODIGO_BANCO_VAZIO => Arranque::Corrompido {
            motivo: MotivoCorrompido::Vazio,
            mensagem: e.mensagem,
        },
        gravacao::CODIGO_CORROMPIDO => Arranque::Corrompido {
            motivo: MotivoCorrompido::Danificado,
            mensagem: e.mensagem,
        },
        "versao_mais_recente" => Arranque::VersaoMaisRecente,
        _ => Arranque::SemPasta {
            mensagem: e.mensagem,
        },
    }
}

impl Estado {
    fn novo(raiz: &Path, local: &Path, legado: Option<&Path>, pc: &str) -> Estado {
        Estado {
            conn: Mutex::new(None),
            caminhos: Caminhos::new(raiz, local),
            lock: Mutex::new(None),
            so_consulta: AtomicBool::new(false),
            ultima_gravacao: Mutex::new(None),
            erro_gravacao: Mutex::new(None),
            nota_gravacao: Mutex::new(None),
            aviso_arranque: Mutex::new(None),
            sujo: AtomicBool::new(false),
            raiz: raiz.to_path_buf(),
            no_drive: std::sync::OnceLock::new(),
            pc: pc.to_string(),
            assinatura: local.join("assinatura"),
            legado: legado.map(Path::to_path_buf),
            arranque: Mutex::new(Arranque::Ok),
            ultima_alteracao: Mutex::new(None),
            ultima_falha: Mutex::new(None),
            ultima_renovacao: Mutex::new(None),
            fechado: AtomicBool::new(false),
        }
    }

    /// Cria as pastas, adquire o lock e prepara a cópia de trabalho.
    #[cfg(test)]
    pub fn arrancar(
        raiz: &Path,
        local: &Path,
        pc: &str,
        agora: NaiveDateTime,
    ) -> (Estado, Arranque) {
        Estado::arrancar_com_legado(raiz, local, None, pc, agora)
    }

    /// Como `arrancar`, migrando primeiro a pasta local antiga `legado`.
    pub fn arrancar_com_legado(
        raiz: &Path,
        local: &Path,
        legado: Option<&Path>,
        pc: &str,
        agora: NaiveDateTime,
    ) -> (Estado, Arranque) {
        let e = Estado::novo(raiz, local, legado, pc);
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

    fn juntar_aviso(&self, texto: String) {
        let mut g = trava(&self.aviso_arranque);
        *g = Some(match g.take() {
            Some(a) => format!("{a}\n{texto}"),
            None => texto,
        });
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

    /// O `banco` foi gravado por outro PC desde a nossa última gravação? Uma
    /// assinatura em falta ou de outro formato conta como "sem assinatura".
    fn alterado_fora(&self) -> bool {
        let Ok(guardada) = std::fs::read_to_string(&self.assinatura) else {
            return false;
        };
        if guardada.lines().next() != Some(ASSINATURA_V2) {
            return false;
        }
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
            Err(e) => {
                return Arranque::SemPasta {
                    mensagem: e.mensagem,
                };
            }
        };
        // Pasta local antiga (antes de haver uma por pasta de dados).
        if let Some(a) = self.migrar_legado(agora) {
            self.juntar_aviso(a);
        }
        // Cópia local danificada com alterações por gravar: não é o banco
        // partilhado que está danificado. Põe-se de lado e abre-se do banco.
        match gravacao::isolar_trabalho_danificado(c, agora) {
            Ok(Some(p)) => self.juntar_aviso(msg_trabalho_danificado(&p)),
            Ok(None) => {}
            // Não se conseguiu ler a cópia local (em uso, sem acesso): não é o mesmo
            // que estar danificada. A mensagem já o diz; "Tentar de novo" repete.
            Err(e) if e.codigo == gravacao::CODIGO_TRABALHO_IO => {
                let _ = l.libertar();
                return Arranque::SemPasta {
                    mensagem: e.mensagem,
                };
            }
            Err(e) => {
                let _ = l.libertar();
                return Arranque::SemPasta {
                    mensagem: format!(
                        "A cópia local deste PC está danificada e não foi possível pô-la de lado ({}).",
                        e.mensagem
                    ),
                };
            }
        }
        // Alterações pendentes feitas sobre um banco que entretanto outro PC
        // gravou: guardam-se como versão e segue-se a partir do banco.
        if self.pendente() && c.trabalho.try_exists().unwrap_or(false) && self.alterado_fora() {
            match self.preservar_trabalho(agora) {
                Ok(()) => self.juntar_aviso(msg_conflito(agora)),
                Err(e) => {
                    let _ = l.libertar();
                    return Arranque::SemPasta {
                        mensagem: msg_conflito_sem_copia(&e),
                    };
                }
            }
        }
        match gravacao::abrir_trabalho(c) {
            Ok(aberto) => {
                self.instalar(slot, aberto, l, agora);
                Arranque::Ok
            }
            Err(e) => {
                let _ = l.libertar();
                mapear_erro_abertura(e)
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

    /// Migra a pasta local antiga (`legado`), partilhada por todas as pastas de
    /// dados. Requer o lock. Nunca deita fora alterações por gravar:
    /// - sem `pendente`: `trabalho.db`/`assinatura` antigos são só cache e saem;
    /// - com `pendente` e a assinatura antiga igual à do `banco` desta pasta (prova
    ///   de que a última gravação deste PC foi para aqui e ninguém gravou depois):
    ///   o trabalho passa a ser a cópia de trabalho desta pasta e é gravado já;
    ///   se isso não for possível (esta pasta já tem alterações por gravar, ou a
    ///   migração falhou) vai para `versoes\frota_legado_*.db` (nunca podado);
    /// - com `pendente` sem essa prova (pode ser de outra pasta): NÃO entra na
    ///   pasta partilhada. Fica neste PC, em `<legado>\legado_por_gravar_*.db`
    ///   (com os auxiliares), o `pendente` antigo sai e avisa-se com o caminho.
    ///
    /// Devolve o aviso a mostrar, se houver.
    fn migrar_legado(&self, agora: NaiveDateTime) -> Option<String> {
        let leg = self.legado.as_ref()?;
        let c = &self.caminhos;
        let l_trab = leg.join("trabalho.db");
        let l_pend = leg.join("pendente");
        let l_ass = leg.join("assinatura");
        if !l_trab.exists() && !l_pend.exists() && !l_ass.exists() {
            return None;
        }
        let limpar = || {
            let _ = gravacao::limpar_trabalho(&l_trab, true);
            let _ = std::fs::remove_file(&l_ass);
            let _ = std::fs::remove_file(leg.join("assinatura.tmp"));
            let _ = std::fs::remove_file(&l_pend);
        };
        if !(l_pend.exists() && l_trab.exists()) {
            // Auxiliares sem o principal, com `pendente`: restos de uma
            // renomeação parcial (o principal está noutro sítio). Não se apagam.
            let orfaos =
                l_pend.exists() && gravacao::auxiliares_de(&l_trab).map_or(true, |v| !v.is_empty());
            if orfaos {
                log::warn!("pasta local antiga com auxiliares sem trabalho.db: deixada como está");
                return None;
            }
            limpar();
            log::info!("pasta local antiga sem alterações por gravar: removida");
            return None;
        }
        let comprovada = std::fs::read_to_string(&l_ass)
            .ok()
            .zip(assinatura_antiga_de(&c.banco))
            .is_some_and(|(g, a)| g == a);
        if !comprovada {
            return Some(self.guardar_legado_local(leg, &l_trab, &l_pend, &l_ass, agora));
        }
        if !self.pendente() {
            // Marcador primeiro: um trabalho já movido sem marcador seria substituído pelo banco.
            let r = gravacao::limpar_trabalho(&c.trabalho, true)
                .and_then(|_| gravacao::marcar_pendente(c))
                .and_then(|_| gravacao::mover_base(&l_trab, &c.trabalho));
            match r {
                Ok(()) => {
                    // O trabalho baseia-se no banco atual (assinatura antiga igual).
                    self.registar_assinatura();
                    limpar();
                    log::info!("pasta local antiga migrada para {}", c.trabalho.display());
                    return None;
                }
                // O principal já está em `c.trabalho` (com o marcador): segue-se com
                // ele, e o que ficou na pasta antiga não se apaga.
                Err(e) if e.codigo == gravacao::CODIGO_MOVER_PARCIAL => {
                    log::error!("migração parcial da pasta local antiga: {}", e.mensagem);
                    self.registar_assinatura();
                    return Some(format!(
                        "As alterações por gravar de uma versão anterior do programa foram passadas para esta pasta, mas alguns ficheiros auxiliares ficaram em {} ({}). Não os apague.",
                        leg.display(),
                        e.mensagem
                    ));
                }
                Err(e) => {
                    // Nada foi movido: o marcador criado agora não pode fazer valer
                    // uma cópia de trabalho que não existe.
                    let _ = std::fs::remove_file(&c.pendente);
                    log::warn!(
                        "não foi possível migrar a pasta local antiga: {}",
                        e.mensagem
                    );
                }
            }
        }
        let r = Connection::open_with_flags(&l_trab, OpenFlags::SQLITE_OPEN_READ_WRITE)
            .map_err(Erro::from)
            .and_then(|conn| versoes::copia_legado(&conn, c, agora));
        match r {
            Ok(nome) => {
                limpar();
                Some(format!(
                    "Foram encontradas alterações deste PC que não tinham sido guardadas (de uma versão anterior do programa). Foram guardadas em Backup (cópia {nome}); restaure-a para as recuperar."
                ))
            }
            Err(e) => {
                log::warn!("pasta local antiga não preservada: {}", e.mensagem);
                Some(format!(
                    "Foram encontradas alterações deste PC por gravar de uma versão anterior do programa, mas não foi possível guardá-las em Backup ({}). Ficaram em {}.",
                    e.mensagem,
                    l_trab.display()
                ))
            }
        }
    }

    /// Alterações antigas por gravar sem prova de que são desta pasta de dados:
    /// ficam neste PC (`<legado>\legado_por_gravar_AAAA-MM-DD_HHMM.db`, com os
    /// auxiliares) e nunca vão para a pasta partilhada. Devolve o aviso.
    fn guardar_legado_local(
        &self,
        leg: &Path,
        l_trab: &Path,
        l_pend: &Path,
        l_ass: &Path,
        agora: NaiveDateTime,
    ) -> String {
        let base = format!("legado_por_gravar_{}", agora.format("%Y-%m-%d_%H%M"));
        let r = gravacao::nome_livre(leg, &base).and_then(|destino| {
            gravacao::mover_base(l_trab, &destino)?;
            Ok(destino)
        });
        match r {
            Ok(destino) => {
                let _ = std::fs::remove_file(l_pend);
                let _ = std::fs::remove_file(l_ass);
                let _ = std::fs::remove_file(leg.join("assinatura.tmp"));
                log::warn!(
                    "pasta local antiga de origem incerta guardada em {}",
                    destino.display()
                );
                format!(
                    "Foram encontradas alterações deste PC que não tinham sido guardadas (de uma versão anterior do programa), mas não foi possível confirmar que são desta pasta de dados. Não foram misturadas com estes dados: ficaram guardadas neste computador em {}.",
                    destino.display()
                )
            }
            Err(e) => {
                log::warn!("pasta local antiga não posta de lado: {}", e.mensagem);
                format!(
                    "Foram encontradas alterações deste PC por gravar de uma versão anterior do programa, de origem incerta. Não foram misturadas com estes dados e ficaram em {} ({}).",
                    l_trab.display(),
                    e.mensagem
                )
            }
        }
    }

    /// Passa a trabalhar sobre a ligação aberta com o lock `l` (modo normal).
    fn instalar(
        &self,
        slot: &mut Option<Connection>,
        aberto: Aberto,
        l: Lock,
        agora: NaiveDateTime,
    ) {
        *slot = Some(aberto.conn);
        *trava(&self.lock) = Some(l);
        *trava(&self.ultima_renovacao) = Some(agora);
        self.so_consulta.store(false, Ordering::SeqCst);
        if aberto.gravado || !self.pendente() {
            // trabalho == banco (copiado agora, ou as alterações pendentes foram
            // gravadas mesmo que o marcador não tenha saído).
            self.registar_assinatura();
            if aberto.gravado {
                *trava(&self.ultima_gravacao) = Some(agora);
            }
        } else {
            // A gravação pendente falhou durante o arranque: volta a ser tentada.
            // A assinatura guardada continua a ser a do banco em que o trabalho se baseia.
            *trava(&self.ultima_alteracao) = Some(agora);
            self.sujo.store(true, Ordering::SeqCst);
        }
        if let Some(a) = aberto.aviso {
            *trava(&self.nota_gravacao) = Some(a);
        }
    }

    pub fn arranque(&self) -> Arranque {
        trava(&self.arranque).clone()
    }

    /// "Tentar de novo": depois de um arranque `Bloqueado`, `SemPasta` ou
    /// `Corrompido { Vazio }` (o Google Drive ainda não tinha descarregado o
    /// ficheiro), sem dados abertos nem em consulta, volta a tentar arrancar; nos
    /// outros casos (incluindo `Corrompido { Danificado }`) devolve o estado atual.
    pub fn tentar_arranque(&self, agora: NaiveDateTime) -> Arranque {
        let mut g = trava(&self.conn);
        let atual = self.arranque();
        let repetir = g.is_none()
            && !self.fechado.load(Ordering::SeqCst)
            && matches!(
                atual,
                Arranque::Bloqueado { .. }
                    | Arranque::SemPasta { .. }
                    | Arranque::Corrompido {
                        motivo: MotivoCorrompido::Vazio,
                        ..
                    }
            );
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
    pub fn mutar<T>(
        &self,
        agora: NaiveDateTime,
        f: impl FnOnce(&mut Connection) -> Res<T>,
    ) -> Res<T> {
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
            // Com aviso = gravado, mas o marcador ficou: não é uma falha.
            Ok(aviso) => {
                self.registar_assinatura();
                self.sujo.store(false, Ordering::SeqCst);
                *trava(&self.ultima_gravacao) = Some(agora);
                *trava(&self.erro_gravacao) = None;
                *trava(&self.nota_gravacao) = aviso;
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

    /// Antes de trocar o executável: grava (com a verificação de conflito de
    /// `gravar_com`) e faz `frota_antes-da-v{versao}.db`. Qualquer erro (incluindo
    /// `conflito`) aborta a atualização; o lock e os dados continuam abertos.
    pub fn gravar_antes_de_atualizar(&self, versao: &str, agora: NaiveDateTime) -> Res<()> {
        if self.so_consulta.load(Ordering::SeqCst) {
            return Err(erro_so_consulta());
        }
        let g = trava(&self.conn);
        let conn = g.as_ref().ok_or_else(sem_dados)?;
        if self.fechado.load(Ordering::SeqCst) || self.so_consulta.load(Ordering::SeqCst) {
            return Err(erro_so_consulta());
        }
        // Mesmo sem alterações por gravar: se outro PC gravou, não seguir.
        if self.alterado_fora() {
            return Err(self.entrar_em_conflito(conn, agora));
        }
        if self.sujo.load(Ordering::SeqCst) || self.pendente() {
            self.gravar_com(conn, agora)?;
        }
        versoes::copia_antes_de(&self.caminhos, versao)
    }

    pub fn so_consulta(&self) -> bool {
        self.so_consulta.load(Ordering::SeqCst)
    }

    pub fn armazenamento(&self) -> EstadoArmazenamento {
        EstadoArmazenamento {
            pasta: self.raiz.display().to_string(),
            no_google_drive: *self.no_drive.get_or_init(|| no_google_drive(&self.raiz)),
            ultima_gravacao: trava(&self.ultima_gravacao)
                .map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string()),
            erro: trava(&self.erro_gravacao).clone(),
            nota_gravacao: trava(&self.nota_gravacao).clone(),
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

    /// Dá/tira o nome de uma versão. Recusado em consulta (a pasta é de outro PC).
    /// Segura `conn` para não se cruzar com cópias, restauros e a poda.
    pub fn nomear_versao(&self, nome: &str, rotulo: &str) -> Res<String> {
        let _g = trava(&self.conn);
        if self.so_consulta.load(Ordering::SeqCst) {
            return Err(erro_so_consulta());
        }
        versoes::nomear(&self.caminhos, nome, rotulo)
    }

    /// Apaga uma versão. Recusado em consulta.
    pub fn apagar_versao(&self, nome: &str) -> Res<()> {
        let _g = trava(&self.conn);
        if self.so_consulta.load(Ordering::SeqCst) {
            return Err(erro_so_consulta());
        }
        versoes::apagar(&self.caminhos, nome)
    }

    /// Repõe uma versão. Em modo normal carrega-a na ligação aberta (com cópia de
    /// segurança do estado atual).
    ///
    /// Depois de um arranque `Corrompido` (sem ligação): adquire o lock e volta a
    /// verificar o `banco` — se entretanto ficou válido (p.ex. a sincronização do
    /// Drive terminou) NÃO restaura nada, abre normalmente e avisa. Senão move o
    /// `banco` atual para `versoes\frota_danificado_*.db` (danificado) ou
    /// `versoes\frota_posto_de_parte_*.db` (vazio/ilegível) — nunca se perde —, restaura
    /// para uma ligação vazia em memória (sem cópia de segurança, que seria de uma
    /// base vazia), o que grava `banco`, e continua o arranque normal. Se o restauro
    /// falhar, o ficheiro posto de lado volta ao lugar.
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
            return match r {
                // `Ok` = o banco foi gravado (mesmo que o marcador tenha ficado).
                Ok(aviso) => {
                    self.registar_assinatura();
                    self.sujo.store(false, Ordering::SeqCst);
                    *trava(&self.ultima_gravacao) = Some(agora);
                    *trava(&self.erro_gravacao) = None;
                    *trava(&self.nota_gravacao) = aviso;
                    *trava(&self.ultima_falha) = None;
                    Ok(())
                }
                // A ligação pode ter mudado: gravar o que lá estiver.
                Err(e) => {
                    self.registar(agora);
                    Err(e)
                }
            };
        }
        if !matches!(self.arranque(), Arranque::Corrompido { .. }) {
            return Err(sem_dados());
        }
        let l = match lock::adquirir(&self.caminho_lock(), &self.pc, para_utc(agora)) {
            Ok(l) => l,
            Err(e) if e.codigo == "bloqueado" => {
                return Err(erro(
                    "bloqueado",
                    format!(
                        "O sistema está aberto no PC «{}». Feche-o lá primeiro.",
                        e.mensagem
                    ),
                ));
            }
            Err(e) => return Err(e),
        };
        match self.restaurar_no_arranque(nome, agora) {
            Ok((aberto, aviso)) => {
                self.instalar(&mut g, aberto, l, agora);
                *trava(&self.ultima_gravacao) = Some(agora);
                *trava(&self.arranque) = Arranque::Ok;
                self.juntar_aviso(aviso);
                Ok(())
            }
            Err(e) => {
                let _ = l.libertar();
                Err(e)
            }
        }
    }

    /// Parte de `restaurar_versao` depois de um arranque `Corrompido`, já com o
    /// lock. Devolve a cópia de trabalho aberta e o aviso a mostrar.
    fn restaurar_no_arranque(&self, nome: &str, agora: NaiveDateTime) -> Res<(Aberto, String)> {
        let c = &self.caminhos;
        let estado = gravacao::estado_banco(&c.banco)?;
        if estado == EstadoBanco::Valido {
            log::info!("restauro no arranque: o banco voltou a ser válido; nada restaurado");
            return Ok((gravacao::abrir_trabalho(c)?, MSG_BANCO_VOLTOU.to_string()));
        }
        // Vazio/ilegível -> `frota_posto_de_parte_*` (pode vir a ter dados bons);
        // danificado -> `frota_danificado_*`. Um erro deixa o banco no lugar
        // (salvo `mover_parcial`, e aí o arranque recusa criar uma base vazia).
        let vazio = estado == EstadoBanco::Vazio;
        let posto_de_lado = versoes::guardar_danificado(c, agora, vazio)?;
        let r = (|| {
            // Num arranque `Corrompido` não havia alterações pendentes (com
            // pendente abre-se o trabalho, não o banco): a cópia de trabalho é
            // cache. Sai já, para um marcador que não saia não a fazer valer.
            gravacao::limpar_trabalho(&c.trabalho, true)?;
            let mut mem = Connection::open_in_memory()?;
            versoes::restaurar(&mut mem, c, nome, false, agora)?;
            drop(mem);
            gravacao::abrir_trabalho(c)
        })();
        match r {
            Ok(aberto) => {
                let aviso = match &posto_de_lado {
                    Some(n) if vazio => format!(
                        "Foi restaurada uma cópia de segurança. O ficheiro de dados que estava vazio ou incompleto foi posto de parte em Backup ({n}); se a sincronização o completar, pode restaurá-lo de lá."
                    ),
                    Some(n) => format!(
                        "Foi restaurada uma cópia de segurança. O ficheiro danificado foi guardado à parte em Backup ({n})."
                    ),
                    None => "Foi restaurada uma cópia de segurança.".to_string(),
                };
                Ok((aberto, aviso))
            }
            Err(e) => {
                // Restauro falhou: o ficheiro original volta ao lugar (se o banco
                // continua ausente), para nunca se arrancar com uma base vazia.
                if let Some(n) = &posto_de_lado
                    && !c.banco.exists()
                {
                    let origem = c.dados.join("versoes").join(n);
                    if let Err(e2) = gravacao::mover_base(&origem, &c.banco) {
                        log::error!("não foi possível repor {n}: {}", e2.mensagem);
                    }
                }
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
        c.query_row("SELECT COUNT(*) FROM carros", [], |r| r.get(0))
            .unwrap()
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
        assert_eq!(
            inserir(&e, t0(), "AA-00-AA").unwrap_err().codigo,
            "sem_dados"
        );
        assert!(
            std::fs::read_to_string(dados.join("frota.lock"))
                .unwrap()
                .starts_with("PC-X\n")
        );
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
            Some(
                "O sistema foi aberto noutro PC (PC-X). As alterações feitas neste PC foram guardadas em Backup (cópia de 20/10 12:00). Feche e volte a abrir o sistema."
            )
        );
        // a alteração por gravar ficou numa versão e o pendente foi removido
        assert_eq!(
            versao_com(&raiz, "AA-00-AA").as_deref(),
            Some("frota_2026-10-20_1200.db")
        );
        assert!(!e.caminhos().pendente.exists());
        // não grava por cima dos dados do outro PC
        e.tick(s(40));
        let banco = e.caminhos().banco.clone();
        assert!(!banco.exists() || carros_no_banco(&banco) == 0);
        assert_eq!(
            inserir(&e, s(41), "BB-11-BB").unwrap_err().codigo,
            "so_consulta"
        );
        e.fechar(s(50).date(), s(50)).unwrap();
        assert!(
            std::fs::read_to_string(&lock_p)
                .unwrap()
                .starts_with("PC-X\n")
        );
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
        assert!(
            matches!(&a, Arranque::Corrompido { motivo: MotivoCorrompido::Danificado, mensagem } if mensagem.contains("danificado")),
            "{a:?}"
        );
        assert!(!raiz.join("dados").join("frota.lock").exists());
        // danificado não volta a ser tentado
        assert_eq!(e.tentar_arranque(s(11)), a);
        let v = e.listar_versoes().unwrap();
        assert!(v.iter().any(|x| x.nome == "frota_2026-10-20.db"));
        e.restaurar_versao("frota_2026-10-20.db", s(80)).unwrap();
        assert_eq!(e.arranque(), Arranque::Ok);
        // o ficheiro danificado ficou guardado à parte (intacto); sem "cópia de
        // segurança" de uma base vazia
        let nomes: Vec<_> = e
            .listar_versoes()
            .unwrap()
            .into_iter()
            .map(|v| v.nome)
            .collect();
        assert_eq!(
            nomes,
            vec!["frota_danificado_2026-10-20_1201.db", "frota_2026-10-20.db"]
        );
        let guardado = raiz
            .join("dados")
            .join("versoes")
            .join("frota_danificado_2026-10-20_1201.db");
        assert_eq!(std::fs::read(&guardado).unwrap(), vec![0xABu8; 4096]);
        assert!(
            e.armazenamento()
                .aviso_arranque
                .unwrap()
                .contains("frota_danificado_2026-10-20_1201.db")
        );
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
        assert_eq!(
            versao_com(&raiz, "A2").as_deref(),
            Some("frota_2026-10-20_1205.db")
        );
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
        assert_eq!(
            a2.armazenamento().aviso_arranque.as_deref(),
            Some(esperado.as_str())
        );
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
        assert_eq!(
            e.tentar_arranque(s(6)),
            Arranque::Bloqueado { pc: "PC-X".into() }
        );
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
        assert_eq!(
            e.tentar_arranque(s(6)),
            Arranque::Bloqueado { pc: "PC-X".into() }
        );
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
        assert_eq!(
            versao_com(&raiz, "A2").as_deref(),
            Some("frota_2026-10-20_1203.db")
        );
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
        c.execute_batch("CREATE TABLE x(a); PRAGMA user_version = 99;")
            .unwrap();
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
    fn antes_de_atualizar_grava_e_faz_copia_antes_da() {
        let (_t, raiz, local) = cenario();
        let (e, _) = Estado::arrancar(&raiz, &local, "PC-A", t0());
        inserir(&e, t0(), "AA-00-AA").unwrap(); // por gravar
        e.gravar_antes_de_atualizar("1.3.0", s(1)).unwrap();
        let banco = e.caminhos().banco.clone();
        assert_eq!(carros_no_banco(&banco), 1);
        assert!(!e.caminhos().pendente.exists());
        let antes = raiz
            .join("dados")
            .join("versoes")
            .join("frota_antes-da-v1.3.0.db");
        assert_eq!(carros_no_banco(&antes), 1);
        // continua aberto normalmente (se a troca falhar, a app continua)
        assert!(!e.armazenamento().so_consulta);
        inserir(&e, s(2), "BB-11-BB").unwrap();
    }

    #[test]
    fn antes_de_atualizar_com_conflito_aborta() {
        let (_t, raiz, local) = cenario();
        let banco = raiz.join("dados").join("frota.db");
        let (a, _) = Estado::arrancar(&raiz, &local.join("a"), "PC-A", t0());
        inserir(&a, t0(), "A1").unwrap();
        a.tick(s(3));
        inserir(&a, s(10), "A2").unwrap();
        // outro PC gravou o banco entretanto
        let (x, _) = Estado::arrancar(&raiz, &local.join("x"), "PC-X", s(200));
        inserir(&x, s(200), "X1").unwrap();
        x.fechar(s(201).date(), s(201)).unwrap();
        let err = a.gravar_antes_de_atualizar("1.3.0", s(202)).unwrap_err();
        assert_eq!(err.codigo, "conflito");
        assert!(tem(&banco, "X1") && !tem(&banco, "A2"));
        assert!(
            !raiz
                .join("dados")
                .join("versoes")
                .join("frota_antes-da-v1.3.0.db")
                .exists()
        );
    }

    #[test]
    fn antes_de_atualizar_recusado_em_consulta() {
        let (_t, raiz, local) = cenario();
        {
            let (x, _) = Estado::arrancar(&raiz, &local.join("x"), "PC-X", t0());
            inserir(&x, t0(), "AA-00-AA").unwrap();
            x.tick(s(3));
        }
        let (e, _) = Estado::arrancar(&raiz, &local.join("y"), "PC-Y", s(5));
        e.abrir_consulta().unwrap();
        assert_eq!(
            e.gravar_antes_de_atualizar("1.3.0", s(6))
                .unwrap_err()
                .codigo,
            "so_consulta"
        );
    }

    #[test]
    fn deteta_google_drive() {
        assert!(no_google_drive(Path::new(r"G:\O meu disco\Frota")));
        assert!(no_google_drive(Path::new(r"G:\My Drive\Frota")));
        assert!(no_google_drive(Path::new(r"C:\Users\x\Google Drive\Frota")));
        assert!(no_google_drive(Path::new(r"g:\meu disco\frota")));
        assert!(no_google_drive(Path::new(r"G:\Meu Drive\Frota Teste")));
        // qualquer idioma: pela pasta que o Drive cria na raiz da unidade
        let t = tempfile::tempdir().unwrap();
        assert!(!unidade_do_drive(t.path()));
        std::fs::create_dir(t.path().join(".shortcut-targets-by-id")).unwrap();
        assert!(unidade_do_drive(t.path()));
        assert!(!no_google_drive(Path::new(r"C:\Frota")));
    }

    // ---------- C1: banco vazio / danificado no arranque ----------

    /// Banco com 1 carro gravado, cópia diária de hoje; devolve o caminho do banco.
    fn banco_com_um_carro(raiz: &Path, local: &Path) -> std::path::PathBuf {
        let (e, _) = Estado::arrancar(raiz, local, "PC-A", t0());
        inserir(&e, t0(), "AA-00-AA").unwrap();
        e.fechar(t0().date(), s(1)).unwrap();
        raiz.join("dados").join("frota.db")
    }

    #[test]
    fn vazio_tentar_de_novo_abre_quando_o_ficheiro_chega() {
        let (_t, raiz, local) = cenario();
        let banco = banco_com_um_carro(&raiz, &local);
        let bom = std::fs::read(&banco).unwrap();
        std::fs::write(&banco, b"").unwrap(); // placeholder do Drive
        let (e, a) = Estado::arrancar(&raiz, &local, "PC-A", s(10));
        match &a {
            Arranque::Corrompido {
                motivo: MotivoCorrompido::Vazio,
                mensagem,
            } => assert!(mensagem.contains("Disponível offline"), "{mensagem}"),
            outro => panic!("{outro:?}"),
        }
        assert!(!raiz.join("dados").join("frota.lock").exists());
        // ainda vazio
        assert_eq!(e.tentar_arranque(s(11)), a);
        // a sincronização terminou
        std::fs::write(&banco, &bom).unwrap();
        assert_eq!(e.tentar_arranque(s(12)), Arranque::Ok);
        let n: i64 = e
            .ler(|c| Ok(c.query_row("SELECT COUNT(*) FROM carros", [], |r| r.get(0))?))
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn restauro_no_arranque_com_banco_valido_outra_vez_nao_restaura() {
        let (_t, raiz, local) = cenario();
        let banco = banco_com_um_carro(&raiz, &local);
        let bom = std::fs::read(&banco).unwrap();
        // uma versão mais antiga, sem carros
        let versoes_dir = raiz.join("dados").join("versoes");
        {
            let c = Connection::open(versoes_dir.join("frota_2026-10-01.db")).unwrap();
            let mut c = c;
            crate::migracoes::migrar(&mut c).unwrap();
        }
        std::fs::write(&banco, b"").unwrap();
        let (e, a) = Estado::arrancar(&raiz, &local, "PC-A", s(10));
        assert!(matches!(a, Arranque::Corrompido { .. }));
        // entretanto o Drive acabou de descarregar o ficheiro bom
        std::fs::write(&banco, &bom).unwrap();
        e.restaurar_versao("frota_2026-10-01.db", s(20)).unwrap();
        assert_eq!(e.arranque(), Arranque::Ok);
        assert_eq!(
            std::fs::read(&banco).unwrap(),
            bom,
            "o banco bom não foi substituído"
        );
        assert_eq!(carros_no_banco(&banco), 1);
        assert!(!std::fs::read_dir(&versoes_dir).unwrap().any(|f| {
            f.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("frota_danificado_")
        }));
        assert_eq!(
            e.armazenamento().aviso_arranque.as_deref(),
            Some(MSG_BANCO_VOLTOU)
        );
    }

    #[test]
    fn restauro_no_arranque_que_falha_repoe_o_ficheiro_original() {
        let (_t, raiz, local) = cenario();
        let banco = banco_com_um_carro(&raiz, &local);
        let versoes_dir = raiz.join("dados").join("versoes");
        std::fs::write(versoes_dir.join("frota_2026-10-02.db"), vec![0xCDu8; 4096]).unwrap();
        std::fs::write(&banco, vec![0xABu8; 4096]).unwrap();
        let (e, _) = Estado::arrancar(&raiz, &local, "PC-A", s(10));
        let err = e
            .restaurar_versao("frota_2026-10-02.db", s(20))
            .unwrap_err();
        assert_eq!(err.codigo, "corrompido");
        assert_eq!(std::fs::read(&banco).unwrap(), vec![0xABu8; 4096]);
        assert!(matches!(e.arranque(), Arranque::Corrompido { .. }));
        assert!(!raiz.join("dados").join("frota.lock").exists());
    }

    /// v0.1.1: no caso "vazio" o ficheiro é posto de parte (não "danificado") e,
    /// se depois ficar com dados bons, pode ser restaurado.
    #[test]
    fn vazio_restaurado_fica_posto_de_parte_e_e_restauravel_depois() {
        let (_t, raiz, local) = cenario();
        let banco = banco_com_um_carro(&raiz, &local);
        let bom = std::fs::read(&banco).unwrap();
        std::fs::write(&banco, b"").unwrap();
        let (e, a) = Estado::arrancar(&raiz, &local, "PC-A", s(10));
        assert!(matches!(
            a,
            Arranque::Corrompido {
                motivo: MotivoCorrompido::Vazio,
                ..
            }
        ));
        e.restaurar_versao("frota_2026-10-20.db", s(60)).unwrap();
        assert_eq!(e.arranque(), Arranque::Ok);
        let posto = "frota_posto_de_parte_2026-10-20_1201.db";
        let aviso = e.armazenamento().aviso_arranque.unwrap();
        assert!(
            aviso.contains(posto) && aviso.contains("posto de parte"),
            "{aviso}"
        );
        assert!(!aviso.contains("danificado"), "{aviso}");
        let p = raiz.join("dados").join("versoes").join(posto);
        let v = e.listar_versoes().unwrap();
        assert!(!v.iter().find(|x| x.nome == posto).unwrap().restauravel);
        // a sincronização acaba por completar o ficheiro posto de parte
        std::fs::write(&p, &bom).unwrap();
        let v = e.listar_versoes().unwrap();
        assert!(v.iter().find(|x| x.nome == posto).unwrap().restauravel);
        inserir(&e, s(70), "BB-11-BB").unwrap();
        e.restaurar_versao(posto, s(120)).unwrap();
        let n: i64 = e
            .ler(|c| Ok(c.query_row("SELECT COUNT(*) FROM carros", [], |r| r.get(0))?))
            .unwrap();
        assert_eq!(n, 1);
        // com cópia de segurança do estado anterior (2 carros)
        assert_eq!(
            versao_com(&raiz, "BB-11-BB").as_deref(),
            Some("frota_2026-10-20_1202.db")
        );
    }

    /// v0.1.1: um erro de leitura da cópia local não é "cópia local danificada";
    /// é `SemPasta` (pode tentar-se de novo) e nada é posto de lado.
    #[test]
    fn trabalho_ilegivel_por_io_da_sem_pasta_e_tentar_de_novo_recupera() {
        let (_t, raiz, local) = cenario();
        banco_com_um_carro(&raiz, &local);
        std::fs::remove_file(local.join("trabalho.db")).unwrap();
        std::fs::create_dir_all(local.join("trabalho.db")).unwrap();
        std::fs::write(local.join("pendente"), b"").unwrap();
        let (e, a) = Estado::arrancar(&raiz, &local, "PC-A", s(10));
        match &a {
            Arranque::SemPasta { mensagem } => {
                assert!(mensagem.contains("Não foi possível ler"), "{mensagem}");
                assert!(!mensagem.contains("danificada"), "{mensagem}");
            }
            outro => panic!("{outro:?}"),
        }
        assert!(!raiz.join("dados").join("frota.lock").exists());
        assert!(local.join("pendente").exists());
        // o obstáculo desaparece: "Tentar de novo" abre normalmente
        std::fs::remove_dir(local.join("trabalho.db")).unwrap();
        assert_eq!(e.tentar_arranque(s(20)), Arranque::Ok);
    }

    #[test]
    fn corrompido_serializa_motivo_e_mensagem() {
        let a = Arranque::Corrompido {
            motivo: MotivoCorrompido::Vazio,
            mensagem: "m".into(),
        };
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            r#"{"estado":"corrompido","motivo":"vazio","mensagem":"m"}"#
        );
    }

    // ---------- I1: cópia de trabalho danificada ----------

    #[test]
    fn trabalho_danificado_com_pendente_abre_do_banco_e_avisa() {
        let (_t, raiz, local) = cenario();
        let banco = banco_com_um_carro(&raiz, &local);
        std::fs::write(local.join("trabalho.db"), vec![0xCDu8; 4096]).unwrap();
        std::fs::write(local.join("pendente"), b"").unwrap();
        let (e, a) = Estado::arrancar(&raiz, &local, "PC-A", s(60));
        assert_eq!(a, Arranque::Ok);
        let aviso = e.armazenamento().aviso_arranque.unwrap();
        let guardado = local.join("trabalho_danificado_2026-10-20_1201.db");
        assert!(aviso.contains("não puderam ser recuperadas"), "{aviso}");
        assert!(aviso.contains(&guardado.display().to_string()), "{aviso}");
        assert_eq!(std::fs::read(&guardado).unwrap(), vec![0xCDu8; 4096]);
        assert!(!local.join("pendente").exists());
        assert_eq!(carros_no_banco(&banco), 1);
        let n: i64 = e
            .ler(|c| Ok(c.query_row("SELECT COUNT(*) FROM carros", [], |r| r.get(0))?))
            .unwrap();
        assert_eq!(n, 1);
    }

    // ---------- I2: assinatura por conteúdo ----------

    #[test]
    fn mtime_alterado_com_o_mesmo_conteudo_nao_e_conflito() {
        let (_t, raiz, local) = cenario();
        let banco = raiz.join("dados").join("frota.db");
        let (a, _) = Estado::arrancar(&raiz, &local, "PC-A", t0());
        inserir(&a, t0(), "A1").unwrap();
        a.tick(s(3));
        // o Drive "toca" no ficheiro sem mudar o conteúdo
        let f = std::fs::OpenOptions::new()
            .write(true)
            .open(&banco)
            .unwrap();
        f.set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(86_400))
            .unwrap();
        drop(f);
        inserir(&a, s(10), "A2").unwrap();
        a.tick(s(13));
        let est = a.armazenamento();
        assert_eq!(est.erro, None);
        assert!(!est.so_consulta);
        assert_eq!(carros_no_banco(&banco), 2);
    }

    #[test]
    fn conteudo_diferente_do_mesmo_tamanho_e_conflito() {
        let (_t, raiz, local) = cenario();
        let banco = raiz.join("dados").join("frota.db");
        let (a, _) = Estado::arrancar(&raiz, &local, "PC-A", t0());
        inserir(&a, t0(), "A1").unwrap();
        a.tick(s(3));
        // outro PC grava "A9" no lugar de "A1": mesmo tamanho, conteúdo diferente
        {
            let c = Connection::open(&banco).unwrap();
            c.execute("UPDATE carros SET matricula = 'A9'", []).unwrap();
        }
        inserir(&a, s(10), "A2").unwrap();
        a.tick(s(13));
        assert!(a.armazenamento().so_consulta);
        assert!(tem(&banco, "A9") && !tem(&banco, "A2"));
    }

    #[test]
    fn assinatura_de_formato_antigo_conta_como_sem_assinatura() {
        let (_t, raiz, local) = cenario();
        let banco = banco_com_um_carro(&raiz, &local);
        std::fs::write(local.join("assinatura"), "4096\n123\nabcdef").unwrap();
        std::fs::write(local.join("pendente"), b"").unwrap();
        let (e, a) = Estado::arrancar(&raiz, &local, "PC-A", s(10));
        assert_eq!(a, Arranque::Ok);
        assert_eq!(e.armazenamento().aviso_arranque, None, "sem conflito falso");
        assert!(
            std::fs::read_to_string(local.join("assinatura"))
                .unwrap()
                .starts_with("v2\n")
        );
        inserir(&e, s(11), "BB-11-BB").unwrap();
        e.tick(s(14));
        assert_eq!(carros_no_banco(&banco), 2);
    }

    // ---------- I4: só o marcador falhou ----------

    #[cfg(windows)]
    #[test]
    fn marcador_que_nao_sai_regista_assinatura_e_nao_da_conflito() {
        use crate::armazenamento::gravacao::testes::trancar;
        let (_t, raiz, local) = cenario();
        let banco = raiz.join("dados").join("frota.db");
        let (a, _) = Estado::arrancar(&raiz, &local, "PC-A", t0());
        inserir(&a, t0(), "A1").unwrap();
        let pendente = local.join("pendente");
        let tranca = trancar(&pendente);
        a.tick(s(3));
        drop(tranca);
        let est = a.armazenamento();
        // v0.1.1: é uma nota informativa, não um erro de gravação
        assert_eq!(est.erro, None);
        assert!(
            est.nota_gravacao
                .as_deref()
                .unwrap()
                .contains("foram guardados")
        );
        assert!(est.ultima_gravacao.is_some());
        assert_eq!(carros_no_banco(&banco), 1);
        // a gravação seguinte não vê um conflito falso
        inserir(&a, s(10), "A2").unwrap();
        a.tick(s(13));
        let est = a.armazenamento();
        assert!(!est.so_consulta);
        assert_eq!(est.erro, None);
        assert_eq!(est.nota_gravacao, None);
        assert_eq!(carros_no_banco(&banco), 2);
    }

    // ---------- pasta local antiga ----------

    #[test]
    fn pasta_local_por_pasta_de_dados() {
        let (t, raiz, _local) = cenario();
        std::fs::create_dir_all(&raiz).unwrap();
        let base = t.path().join("base");
        let a = pasta_local(&base, &raiz);
        assert_eq!(a.parent(), Some(base.as_path()));
        assert_eq!(a.file_name().unwrap().len(), 12);
        // o mesmo caminho escrito de outra forma dá a mesma pasta
        let outra_forma = std::path::PathBuf::from(raiz.to_string_lossy().to_uppercase());
        assert_eq!(pasta_local(&base, &outra_forma), a);
        assert_ne!(pasta_local(&base, &t.path().join("demo")), a);
    }

    /// Pasta local antiga do PC-A com a assinatura antiga do banco e `matricula`
    /// por gravar no trabalho.
    fn legado_com_pendente(raiz: &Path, legado: &Path, assinatura_certa: bool, matricula: &str) {
        let banco = raiz.join("dados").join("frota.db");
        std::fs::create_dir_all(legado).unwrap();
        std::fs::copy(&banco, legado.join("trabalho.db")).unwrap();
        let c = Connection::open(legado.join("trabalho.db")).unwrap();
        c.execute(
            "INSERT INTO carros (matricula, estado) VALUES (?1, 'ativo')",
            [matricula],
        )
        .unwrap();
        drop(c);
        std::fs::write(legado.join("pendente"), b"").unwrap();
        let ass = if assinatura_certa {
            assinatura_antiga_de(&banco).unwrap()
        } else {
            "1\n2\nff".to_string()
        };
        std::fs::write(legado.join("assinatura"), ass).unwrap();
    }

    #[test]
    fn legado_da_mesma_pasta_e_migrado_e_gravado() {
        let (_t, raiz, local) = cenario();
        let legado = local.join("antigo");
        let novo = local.join("novo");
        let banco = banco_com_um_carro(&raiz, &local.join("x"));
        legado_com_pendente(&raiz, &legado, true, "L1");
        let (e, a) = Estado::arrancar_com_legado(&raiz, &novo, Some(&legado), "PC-A", s(10));
        assert_eq!(a, Arranque::Ok);
        assert_eq!(e.armazenamento().aviso_arranque, None);
        assert!(tem(&banco, "L1"));
        assert!(!legado.join("trabalho.db").exists());
        assert!(!legado.join("pendente").exists());
        assert!(!legado.join("assinatura").exists());
        assert!(!novo.join("pendente").exists());
    }

    /// v0.1.1: sem prova de que é desta pasta, fica neste PC e nunca entra nas
    /// versões partilhadas.
    #[test]
    fn legado_de_origem_incerta_fica_local_e_avisa() {
        let (_t, raiz, local) = cenario();
        let legado = local.join("antigo");
        let banco = banco_com_um_carro(&raiz, &local.join("x"));
        legado_com_pendente(&raiz, &legado, false, "L1");
        std::fs::write(legado.join("trabalho.db-journal"), b"").unwrap();
        let (e, a) =
            Estado::arrancar_com_legado(&raiz, &local.join("novo"), Some(&legado), "PC-A", s(60));
        assert_eq!(a, Arranque::Ok);
        let guardado = legado.join("legado_por_gravar_2026-10-20_1201.db");
        let aviso = e.armazenamento().aviso_arranque.unwrap();
        assert!(aviso.contains(&guardado.display().to_string()), "{aviso}");
        assert!(aviso.contains("não foi possível confirmar"), "{aviso}");
        assert!(tem(&guardado, "L1"));
        assert!(
            legado
                .join("legado_por_gravar_2026-10-20_1201.db-journal")
                .exists()
        );
        assert!(!tem(&banco, "L1"), "não grava por cima sem certeza");
        assert_eq!(
            versao_com(&raiz, "L1"),
            None,
            "nada nas versões partilhadas"
        );
        assert!(!legado.join("pendente").exists());
        assert!(!legado.join("trabalho.db").exists());
        assert!(!legado.join("assinatura").exists());
        // o arranque seguinte não volta a mexer no ficheiro guardado
        drop(e);
        let (e2, _) =
            Estado::arrancar_com_legado(&raiz, &local.join("novo"), Some(&legado), "PC-A", s(120));
        assert_eq!(e2.armazenamento().aviso_arranque, None);
        assert!(tem(&guardado, "L1"));
    }

    /// v0.1.1: comprovadamente desta pasta, mas esta pasta já tem alterações por
    /// gravar: vai para `frota_legado_*` (não podado) em Backup.
    #[test]
    fn legado_comprovado_que_nao_se_pode_migrar_vai_para_frota_legado() {
        let (_t, raiz, local) = cenario();
        let legado = local.join("antigo");
        let novo = local.join("novo");
        let banco = banco_com_um_carro(&raiz, &novo);
        legado_com_pendente(&raiz, &legado, true, "L1");
        std::fs::write(novo.join("pendente"), b"").unwrap();
        let (e, a) = Estado::arrancar_com_legado(&raiz, &novo, Some(&legado), "PC-A", s(60));
        assert_eq!(a, Arranque::Ok);
        let aviso = e.armazenamento().aviso_arranque.unwrap();
        assert!(aviso.contains("frota_legado_2026-10-20_1201.db"), "{aviso}");
        assert_eq!(
            versao_com(&raiz, "L1").as_deref(),
            Some("frota_legado_2026-10-20_1201.db")
        );
        assert!(!tem(&banco, "L1"));
        assert!(!legado.join("pendente").exists());
        let v = e.listar_versoes().unwrap();
        assert!(
            v.iter()
                .find(|x| x.nome == "frota_legado_2026-10-20_1201.db")
                .unwrap()
                .restauravel
        );
    }

    #[test]
    fn legado_sem_pendente_e_so_cache() {
        let (_t, raiz, local) = cenario();
        let legado = local.join("antigo");
        banco_com_um_carro(&raiz, &local.join("x"));
        std::fs::create_dir_all(&legado).unwrap();
        std::fs::write(legado.join("trabalho.db"), b"x").unwrap();
        std::fs::write(legado.join("assinatura"), b"x").unwrap();
        let (e, a) =
            Estado::arrancar_com_legado(&raiz, &local.join("novo"), Some(&legado), "PC-A", s(10));
        assert_eq!(a, Arranque::Ok);
        assert_eq!(e.armazenamento().aviso_arranque, None);
        assert!(!legado.join("trabalho.db").exists());
        assert!(!legado.join("assinatura").exists());
    }
}
