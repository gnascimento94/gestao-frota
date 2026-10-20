// "Desfazer" após apagar: a ação fica pendente 8 s; se o utilizador não desfizer, corre.
// Ao sair (beforeunload) ou ao fechar a janela, as pendentes correm de imediato e o fecho
// espera por elas (e pelas que já estavam a correr), no máximo 4 s.
// O hook `useDesfazer` está em ./useDesfazer.

import { AnimatePresence, motion, useIsPresent, useReducedMotion } from 'motion/react'
import {
  useCallback,
  useContext,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react'
import {
  ContextoDesfazer,
  ESPERA_DESFAZER_MS,
  LIMITE_FECHO_MS,
  type ResumoPendentes,
} from './useDesfazer'

interface Pendente {
  rotulo: string
  acao: () => Promise<void>
  aoDesfazer?: () => void
  temporizador: ReturnType<typeof setTimeout>
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

function resumir(pendentes: Map<number, Pendente>): ResumoPendentes | null {
  const n = pendentes.size
  if (n === 0) return null
  if (n === 1) return { texto: [...pendentes.values()][0].rotulo, quantas: 1 }
  return { texto: `Apagados (${n})`, quantas: n }
}

export function DesfazerProvider({
  children,
  aoFecharJanela = aoFecharJanelaTauri,
}: {
  children: ReactNode
  aoFecharJanela?: AoFecharJanela
}) {
  const pendentes = useRef(new Map<number, Pendente>())
  const emCurso = useRef(new Set<Promise<void>>())
  const proximoId = useRef(1)
  const [resumo, setResumo] = useState<ResumoPendentes | null>(null)

  const atualizar = useCallback(() => setResumo(resumir(pendentes.current)), [])

  const executar = useCallback(
    (id: number) => {
      const p = pendentes.current.get(id)
      if (!p) return
      clearTimeout(p.temporizador)
      pendentes.current.delete(id)
      atualizar()
      const promessa = correr(p.acao)
      emCurso.current.add(promessa)
      void promessa.finally(() => emCurso.current.delete(promessa))
    },
    [atualizar],
  )

  /** Corre as pendentes e espera por todas as que estão a correr. */
  const executarTodas = useCallback(async () => {
    for (const id of [...pendentes.current.keys()]) executar(id)
    while (emCurso.current.size > 0) await Promise.all([...emCurso.current])
  }, [executar])

  const agendar = useCallback(
    (rotulo: string, acao: () => Promise<void>, aoDesfazer?: () => void) => {
      const id = proximoId.current++
      const temporizador = setTimeout(() => executar(id), ESPERA_DESFAZER_MS)
      pendentes.current.set(id, { rotulo, acao, aoDesfazer, temporizador })
      atualizar()
    },
    [executar, atualizar],
  )

  const desfazerTodas = useCallback(() => {
    const desfeitas = [...pendentes.current.values()]
    for (const p of desfeitas) clearTimeout(p.temporizador)
    pendentes.current.clear()
    atualizar()
    for (const p of desfeitas) p.aoDesfazer?.()
  }, [atualizar])

  // Ref para os handlers de saída verem sempre a versão atual.
  const executarTodasRef = useRef(executarTodas)
  useLayoutEffect(() => {
    executarTodasRef.current = executarTodas
  })

  useEffect(() => {
    const antesDeSair = () => void executarTodasRef.current()
    window.addEventListener('beforeunload', antesDeSair)

    // No fecho, espera no máximo LIMITE_FECHO_MS para não prender a janela.
    const aoFechar = () =>
      new Promise<void>((resolver) => {
        const limite = setTimeout(resolver, LIMITE_FECHO_MS)
        void executarTodasRef.current().finally(() => {
          clearTimeout(limite)
          resolver()
        })
      })

    let desligar: (() => void) | null = null
    let ativo = true
    Promise.resolve(aoFecharJanela(aoFechar))
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

  const valor = useMemo(() => ({ agendar, resumo, desfazerTodas }), [agendar, resumo, desfazerTodas])
  return <ContextoDesfazer.Provider value={valor}>{children}</ContextoDesfazer.Provider>
}

function editavel(alvo: EventTarget | null) {
  return (
    alvo instanceof HTMLElement &&
    (alvo.isContentEditable || alvo instanceof HTMLInputElement || alvo instanceof HTMLTextAreaElement)
  )
}

/** Toast "Apagado · Desfazer" (ou "Apagados (N) · Desfazer"). Ctrl+Z desfaz enquanto está visível. */
export function Desfazer() {
  const c = useContext(ContextoDesfazer)
  if (!c) throw new Error('<Desfazer/> tem de estar dentro de <DesfazerProvider>')
  const reduzir = useReducedMotion()
  const { resumo, desfazerTodas } = c
  const visivel = resumo !== null

  useEffect(() => {
    if (!visivel) return
    const tecla = (e: KeyboardEvent) => {
      if (!e.ctrlKey || e.shiftKey || e.altKey || e.metaKey || e.key.toLowerCase() !== 'z') return
      if (editavel(e.target)) return // nos campos, Ctrl+Z desfaz o texto
      e.preventDefault()
      desfazerTodas()
    }
    window.addEventListener('keydown', tecla)
    return () => window.removeEventListener('keydown', tecla)
  }, [visivel, desfazerTodas])

  return (
    <div className="desfazer-zona" role="status" aria-live="polite">
      <AnimatePresence>
        {resumo && (
          <Toast key="desfazer" texto={resumo.texto} reduzir={!!reduzir} onDesfazer={desfazerTodas} />
        )}
      </AnimatePresence>
    </div>
  )
}

function Toast({ texto, reduzir, onDesfazer }: { texto: string; reduzir: boolean; onDesfazer(): void }) {
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
      <span>{texto}</span>
      <span aria-hidden="true" className="desfazer-sep">
        ·
      </span>
      <button
        type="button"
        className="desfazer-botao"
        onClick={onDesfazer}
        disabled={!presente}
        aria-keyshortcuts="Control+Z"
      >
        Desfazer
      </button>
    </motion.div>
  )
}
