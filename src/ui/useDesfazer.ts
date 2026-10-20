import { createContext, useContext } from 'react'

/** Tempo durante o qual um apagamento pode ser desfeito. */
export const ESPERA_DESFAZER_MS = 8000
/** Tempo máximo de espera pelas ações pendentes quando a janela fecha. */
export const LIMITE_FECHO_MS = 4000

export interface AgendarDesfazer {
  /** Agenda `acao` para daqui a 8 s; até lá o utilizador pode desfazer. */
  agendar(rotulo: string, acao: () => Promise<void>): void
}

/** O que o toast mostra: o rótulo (uma ação) ou "Apagados (N)". */
export interface ResumoPendentes {
  texto: string
  quantas: number
}

export interface ValorDesfazer extends AgendarDesfazer {
  resumo: ResumoPendentes | null
  /** Cancela todas as ações pendentes. */
  desfazerTodas(): void
}

export const ContextoDesfazer = createContext<ValorDesfazer | null>(null)

export function useDesfazer(): AgendarDesfazer {
  const c = useContext(ContextoDesfazer)
  if (!c) throw new Error('useDesfazer tem de estar dentro de <DesfazerProvider>')
  return { agendar: c.agendar }
}
