//! Comandos Tauri. Cada escrita passa por `Estado::mutar` (recusada em modo
//! consulta; marca a alteração para gravar). Só aqui se lê o relógio.
//!
//! Os comandos correm fora da thread principal (`async`) para não bloquear a janela.

use crate::armazenamento::versoes::Versao;
use crate::atualizacao;
use crate::erro::{Erro, Res};
use crate::estado::{Arranque, Estado, EstadoArmazenamento};
use crate::regras::alertas::Limiares;
use crate::repositorio::carros::{self, Carro, CarroInput};
use crate::repositorio::consultas::{self, FiltroRelatorio, Painel, Relatorio};
use crate::repositorio::historico::{self, HistoricoItem};
use crate::repositorio::lancamentos::{self, DocumentoInput, KmInput, ManutencaoInput, PneuInput};
use crate::repositorio::motoristas::{
    self, Atribuicao, FiltroAtribuicoes, Motorista, MotoristaInput,
};
use crate::repositorio::multas::{self, MultaInput, ResumoMultasMotorista};
use crate::repositorio::planos::{self, Plano, PlanoInput, TipoManutencao};
use crate::repositorio::{config, data as ler_data};
use chrono::{Local, NaiveDate, NaiveDateTime};
use serde::Serialize;
use tauri::State;

fn hoje() -> NaiveDate {
    Local::now().date_naive()
}

fn agora() -> NaiveDateTime {
    Local::now().naive_local()
}

type E<'a> = State<'a, Estado>;

// ---------- armazenamento ----------

/// Estado do arranque. Depois de `bloqueado`/`semPasta` volta a tentar ("Tentar de novo").
#[tauri::command(async)]
pub fn arranque(e: E<'_>) -> Res<Arranque> {
    Ok(e.tentar_arranque(agora()))
}

/// Fecha o aviso do arranque (`EstadoArmazenamento::aviso_arranque`).
#[tauri::command(async)]
pub fn dispensar_aviso(e: E<'_>) -> Res<()> {
    e.dispensar_aviso();
    Ok(())
}

#[tauri::command(async)]
pub fn abrir_consulta(e: E<'_>) -> Res<()> {
    e.abrir_consulta()
}

#[tauri::command(async)]
pub fn estado_armazenamento(e: E<'_>) -> Res<EstadoArmazenamento> {
    Ok(e.armazenamento())
}

#[tauri::command(async)]
pub fn fazer_copia(e: E<'_>) -> Res<()> {
    e.fazer_copia(agora())
}

#[tauri::command(async)]
pub fn listar_versoes(e: E<'_>) -> Res<Vec<Versao>> {
    e.listar_versoes()
}

#[tauri::command(async)]
pub fn restaurar_versao(e: E<'_>, nome: String) -> Res<()> {
    e.restaurar_versao(&nome, agora())
}

// ---------- atualização ----------

#[derive(Debug, Clone, Serialize)]
pub struct NovaVersao {
    pub versao: String,
    pub notas: String,
}

/// Há versão mais recente no GitHub? `None` em desenvolvimento, em modo
/// consulta, sem internet (timeout 5 s) ou em qualquer erro.
#[tauri::command(async)]
pub fn verificar_atualizacao(e: E<'_>) -> Option<NovaVersao> {
    if cfg!(debug_assertions) || e.so_consulta() {
        return None;
    }
    let l = atualizacao::verificar(env!("CARGO_PKG_VERSION"))?;
    log::info!("atualização disponível: {}", l.versao);
    Some(NovaVersao {
        versao: l.versao,
        notas: l.notas,
    })
}

/// Descarrega e valida `Frota.new.exe`, grava os dados, faz a cópia
/// `frota_antes-da-vX.Y.Z.db`, troca os executáveis e reinicia. Qualquer falha
/// apaga `Frota.new.exe` e devolve o erro; a app continua na versão atual.
///
/// Tem de correr fora da thread principal (`async`): assim `restart` pede a
/// saída ao ciclo de eventos, que corre `ExitRequested`/`Exit` (o `fechar` em
/// `lib.rs` grava e liberta o `frota.lock`; o plugin single-instance liberta o
/// seu mutex) antes de lançar o novo processo — que não é engolido como
/// "segunda instância". `current_exe()` devolve o caminho com que o processo
/// arrancou (`Frota.exe`), que já é a versão nova.
#[tauri::command(async)]
pub fn aplicar_atualizacao(app: tauri::AppHandle, e: E<'_>) -> Res<()> {
    if cfg!(debug_assertions) || e.so_consulta() {
        return Err(Erro {
            campo: None,
            codigo: "atualizacao_indisponivel".into(),
            mensagem: "A atualização automática não está disponível neste modo.".into(),
        });
    }
    let exe = std::env::current_exe()?;
    let (l, novo) = atualizacao::preparar(&exe, env!("CARGO_PKG_VERSION"))?;
    // Grava, faz a cópia antes da versão e troca os executáveis; em falha apaga
    // `Frota.new.exe` (salvo troca a meio, em que a mensagem diz onde estão).
    if let Err(err) = atualizacao::instalar(&exe, &novo, || {
        e.gravar_antes_de_atualizar(&l.versao, agora())
    }) {
        log::warn!("atualização abortada: {}", err.mensagem);
        return Err(err);
    }
    log::info!("atualizado para {}; a reiniciar", l.versao);
    app.restart()
}

