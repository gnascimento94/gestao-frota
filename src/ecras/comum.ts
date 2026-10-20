// Utilitários partilhados pelos ecrãs de carros e motoristas.

import { useCallback, useEffect, useState } from 'react'
import type { ErroApi, EstadoCarro } from '../api/tipos'

const dois = (n: number) => String(n).padStart(2, '0')

/** Data de hoje (hora local) como "AAAA-MM-DD". */
export function hoje(): string {
  const d = new Date()
  return `${d.getFullYear()}-${dois(d.getMonth() + 1)}-${dois(d.getDate())}`
}

/** Data daqui a `dias` dias como "AAAA-MM-DD". */
export function emDias(dias: number): string {
  const d = new Date()
  d.setDate(d.getDate() + dias)
  return `${d.getFullYear()}-${dois(d.getMonth() + 1)}-${dois(d.getDate())}`
}

export const anoAtual = () => new Date().getFullYear()

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

export type CampoErro = 'matricula' | 'data' | 'km' | 'valor' | 'geral'

/** Mapa `codigo → campo` dos erros de validação do núcleo. */
export function campoDoErro(codigo: string): CampoErro {
  if (codigo.startsWith('matricula_')) return 'matricula'
  if (codigo.startsWith('data_')) return 'data'
  if (codigo === 'km_abaixo') return 'km'
  if (codigo === 'valor_invalido') return 'valor'
  return 'geral' // referencia_invalida, so_consulta, …
}

export type ErrosCampos = Partial<Record<CampoErro, string>>

export function errosDe(e: unknown): ErrosCampos {
  const { codigo, mensagem } = paraErroApi(e)
  return { [campoDoErro(codigo)]: mensagem }
}

/** Carrega dados de forma assíncrona; `recarregar()` repete o pedido. */
export function useCarregar<T>(buscar: () => Promise<T>) {
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
  }, [buscar, versao])

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

/** O núcleo escreve o estado da multa no início do detalhe do histórico. */
const ESTADOS_MULTA = [
  'por identificar condutor',
  'por pagar',
  'por reembolsar',
  'fechada',
] as const
export type EstadoMulta = (typeof ESTADOS_MULTA)[number]

export function estadoDaMulta(detalhe: string | null): { estado: EstadoMulta | null; resto: string | null } {
  if (!detalhe) return { estado: null, resto: null }
  for (const estado of ESTADOS_MULTA) {
    if (detalhe === estado) return { estado, resto: null }
    if (detalhe.startsWith(`${estado} · `)) return { estado, resto: detalhe.slice(estado.length + 3) }
  }
  return { estado: null, resto: detalhe }
}


/** Alerta visual da validade da carta: vencida, ou a vencer nos próximos 60 dias. */
export function estadoCarta(validade: string | null): 'vencido' | 'proximo' | null {
  if (!validade) return null
  if (validade < hoje()) return 'vencido'
  if (validade <= emDias(60)) return 'proximo'
  return null
}

