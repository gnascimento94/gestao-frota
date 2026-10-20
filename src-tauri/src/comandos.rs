//! Comandos Tauri. Cada escrita passa por `Estado::mutar` (recusada em modo
//! consulta; marca a alteraÃ§Ã£o para gravar). SÃ³ aqui se lÃª o relÃ³gio.
//!
//! Os comandos correm fora da thread principal (`async`) para nÃ£o bloquear a janela.

use crate::armazenamento::versoes::Versao;
use crate::erro::Res;
use crate::estado::{Arranque, Estado, EstadoArmazenamento};
use crate::regras::alertas::Limiares;
use crate::repositorio::carros::{self, Carro, CarroInput};
use crate::repositorio::consultas::{self, FiltroRelatorio, Painel, Relatorio};
use crate::repositorio::historico::{self, HistoricoItem};
use crate::repositorio::lancamentos::{
    self, DocumentoInput, KmInput, ManutencaoInput, PneuInput,
};
use crate::repositorio::motoristas::{self, Motorista, MotoristaInput};
use crate::repositorio::multas::{self, MultaInput, ResumoMultasMotorista};
use crate::repositorio::planos::{self, Plano, PlanoInput, TipoManutencao};
use crate::repositorio::{config, data as ler_data};
use chrono::{Local, NaiveDate, NaiveDateTime};
use tauri::State;

fn hoje() -> NaiveDate {
    Local::now().date_naive()
}

fn agora() -> NaiveDateTime {
    Local::now().naive_local()
}

type E<'a> = State<'a, Estado>;

// ---------- armazenamento ----------

#[tauri::command(async)]
pub fn arranque(e: E<'_>) -> Res<Arranque> {
    Ok(e.arranque())
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
    e.ler(|c| motoristas::listar_motoristas(c, so_ativos))
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
    e.mutar(agora(), |c| motoristas::atribuir(c, carro_id, motorista_id, dia))
}

// ---------- lanÃ§amentos ----------

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
    e.mutar(agora(), |c| lancamentos::pagar_documento(c, id, &pago_em, h))
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
    e.mutar(agora(), |c| multas::identificar_condutor(c, id, motorista_id, dia, h))
}

#[tauri::command(async)]
pub fn multas_motorista(e: E<'_>, motorista_id: i64) -> Res<ResumoMultasMotorista> {
    let h = hoje();
    e.ler(|c| multas::multas_motorista(c, motorista_id, h))
}

// ---------- histÃ³rico ----------

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

/// `id: None` cria; senÃ£o renomeia.
#[tauri::command(async)]
pub fn tipo_guardar(e: E<'_>, id: Option<i64>, nome: String) -> Res<i64> {
    e.mutar(agora(), |c| planos::guardar_tipo(c, id, &nome))
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

// ---------- configuraÃ§Ã£o ----------

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
    e.ler(|c| consultas::relatorio(c, &filtro))
}
