// "Desfazer" após apagar: a ação fica pendente 8 s; se o utilizador não desfizer, corre.
// Ao sair (beforeunload) ou ao fechar a janela, as pendentes correm de imediato.

import { AnimatePresence, motion, useIsPresent, useReducedMotion } from 'motion/react'
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react'

export const ESPERA_DESFAZER_MS = 8000

interface Pendente {
  id: number
  rotulo: string
  acao: () => Promise<void>
  temporizador: ReturnType<typeof setTimeout>
}

interface ContextoDesfazer {
  agendar(rotulo: string, acao: () => Promise<void>): void
  /** Uso interno de `<Desfazer/>`. */
  atual: { id: number; rotulo: string } | null
  desfazer(id: number): void
}

const Contexto = createContext<ContextoDesfazer | null>(null)

export function useDesfazer(): { agendar(rotulo: string, acao: () => Promise<void>): void } {
  const c = useContext(Contexto)
  if (!c) throw new Error('useDesfazer tem de estar dentro de <DesfazerProvider>')
  return { agendar: c.agendar }
}

/**
 * Regista `executar` para correr quando a janela vai fechar; devolve a função para desligar.
 * Por omissão usa `getCurrentWindow().onCloseRequested` do Tauri (só quando corre dentro do Tauri).
 */
export type AoFecharJanela = (executar: () => Promise<void>) => (() => void) | Promise<() => void>

const aoFecharJanelaTauri: AoFecharJanela = async (executar) => {
  if (typeof window === 'undefined' || !('__TAURI_INTERNALS__' in window)) return () => {}
  const { getCurrentWindow } = await import('@tauri-apps/api/window')
  // O Tauri espera por este handler e depois destrói a janela.
  return getCurrentWindow().onCloseRequested(() => executar())
}

async function correr(acao: () => Promise<void>) {
  try {
    await acao()
  } catch (e) {
    console.error('Falha ao executar ação pendente', e)
  }
}

export function DesfazerProvider({
  children,
  aoFecharJanela = aoFecharJanelaTauri,
}: {
  children: ReactNode
  aoFecharJanela?: AoFecharJanela
}) {
  const pendentes = useRef(new Map<number, Pendente>())
  const proximoId = useRef(1)
  const [atual, setAtual] = useState<{ id: number; rotulo: string } | null>(null)

  const mostrarUltima = useCallback(() => {
    const ultima = [...pendentes.current.values()].at(-1)
    setAtual(ultima ? { id: ultima.id, rotulo: ultima.rotulo } : null)
  }, [])

  const executar = useCallback(
    (id: number) => {
      const p = pendentes.current.get(id)
      if (!p) return Promise.resolve()
      clearTimeout(p.temporizador)
      pendentes.current.delete(id)
      mostrarUltima()
      return correr(p.acao)
    },
    [mostrarUltima],
  )

  const executarTodas = useCallback(async () => {
    await Promise.all([...pendentes.current.keys()].map((id) => executar(id)))
  }, [executar])

  const agendar = useCallback(
    (rotulo: string, acao: () => Promise<void>) => {
      const id = proximoId.current++
      const temporizador = setTimeout(() => void executar(id), ESPERA_DESFAZER_MS)
      pendentes.current.set(id, { id, rotulo, acao, temporizador })
      setAtual({ id, rotulo })
    },
    [executar],
  )

  const desfazer = useCallback(
    (id: number) => {
      const p = pendentes.current.get(id)
      if (!p) return
      clearTimeout(p.temporizador)
      pendentes.current.delete(id)
      mostrarUltima()
    },
    [mostrarUltima],
  )

  // Ref para os handlers de saída verem sempre a versão atual.
  const executarTodasRef = useRef(executarTodas)
  useLayoutEffect(() => {
    executarTodasRef.current = executarTodas
  })

  useEffect(() => {
    const antesDeSair = () => void executarTodasRef.current()
    window.addEventListener('beforeunload', antesDeSair)

    let desligar: (() => void) | null = null
    let ativo = true
    Promise.resolve(aoFecharJanela(() => executarTodasRef.current()))
      .then((d) => {
        if (ativo) desligar = d
        else d()
      })
      .catch((e) => console.error('Não foi possível vigiar o fecho da janela', e))

    return () => {
      ativo = false
      window.removeEventListener('beforeunload', antesDeSair)
      desligar?.()
    }
  }, [aoFecharJanela])

  // Se o provider desaparecer, o apagamento pedido não se perde.
  useEffect(() => () => void executarTodasRef.current(), [])

  const valor = useMemo(() => ({ agendar, atual, desfazer }), [agendar, atual, desfazer])
  return <Contexto.Provider value={valor}>{children}</Contexto.Provider>
}

/** Toast "Apagado · Desfazer" para a ação pendente mais recente. */
export function Desfazer() {
  const c = useContext(Contexto)
  if (!c) throw new Error('<Desfazer/> tem de estar dentro de <DesfazerProvider>')
  const reduzir = useReducedMotion()
  const { atual, desfazer } = c

  return (
    <div className="desfazer-zona" role="status" aria-live="polite">
      <AnimatePresence>
        {atual && (
          <Toast
            key={atual.id}
            rotulo={atual.rotulo}
            reduzir={!!reduzir}
            onDesfazer={() => desfazer(atual.id)}
          />
        )}
      </AnimatePresence>
    </div>
  )
}

function Toast({
  rotulo,
  reduzir,
  onDesfazer,
}: {
  rotulo: string
  reduzir: boolean
  onDesfazer(): void
}) {
  // Durante a animação de saída o toast já não é interativo nem anunciado.
  const presente = useIsPresent()
  return (
    <motion.div
      className="desfazer"
      aria-hidden={presente ? undefined : true}
      initial={reduzir ? { opacity: 0 } : { opacity: 0, y: 16, scale: 0.98 }}
      animate={{ opacity: 1, y: 0, scale: 1 }}
      exit={reduzir ? { opacity: 0 } : { opacity: 0, y: 8 }}
      transition={reduzir ? { duration: 0.15 } : { type: 'spring', bounce: 0, duration: 0.35 }}
    >
      <span>{rotulo}</span>
      <span aria-hidden="true" className="desfazer-sep">
        ·
      </span>
      <button type="button" className="desfazer-botao" onClick={onDesfazer} disabled={!presente}>
        Desfazer
      </button>
    </motion.div>
  )
}
