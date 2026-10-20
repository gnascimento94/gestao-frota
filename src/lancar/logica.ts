// Lógica partilhada pelos formulários da folha Lançar: leitura de valores escritos,
// validação ao escrever, envio ao núcleo e confirmação de KM abaixo do anterior.
// As regras de negócio ficam no núcleo; aqui só se impede o que é obviamente inválido.

import { useCallback, useRef, useState, type ReactNode } from 'react'
import type { Carro } from '../api/tipos'
import { ROTULO_ESTADO, errosDe, hoje, paraErroApi } from '../ecras/comum'
import { parseEuros } from '../util/formato'

/** Erros por campo (chaves do núcleo — ver `campoDoErro` — e campos locais do formulário). */
export type Erros = Partial<Record<string, string>>

export const MSG_EUROS = 'Escreva um valor em euros, por exemplo 120,00.'
export const MSG_KM = 'Indique os quilómetros como número inteiro.'
export const MSG_CARRO = 'Escolha o carro.'
export const MSG_FUTURO = 'A data não pode ser no futuro.'

/** Props comuns a todos os formulários de lançamento. */
export interface PropsForm {
  carro: Carro | null
  /** Seletor de carro (topo do formulário), com o erro do carro, se houver. */
  seletor(erro?: string): ReactNode
  /** Foca o primeiro campo do formulário ao abrir (o carro já vem escolhido). */
  focoNoFormulario: boolean
  onLancado(): void
  onCancelar(): void
}

export const vazio = (t: string) => t.trim() === ''

/** "AA-12-BB · Renault Clio" (e o estado, se não estiver ativo). */
export function rotuloCarro(c: Carro): string {
  const nome = [c.marca, c.modelo].filter(Boolean).join(' ')
  const estado = c.estado === 'ativo' ? '' : ` (${ROTULO_ESTADO[c.estado].toLowerCase()})`
  return `${c.matricula}${nome ? ` · ${nome}` : ''}${estado}`
}

/** KM escrito pelo utilizador (aceita espaços e pontos de milhares); `null` se inválido ou vazio. */
export function lerKm(texto: string): number | null {
  const t = texto.replace(/[\s.]/g, '') // \s inclui o espaço fino e o inseparável
  if (!/^\d+$/.test(t)) return null
  const n = Number(t)
  return Number.isSafeInteger(n) ? n : null
}

/** Erro de um campo em euros: texto inválido logo ao escrever; vazio obrigatório só ao submeter. */
export function erroEuros(texto: string, obrigatorio: boolean, tentou: boolean): string | undefined {
  if (vazio(texto)) return obrigatorio && tentou ? MSG_EUROS : undefined
  return parseEuros(texto) === null ? MSG_EUROS : undefined
}

/** Erro de um campo de KM (inteiro ≥ 0). */
export function erroKm(texto: string, obrigatorio: boolean, tentou: boolean): string | undefined {
  if (vazio(texto)) return obrigatorio && tentou ? MSG_KM : undefined
  return lerKm(texto) === null ? MSG_KM : undefined
}

/** Data obrigatória (só se queixa ao submeter). */
export function erroData(texto: string, tentou: boolean, mensagem = 'Indique a data.'): string | undefined {
  return tentou && vazio(texto) ? mensagem : undefined
}

/** Data que não pode ser futura (as datas "AAAA-MM-DD" comparam-se como texto). */
export function erroFuturo(texto: string): string | undefined {
  return !vazio(texto) && texto > hoje() ? MSG_FUTURO : undefined
}

/** Data opcional mal formada (não "AAAA-MM-DD"). */
export function erroFormatoData(texto: string): string | undefined {
  return !vazio(texto) && !/^\d{4}-\d{2}-\d{2}$/.test(texto.trim()) ? 'Data inválida.' : undefined
}

/**
 * Campo a que se refere um erro de data do núcleo: o primeiro cuja data aparece na mensagem
 * (por exemplo "Data inválida: 2026-13-01."); `undefined` se nenhum aparecer.
 */
export function campoDaData(mensagem: string, campos: Record<string, string>): string | undefined {
  return Object.entries(campos).find(([, valor]) => !vazio(valor) && mensagem.includes(valor.trim()))?.[0]
}

export const temErros =(e: Erros) => Object.values(e).some(Boolean)

/** `''` → `null`, senão o texto sem espaços nas pontas. */
export const textoOuNulo = (t: string) => t.trim() || null

/**
 * Envio de um lançamento ao núcleo. Mostra os erros do núcleo nos campos; em `km_abaixo`
 * guarda o pedido para `confirmar()` o reenviar com `confirmado: true`.
 */
export function useLancamento<I extends object>(
  enviar: (input: I) => Promise<unknown>,
  onLancado: () => void,
) {
  const [erros, setErros] = useState<Erros>({})
  const [pendente, setPendente] = useState<I | null>(null)
  const [aEnviar, setAEnviar] = useState(false)
  const [tentou, setTentou] = useState(false)
  const emCurso = useRef(false)

  const submeter = useCallback(
    async (input: I) => {
      if (emCurso.current) return
      emCurso.current = true
      setAEnviar(true)
      setErros({})
      setPendente(null)
      try {
        await enviar(input)
        onLancado()
      } catch (e) {
        const { codigo } = paraErroApi(e)
        const mapa: Erros = errosDe(e)
        // No Lançar, a matrícula escolhe-se no seletor de carro.
        if (mapa.matricula) {
          mapa.carro = mapa.matricula
          delete mapa.matricula
        }
        if (codigo === 'km_abaixo') setPendente(input)
        setErros(mapa)
      } finally {
        emCurso.current = false
        setAEnviar(false)
      }
    },
    [enviar, onLancado],
  )

  /** Valida (com os obrigatórios) e, se estiver tudo bem, envia o que `construir()` devolver. */
  const tentar = useCallback(
    (validar: (tentou: boolean) => Erros, construir: () => I) => {
      setTentou(true)
      if (temErros(validar(true))) return
      void submeter(construir())
    },
    [submeter],
  )

  /** Reenvia o pedido que recebeu `km_abaixo`, agora confirmado. */
  const confirmar = useCallback(() => {
    if (pendente) void submeter({ ...pendente, confirmado: true })
  }, [pendente, submeter])

  /** Esquece os erros do núcleo e a confirmação pendente (chamar quando o formulário muda). */
  const limpar = useCallback(() => {
    setErros({})
    setPendente(null)
  }, [])

  return { erros, aEnviar, tentou, pedeConfirmacao: pendente !== null, tentar, confirmar, limpar }
}

/** KM sugerido = KM atual do carro; volta a sugerir quando se escolhe outro carro. */
export function useKmSugerido(carro: Carro | null) {
  const id = carro?.id ?? null
  const [km, setKm] = useState(carro ? String(carro.kmAtual) : '')
  const [idSugerido, setIdSugerido] = useState(id)
  if (id !== idSugerido) {
    setIdSugerido(id)
    if (carro) setKm(String(carro.kmAtual))
  }
  return [km, setKm] as const
}
