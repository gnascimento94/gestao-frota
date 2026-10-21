// Espelho dos structs serializados pelo núcleo em Rust (camelCase).
// Dinheiro: inteiros em cêntimos. Datas: "AAAA-MM-DD" (strings).
// Campos `Option` do Rust chegam como `null`; nos inputs podem ser omitidos.

export type ErroApi = {
  codigo: string
  mensagem: string
  /** Campo do input (camelCase) a que o erro se refere, quando o código não chega para o saber. */
  campo?: string
}

// ---------- armazenamento ----------

export type Arranque =
  | { estado: 'ok' }
  | { estado: 'bloqueado'; pc: string }
  | { estado: 'corrompido'; motivo: MotivoCorrompido; mensagem: string }
  | { estado: 'versaoMaisRecente' }
  | { estado: 'semPasta'; mensagem: string }

/** "vazio": o Google Drive ainda não descarregou o ficheiro (esperar e tentar de novo);
 *  "danificado": o ficheiro existe mas falha a verificação de integridade. */
export type MotivoCorrompido = 'vazio' | 'danificado'

export interface EstadoArmazenamento {
  pasta: string
  noGoogleDrive: boolean
  /** ISO local "AAAA-MM-DDTHH:MM:SS". */
  ultimaGravacao: string | null
  erro: string | null
  /** Aviso do arranque (conflito com outro PC); fica até `dispensarAviso()`. */
  avisoArranque: string | null
  soConsulta: boolean
  versaoApp: string
}

/** Versão mais recente publicada no GitHub (`verificarAtualizacao`). */
export interface NovaVersao {
  versao: string
  notas: string
}

export interface Versao {
  nome: string
  /** "AAAA-MM-DD" ou "AAAA-MM-DD HH:MM". */
  data: string
  /** Bytes. */
  tamanho: number
  /** -1 se não foi possível ler. */
  carros: number
  lancamentos: number
}

// ---------- carros ----------

export type EstadoCarro = 'ativo' | 'inativo' | 'vendido'

export interface Carro {
  id: number
  matricula: string
  marca: string | null
  modelo: string | null
  ano: number | null
  cor: string | null
  kmInicial: number
  kmAtual: number
  dataAquisicao: string | null
  estado: EstadoCarro
  dataSaida: string | null
  notas: string | null
  /** [motoristaId, nome] da atribuição aberta. */
  motoristaAtual: [number, string] | null
  /** Cêntimos gastos no ano pedido. */
  gastoAno: number
  /** Cêntimos gastos desde sempre. */
  gastoTotal: number
}

export interface CarroInput {
  id?: number | null
  matricula: string
  marca?: string | null
  modelo?: string | null
  ano?: number | null
  cor?: string | null
  kmInicial: number
  dataAquisicao?: string | null
  estado: EstadoCarro
  dataSaida?: string | null
  notas?: string | null
}

// ---------- motoristas ----------

export interface Motorista {
  id: number
  nome: string
  telefone: string | null
  cartaNumero: string | null
  cartaValidade: string | null
  ativo: boolean
  notas: string | null
  /** Alerta da carta (regra do painel); null se não há alerta ou o motorista está inativo. */
  alertaCarta: Nivel | null
}

export interface MotoristaInput {
  id?: number | null
  nome: string
  telefone?: string | null
  cartaNumero?: string | null
  cartaValidade?: string | null
  ativo: boolean
  notas?: string | null
}

export interface Atribuicao {
  id: number
  carroId: number
  matricula: string
  motoristaId: number
  motoristaNome: string
  de: string
  /** null = atribuição atual. */
  ate: string | null
}

// ---------- lançamentos ----------

export interface ManutencaoInput {
  carroId: number
  data: string
  km?: number | null
  tipoId: number
  executadaPor: 'propria' | 'oficina'
  oficina?: string | null
  valor: number
  valorPecas?: number | null
  valorMaoObra?: number | null
  descricao?: string | null
  /** true depois de o utilizador confirmar um KM abaixo do anterior. */
  confirmado: boolean
}

export type PosicaoPneu = 'FE' | 'FD' | 'TE' | 'TD' | 'suplente' | 'todos'

export interface PneuInput {
  carroId: number
  data: string
  km?: number | null
  posicao: PosicaoPneu
  marca?: string | null
  modelo?: string | null
  motivo: 'troca' | 'rodizio'
  valor: number
  confirmado: boolean
}

export type TipoDocumento = 'IUC' | 'inspecao' | 'seguro' | 'outro'

export interface DocumentoInput {
  carroId: number
  tipo: TipoDocumento
  descricao?: string | null
  valor: number
  vencimento: string
  pagoEm?: string | null
  anual: boolean
}

export interface KmInput {
  carroId: number
  data: string
  km: number
  confirmado: boolean
}

export type TipoLancamento = 'manutencao' | 'pneu' | 'documento' | 'multa' | 'km'

// ---------- multas ----------

export type Infracao =
  | 'velocidade'
  | 'estacionamento'
  | 'telemovel'
  | 'semaforo'
  | 'cinto'
  | 'documentos'
  | 'portagem'
  | 'outra'

export type ResponsavelMulta = 'empresa' | 'colaborador' | 'coparticipado'

