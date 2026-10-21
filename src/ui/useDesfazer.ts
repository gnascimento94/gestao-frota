import { createContext, useContext, useEffect } from 'react'

/** Tempo durante o qual um apagamento pode ser desfeito. */
export const ESPERA_DESFAZER_MS = 8000
/** Tempo máximo de espera pelas ações pendentes quando a janela fecha. */
export const LIMITE_FECHO_MS = 4000

export interface AgendarDesfazer {
  /**
   * Agenda `acao` para daqui a 8 s; até lá o utilizador pode desfazer
   * (e, nesse caso, corre `aoDesfazer`, p.ex. para voltar a mostrar o item escondido).
   * `chave` (p.ex. "plano:7") identifica o item: fica em `escondidas` enquanto o apagamento
   * está pendente ou a correr — mesmo que o ecrã mude e volte — e, depois de apagado, até
   * uma lista sem ele o confirmar (`useEscondidas`), para não piscar. Se a ação falhar,
   * a chave sai, aparece um aviso com a mensagem do núcleo e a App recarrega.
   */
  agendar(rotulo: string, acao: () => Promise<void>, aoDesfazer?: () => void, chave?: string): void
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
  /** Chaves dos itens a esconder (apagamento pendente, a correr ou acabado de fazer). */
  escondidas: ReadonlySet<string>
  /** As chaves já apagadas que não estão em `presentes` deixam de ser escondidas. */
  confirmarApagadas(presentes: ReadonlySet<string>): void
  /** Mensagem de um apagamento que falhou depois dos 8 s. */
  falha: string | null
  fecharFalha(): void
}

export const ContextoDesfazer = createContext<ValorDesfazer | null>(null)

export function useDesfazer(): AgendarDesfazer {
  const c = useContext(ContextoDesfazer)
  if (!c) throw new Error('useDesfazer tem de estar dentro de <DesfazerProvider>')
  return { agendar: c.agendar }
}

/**
 * Chaves a esconder numa lista. `presentes` são as chaves da lista acabada de carregar
 * (ou `null` enquanto carrega): as já apagadas que lá não estão deixam de ser seguidas.
 */
export function useEscondidas(presentes: ReadonlySet<string> | null): ReadonlySet<string> {
  const c = useContext(ContextoDesfazer)
  if (!c) throw new Error('useEscondidas tem de estar dentro de <DesfazerProvider>')
  const { confirmarApagadas, escondidas } = c
  useEffect(() => {
    if (presentes) confirmarApagadas(presentes)
  }, [presentes, confirmarApagadas])
  return escondidas
}
