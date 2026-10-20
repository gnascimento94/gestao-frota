// Uma função tipada por comando Tauri (src-tauri/src/comandos.rs).
// Os nomes dos argumentos estão em camelCase (o Tauri converte para snake_case).
// Em caso de erro a Promise é rejeitada com um `ErroApi`.

import { invoke } from '@tauri-apps/api/core'
import type {
  Arranque,
  Atribuicao,
  Carro,
  CarroInput,
  DocumentoInput,
  EstadoArmazenamento,
  FiltroRelatorio,
  HistoricoItem,
  KmInput,
  Limiares,
  ManutencaoInput,
  Motorista,
  MotoristaInput,
  MultaInput,
  MultaItem,
  Painel,
  Plano,
  PlanoInput,
  PneuInput,
  Relatorio,
  ResumoMultasMotorista,
  TipoLancamento,
  TipoManutencao,
  Versao,
} from './tipos'

export type { ErroApi } from './tipos'

// ---------- armazenamento ----------

/** Estado do arranque; depois de `bloqueado`/`semPasta`, volta a tentar arrancar. */
export const arranque = () => invoke<Arranque>('arranque')

export const abrirConsulta = () => invoke<void>('abrir_consulta')

/** Fecha o aviso do arranque (`EstadoArmazenamento.avisoArranque`). */
export const dispensarAviso = () => invoke<void>('dispensar_aviso')

export const estadoArmazenamento = () => invoke<EstadoArmazenamento>('estado_armazenamento')

export const fazerCopia = () => invoke<void>('fazer_copia')

export const listarVersoes = () => invoke<Versao[]>('listar_versoes')

export const restaurarVersao = (nome: string) => invoke<void>('restaurar_versao', { nome })

// ---------- carros ----------

export const carrosListar = (soAtivos: boolean, ano: number) =>
  invoke<Carro[]>('carros_listar', { soAtivos, ano })

export const carroObter = (id: number, ano: number) => invoke<Carro>('carro_obter', { id, ano })

/** Devolve o id do carro. */
export const carroGuardar = (input: CarroInput) => invoke<number>('carro_guardar', { input })

export const carroApagar = (id: number) => invoke<void>('carro_apagar', { id })

// ---------- motoristas ----------

export const motoristasListar = (soAtivos: boolean) =>
  invoke<Motorista[]>('motoristas_listar', { soAtivos })

export const motoristaGuardar = (input: MotoristaInput) =>
  invoke<number>('motorista_guardar', { input })

export const motoristaApagar = (id: number) => invoke<void>('motorista_apagar', { id })

/** `motoristaId: null` deixa o carro sem motorista. */
export const atribuir = (carroId: number, motoristaId: number | null, data: string) =>
  invoke<void>('atribuir', { carroId, motoristaId, data })

/** Atribuições (mais recentes primeiro), filtradas por carro e/ou motorista. */
export const atribuicoesListar = (carroId?: number | null, motoristaId?: number | null) =>
  invoke<Atribuicao[]>('atribuicoes_listar', {
    carroId: carroId ?? null,
    motoristaId: motoristaId ?? null,
  })

// ---------- lançamentos ----------

export const lancarManutencao = (input: ManutencaoInput) =>
  invoke<number>('lancar_manutencao', { input })

export const lancarPneu = (input: PneuInput) => invoke<number>('lancar_pneu', { input })

export const lancarDocumento = (input: DocumentoInput) =>
  invoke<number>('lancar_documento', { input })

export const oficinasUsadas = () => invoke<string[]>('oficinas_usadas')

export const atualizarKm = (input: KmInput) => invoke<number>('atualizar_km', { input })

/** Devolve o id do documento do ano seguinte, se foi criado. */
export const pagarDocumento = (id: number, pagoEm: string) =>
  invoke<number | null>('pagar_documento', { id, pagoEm })

export const apagarLancamento = (tipo: TipoLancamento, id: number) =>
  invoke<void>('apagar_lancamento', { tipo, id })

// ---------- multas ----------

export const guardarMulta = (input: MultaInput) => invoke<number>('guardar_multa', { input })

export const condutorSugerido = (carroId: number, data: string) =>
  invoke<number | null>('condutor_sugerido', { carroId, data })

export const marcarReembolsado = (id: number, data: string) =>
  invoke<void>('marcar_reembolsado', { id, data })

export const identificarCondutor = (id: number, motoristaId: number, data: string) =>
  invoke<void>('identificar_condutor', { id, motoristaId, data })

/** Multas (mais recentes primeiro), filtradas por carro e/ou motorista. */
export const multasListar = (carroId?: number | null, motoristaId?: number | null) =>
  invoke<MultaItem[]>('multas_listar', {
    carroId: carroId ?? null,
    motoristaId: motoristaId ?? null,
  })

export const multasMotorista = (motoristaId: number) =>
  invoke<ResumoMultasMotorista>('multas_motorista', { motoristaId })

// ---------- histórico ----------

/** `carroId: null` = todos os carros. */
export const historico = (carroId: number | null, limite: number) =>
  invoke<HistoricoItem[]>('historico', { carroId, limite })

// ---------- tipos e planos ----------

export const tiposListar = () => invoke<TipoManutencao[]>('tipos_listar')

/** `id: null` cria; senão renomeia. */
export const tipoGuardar = (id: number | null, nome: string) =>
  invoke<number>('tipo_guardar', { id, nome })

/** Ativa ou desativa um tipo de manutenção. */
export const tipoAtivo = (id: number, ativo: boolean) => invoke<void>('tipo_ativo', { id, ativo })

export const planosListar =() => invoke<Plano[]>('planos_listar')

export const planoGuardar = (input: PlanoInput) => invoke<number>('plano_guardar', { input })

export const planoApagar = (id: number) => invoke<void>('plano_apagar', { id })

// ---------- configuração ----------

export const limiaresObter = () => invoke<Limiares>('limiares_obter')

export const limiaresGuardar = (input: Limiares) => invoke<void>('limiares_guardar', { input })

// ---------- consultas ----------

export const painel = () => invoke<Painel>('painel')

export const relatorio = (filtro: FiltroRelatorio) => invoke<Relatorio>('relatorio', { filtro })