// ---------- carros ----------

#[tauri::command(async)]
pub fn carros_listar(e: E<'_>, so_ativos: bool, ano: i32) -> Res<Vec<Carro>> {
    e.ler(|c| carros::listar_carros(c, so_ativos, ano))
}

#[tauri::command(async)]
pub fn carro_obter(e: E<'_>, id: i64, ano: i32) -> Res<Carro> {
    e.ler(|c| carros::obter_carro(c, id, ano))
}

#[tauri::command(async)]
pub fn carro_guardar(e: E<'_>, input: CarroInput) -> Res<i64> {
    e.mutar(agora(), |c| carros::guardar_carro(c, &input))
}

#[tauri::command(async)]
pub fn carro_apagar(e: E<'_>, id: i64) -> Res<()> {
    e.mutar(agora(), |c| carros::apagar_carro(c, id))
}

// ---------- motoristas ----------

#[tauri::command(async)]
pub fn motoristas_listar(e: E<'_>, so_ativos: bool) -> Res<Vec<Motorista>> {
    let h = hoje();
    e.ler(|c| motoristas::listar_motoristas(c, so_ativos, h))
}

#[tauri::command(async)]
pub fn motorista_guardar(e: E<'_>, input: MotoristaInput) -> Res<i64> {
    e.mutar(agora(), |c| motoristas::guardar_motorista(c, &input))
}

#[tauri::command(async)]
pub fn motorista_apagar(e: E<'_>, id: i64) -> Res<()> {
    e.mutar(agora(), |c| motoristas::apagar_motorista(c, id))
}

/// `motorista_id: None` = deixar o carro sem motorista.
#[tauri::command(async)]
pub fn atribuir(e: E<'_>, carro_id: i64, motorista_id: Option<i64>, data: String) -> Res<()> {
    let dia = ler_data(&data)?;
    e.mutar(agora(), |c| {
        motoristas::atribuir(c, carro_id, motorista_id, dia)
    })
}

/// Atribuições (mais recentes primeiro), filtradas por carro e/ou motorista.
#[tauri::command(async)]
pub fn atribuicoes_listar(
    e: E<'_>,
    carro_id: Option<i64>,
    motorista_id: Option<i64>,
) -> Res<Vec<Atribuicao>> {
    let f = FiltroAtribuicoes {
        carro_id,
        motorista_id,
    };
    e.ler(|c| motoristas::atribuicoes_de(c, &f))
}

// ---------- lançamentos ----------

#[tauri::command(async)]
pub fn lancar_manutencao(e: E<'_>, input: ManutencaoInput) -> Res<i64> {
    let h = hoje();
    e.mutar(agora(), |c| lancamentos::lancar_manutencao(c, &input, h))
}

#[tauri::command(async)]
pub fn lancar_pneu(e: E<'_>, input: PneuInput) -> Res<i64> {
    let h = hoje();
    e.mutar(agora(), |c| lancamentos::lancar_pneu(c, &input, h))
}

#[tauri::command(async)]
pub fn lancar_documento(e: E<'_>, input: DocumentoInput) -> Res<i64> {
    let h = hoje();
    e.mutar(agora(), |c| lancamentos::lancar_documento(c, &input, h))
}

#[tauri::command(async)]
pub fn oficinas_usadas(e: E<'_>) -> Res<Vec<String>> {
    e.ler(lancamentos::oficinas_usadas)
}

#[tauri::command(async)]
pub fn atualizar_km(e: E<'_>, input: KmInput) -> Res<i64> {
    let h = hoje();
    e.mutar(agora(), |c| lancamentos::atualizar_km(c, &input, h))
}

/// Devolve o id do documento do ano seguinte, se foi criado.
#[tauri::command(async)]
pub fn pagar_documento(e: E<'_>, id: i64, pago_em: String) -> Res<Option<i64>> {
    let h = hoje();
    e.mutar(agora(), |c| {
        lancamentos::pagar_documento(c, id, &pago_em, h)
    })
}

