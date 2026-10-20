// Utilitários partilhados pelos ecrãs de carros e motoristas e pela folha Lançar.
// Estados e alertas (multas, cartas, documentos por pagar) vêm do núcleo em Rust já calculados;
// aqui não se reimplementa nenhuma regra.

import { useCallback, useEffect, useState } from 'react'
import type { ErroApi, EstadoCarro, EstadoMulta } from '../api/tipos'

const dois = (n: number) => String(n).padStart(2, '0')

/** Data de hoje (hora local) como "AAAA-MM-DD". */
export function hoje(): string {
  const d = new Date()
  return `${d.getFullYear()}-${dois(d.getMonth() + 1)}-${dois(d.getDate())}`
}

/** Minúsculas, sem hífens nem espaços: "AA-12-BB" e "aa 12" comparam-se bem. */
export function normalizar(texto: string): string {
  return texto.toLowerCase().replace(/[\s-]+/g, '')
}

/** Converte o que foi rejeitado por um comando num `ErroApi`. */
export function paraErroApi(e: unknown): ErroApi {
  if (e && typeof e === 'object' && 'codigo' in e && 'mensagem' in e) {
    const { codigo, mensagem } = e as ErroApi
    return { codigo: String(codigo), mensagem: String(mensagem) }
  }
  return { codigo: 'desconhecido', mensagem: typeof e === 'string' ? e : 'Ocorreu um erro inesperado.' }
}

export type CampoErro =
  | 'matricula'
  | 'carro'
  | 'data'
  | 'dataSaida'
  | 'dataInfracao'
  | 'km'
  | 'valor'
  | 'valorEmpresa'
  | 'oficina'
  | 'pontos'
  | 'ano'
  | 'geral'

/** Mapa `codigo → campo` dos erros de validação do núcleo. */
export function campoDoErro(codigo: string): CampoErro {
  if (codigo.startsWith('matricula_')) return 'matricula'
  if (codigo === 'data_saida_invalida') return 'dataSaida'
  if (codigo === 'falta_data_infracao') return 'dataInfracao'
  if (codigo.startsWith('data_')) return 'data' // data_futura, data_invalida, data_antes_aquisicao
  if (codigo === 'km_abaixo' || codigo === 'km_invalido') return 'km'
  if (codigo === 'valor_invalido' || codigo === 'valor_nao_bate' || codigo === 'mao_obra_propria')
    return 'valor'
  if (codigo === 'coparticipacao_invalida') return 'valorEmpresa'
  if (codigo === 'falta_oficina') return 'oficina'
  if (codigo === 'pontos_invalidos') return 'pontos'
  return 'geral' // referencia_invalida, so_consulta, …
}

export type ErrosCampos = Partial<Record<CampoErro, string>>

export function errosDe(e: unknown): ErrosCampos {
  const { codigo, mensagem } = paraErroApi(e)
  return { [campoDoErro(codigo)]: mensagem }
}

/**
 * Carrega dados de forma assíncrona; `recarregar()` repete o pedido, tal como uma mudança de
 * `sinal` (a `versao` da App, que muda após cada lançamento). Os dados anteriores ficam à vista
 * até chegarem os novos e o componente não remonta.
 */
export function useCarregar<T>(buscar: () => Promise<T>, sinal = 0) {
  const [dados, setDados] = useState<T | null>(null)
  const [erro, setErro] = useState<string | null>(null)
  const [versao, setVersao] = useState(0)

  useEffect(() => {
    let vivo = true
    buscar().then(
      (d) => {
        if (!vivo) return
        setDados(d)
        setErro(null)
      },
      (e) => {
        if (vivo) setErro(paraErroApi(e).mensagem)
      },
    )
    return () => {
      vivo = false
    }
  }, [buscar, versao, sinal])

  const recarregar = useCallback(() => setVersao((v) => v + 1), [])
  return { dados, erro, recarregar }
}

export const ROTULO_CATEGORIA: Record<string, string> = {
  manutencao: 'Manutenção',
  pneus: 'Pneus',
  iuc: 'IUC',
  inspecao: 'Inspeção',
  seguro: 'Seguro',
  multa: 'Multas',
  outro: 'Outros',
}

export const ROTULO_ESTADO: Record<EstadoCarro, string> = {
  ativo: 'Ativo',
  inativo: 'Inativo',
  vendido: 'Vendido',
}

export const ROTULO_ESTADO_MULTA: Record<EstadoMulta, string> = {
  porIdentificarCondutor: 'Por identificar condutor',
  porPagar: 'Por pagar',
  porReembolsar: 'Por reembolsar',
  fechada: 'Fechada',
}