export interface MultaInput {
  id?: number | null
  carroId: number
  motoristaId?: number | null
  dataInfracao: string
  dataNotificacao?: string | null
  condutorIdentificadoEm?: string | null
  infracao: Infracao
  pontos: number
  valor: number
  vencimento?: string | null
  pagoEm?: string | null
  responsavel: ResponsavelMulta
  /** Só usado em "coparticipado". */
  valorEmpresa?: number | null
  empresaAdiantou: boolean
  reembolsadoEm?: string | null
  descricao?: string | null
}

export type EstadoMulta = 'porIdentificarCondutor' | 'porPagar' | 'porReembolsar' | 'fechada'

export interface MultaItem {
  id: number
  carroId: number
  matricula: string
  dataInfracao: string
  infracao: string
  pontos: number
  valor: number
  valorEmpresa: number
  valorColaborador: number
  estadoMulta: EstadoMulta
  motoristaId: number | null
  vencimento: string | null
  descricao: string | null
}

export interface ResumoMultasMotorista {
  n: number
  total: number
  empresa: number
  colaborador: number
  porReembolsar: number
  pontos3Anos: number
}

// ---------- histórico ----------

export interface HistoricoItem {
  tipo: TipoLancamento
  id: number
  carroId: number
  matricula: string
  data: string
  km: number | null
  titulo: string
  valor: number | null
  detalhe: string | null
  /** Documento ainda por pagar. */
  porPagar: boolean
  /** Prazo de pagamento (documentos e multas). */
  vencimento: string | null
  /** Multas: condutor identificado. */
  motoristaId: number | null
  /** Só nas multas. */
  estadoMulta: EstadoMulta | null
}

// ---------- tipos e planos ----------

export interface TipoManutencao {
  id: number
  nome: string
  ativo: boolean
}

export interface Plano {
  id: number
  tipoId: number
  tipoNome: string
  cadaKm: number | null
  cadaMeses: number | null
  todosCarros: boolean
  /** Vazio quando `todosCarros`. */
  carros: number[]
}

export interface PlanoInput {
  id?: number | null
  tipoId: number
  cadaKm?: number | null
  cadaMeses?: number | null
  todosCarros: boolean
  carros: number[]
}

export interface Limiares {
  km: number
  dias: number
}

// ---------- painel e relatórios ----------

export type Nivel = 'vencido' | 'proximo'

export type TipoAlerta = 'manutencao' | 'documento' | 'carta' | 'identificarCondutor' | 'multa'

export interface Alerta {
  nivel: Nivel
  tipo: TipoAlerta
  carroId: number | null
  motoristaId: number | null
  /** Plano, documento, motorista ou multa, conforme o tipo. */
  refId: number
  titulo: string
  /** Negativo quando vencido. */
  faltaKm: number | null
  /** Negativo quando vencido. */
  faltaDias: number | null
  /** Manutenção: dimensão que decide o nível (só essa `falta*` vem preenchida). */
  motivo: Motivo | null
  /** "AAAA-MM-DD": vencimento, prazo de identificação, validade da carta ou próxima data. */
  data: string | null
  /** Alertas de carro. */
  matricula: string | null
  /** Alerta da carta. */
  nome: string | null
}

export type Motivo = 'km' | 'dias'

export interface Painel {
  /** Ano e mês (1–12) usados pelo núcleo para "este mês" e "no ano". */
  ano: number
  mes: number
  gastoMes: number
  gastoAno: number
  /** "Por pagar (hoje)": vencidos + a vencer nos próximos `porPagarDias` dias (carros ativos). */
  porPagar: number
  porPagarVencido: number
  porPagarProximo: number
  porPagarDias: number
  carrosAtivos: number
  alertas: Alerta[]
  recentes: HistoricoItem[]
}

export type Categoria =
  | 'manutencao'
  | 'pneus'
  | 'iuc'
  | 'inspecao'
  | 'seguro'
  | 'multa'
  | 'outro'

export type PorCategoria = Partial<Record<Categoria, number>>

export interface Resumo {
  total: number
  porCategoria: PorCategoria
  /** Chave "AAAA-MM". */
  porMes: Record<string, PorCategoria>
  /** Total de cada mês (chave "AAAA-MM"). */
  totalPorMes: Record<string, number>
  /** [carroId, total]. */
  porCarro: [number, number][]
  /** "Por pagar (hoje)": dívida atual (vencidos + próximos `porPagarDias` dias), não do período. */
  porPagar: number
  porPagarVencido: number
  porPagarProximo: number
  porPagarDias: number
}

export interface FiltroRelatorio {
  de: string
  ate: string
  carroIds?: number[] | null
  motoristaId?: number | null
}

export interface RelManutencao {
  interna: number
  oficina: number
  pecas: number
  maoObra: number
  naoDiscriminado: number
  /** [oficina, total]. */
  porOficina: [string, number][]
}

export interface RelMultas {
  n: number
  total: number
  empresa: number
  colaboradores: number
  porReembolsar: number
  /** [infração, n.º, total]. */
  porInfracao: [string, number, number][]
  /** [motoristaId, nome, n.º, pontos]. */
  porMotorista: [number | null, string, number, number][]
}

export interface Relatorio {
  resumo: Resumo
  /** [motoristaId, nome, total]. */
  porMotorista: [number | null, string, number][]
  /** [tipo, total]. */
  porTipoManutencao: [string, number][]
  manutencao: RelManutencao
  multas: RelMultas
}