/// `tipo`: manutencao | pneu | documento | multa | km.
#[tauri::command(async)]
pub fn apagar_lancamento(e: E<'_>, tipo: String, id: i64) -> Res<()> {
    e.mutar(agora(), |c| lancamentos::apagar_lancamento(c, &tipo, id))
}

// ---------- multas ----------

#[tauri::command(async)]
pub fn guardar_multa(e: E<'_>, input: MultaInput) -> Res<i64> {
    let h = hoje();
    e.mutar(agora(), |c| multas::guardar_multa(c, &input, h))
}

#[tauri::command(async)]
pub fn condutor_sugerido(e: E<'_>, carro_id: i64, data: String) -> Res<Option<i64>> {
    let dia = ler_data(&data)?;
    e.ler(|c| multas::condutor_sugerido(c, carro_id, dia))
}

#[tauri::command(async)]
pub fn marcar_reembolsado(e: E<'_>, id: i64, data: String) -> Res<()> {
    let dia = ler_data(&data)?;
    let h = hoje();
    e.mutar(agora(), |c| multas::marcar_reembolsado(c, id, dia, h))
}

#[tauri::command(async)]
pub fn identificar_condutor(e: E<'_>, id: i64, motorista_id: i64, data: String) -> Res<()> {
    let dia = ler_data(&data)?;
    let h = hoje();
    e.mutar(agora(), |c| {
        multas::identificar_condutor(c, id, motorista_id, dia, h)
    })
}

#[tauri::command(async)]
pub fn multas_motorista(e: E<'_>, motorista_id: i64) -> Res<ResumoMultasMotorista> {
    let h = hoje();
    e.ler(|c| multas::multas_motorista(c, motorista_id, h))
}

/// Multas (mais recentes primeiro), filtradas por carro e/ou motorista.
#[tauri::command(async)]
pub fn multas_listar(
    e: E<'_>,
    carro_id: Option<i64>,
    motorista_id: Option<i64>,
) -> Res<Vec<multas::MultaItem>> {
    e.ler(|c| multas::multas_listar(c, carro_id, motorista_id))
}

// ---------- histórico ----------

/// `carro_id: None` = todos os carros.
#[tauri::command(async)]
pub fn historico(e: E<'_>, carro_id: Option<i64>, limite: i64) -> Res<Vec<HistoricoItem>> {
    e.ler(|c| historico::historico(c, carro_id, limite))
}

// ---------- tipos e planos ----------

#[tauri::command(async)]
pub fn tipos_listar(e: E<'_>) -> Res<Vec<TipoManutencao>> {
    e.ler(planos::listar_tipos)
}

/// `id: None` cria; senão renomeia.
#[tauri::command(async)]
pub fn tipo_guardar(e: E<'_>, id: Option<i64>, nome: String) -> Res<i64> {
    e.mutar(agora(), |c| planos::guardar_tipo(c, id, &nome))
}

#[tauri::command(async)]
pub fn tipo_ativo(e: E<'_>, id: i64, ativo: bool) -> Res<()> {
    e.mutar(agora(), |c| planos::definir_tipo_ativo(c, id, ativo))
}

#[tauri::command(async)]
pub fn planos_listar(e: E<'_>) -> Res<Vec<Plano>> {
    e.ler(planos::listar_planos)
}

#[tauri::command(async)]
pub fn plano_guardar(e: E<'_>, input: PlanoInput) -> Res<i64> {
    e.mutar(agora(), |c| planos::guardar_plano(c, &input))
}

#[tauri::command(async)]
pub fn plano_apagar(e: E<'_>, id: i64) -> Res<()> {
    e.mutar(agora(), |c| planos::apagar_plano(c, id))
}

// ---------- configuração ----------

#[tauri::command(async)]
pub fn limiares_obter(e: E<'_>) -> Res<Limiares> {
    e.ler(config::obter_limiares)
}

#[tauri::command(async)]
pub fn limiares_guardar(e: E<'_>, input: Limiares) -> Res<()> {
    e.mutar(agora(), |c| config::guardar_limiares(c, &input))
}

// ---------- consultas ----------

#[tauri::command(async)]
pub fn painel(e: E<'_>) -> Res<Painel> {
    let h = hoje();
    e.ler(|c| consultas::painel(c, h))
}

#[tauri::command(async)]
pub fn relatorio(e: E<'_>, filtro: FiltroRelatorio) -> Res<Relatorio> {
    let h = hoje();
    e.ler(|c| consultas::relatorio(c, &filtro, h))
}
