// Painel lateral (folha) que nasce do botão que o abriu e regressa a ele ao fechar.
// Mola sem ressalto; com movimento reduzido, fade de 150 ms. Nunca bloqueia o rato:
// durante a saída, folha e véu deixam de receber cliques.

import { AnimatePresence, motion, useIsPresent, useReducedMotion, type Transition } from 'motion/react'
import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from 'react'
import { createPortal } from 'react-dom'

export interface FolhaProps {
  aberta: boolean
  /** Retângulo do botão que abriu a folha (a animação parte e regressa daqui). */
  origem: DOMRect | null
  onFechar(): void
  /** Nome acessível do diálogo. */
  rotulo?: string
  children?: ReactNode
}

const LARGURA = 440
const MOLA: Transition = { type: 'spring', bounce: 0, duration: 0.35 }

export function Folha({ aberta, origem, onFechar, rotulo = 'Painel', children }: FolhaProps) {
  return createPortal(
    <AnimatePresence>
      {aberta && (
        <ConteudoFolha key="folha" origem={origem} onFechar={onFechar} rotulo={rotulo}>
          {children}
        </ConteudoFolha>
      )}
    </AnimatePresence>,
    document.body,
  )
}

interface Geometria {
  initial: Record<string, number | string>
  origem: string
}

/** Escala e transform-origin para que a folha coincida com o retângulo de origem. */
function geometria(origem: DOMRect | null): Geometria {
  const W = window.innerWidth
  const H = window.innerHeight
  const w = Math.min(LARGURA, W)
  const esquerda = W - w
  if (!origem || origem.width === 0 || origem.height === 0 || W === 0 || H === 0) {
    return { initial: { opacity: 0, x: 30, scaleX: 0.94, scaleY: 0.94, filter: 'blur(6px)' }, origem: '50% 50%' }
  }
  const sx = Math.min(Math.max(origem.width / w, 0.05), 0.95)
  const sy = Math.min(Math.max(origem.height / H, 0.02), 0.95)
  // Ponto fixo O tal que O + s·(caixa − O) = origem, por eixo.
  const ox = (origem.left - sx * esquerda) / (1 - sx) - esquerda
  const oy = origem.top / (1 - sy)
  return {
    initial: { opacity: 0, x: 0, scaleX: sx, scaleY: sy, filter: 'blur(8px)' },
    origem: `${ox}px ${oy}px`,
  }
}

const FOCAVEIS =
  'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])'

function ConteudoFolha({
  origem,
  onFechar,
  rotulo,
  children,
}: {
  origem: DOMRect | null
  onFechar(): void
  rotulo: string
  children?: ReactNode
}) {
  const presente = useIsPresent()
  const reduzir = useReducedMotion()
  const ref = useRef<HTMLElement>(null)
  const anterior = useRef<Element | null>(null)
  // Calculada uma vez ao abrir; a saída regressa ao mesmo retângulo.
  const [g] = useState(() => geometria(origem))
  const onFecharRef = useRef(onFechar)
  useLayoutEffect(() => {
    onFecharRef.current = onFechar
  })

  // Foco: entra na folha ao abrir, volta ao botão de origem ao fechar.
  useEffect(() => {
    if (!presente) return
    anterior.current = document.activeElement
    const folha = ref.current
    const alvo = folha?.querySelector<HTMLElement>('[autofocus], [data-foco-inicial]') ??
      folha?.querySelector<HTMLElement>(FOCAVEIS) ?? folha
    alvo?.focus({ preventScroll: true })
    return () => {
      const a = anterior.current
      if (a instanceof HTMLElement && a.isConnected) a.focus({ preventScroll: true })
    }
  }, [presente])

  // Esc fecha; Tab fica dentro da folha.
  useEffect(() => {
    if (!presente) return
    const tecla = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault()
        onFecharRef.current()
        return
      }
      if (e.key !== 'Tab' || !ref.current) return
      const focaveis = [...ref.current.querySelectorAll<HTMLElement>(FOCAVEIS)]
      if (focaveis.length === 0) {
        e.preventDefault()
        return
      }
      const primeiro = focaveis[0]
      const ultimo = focaveis[focaveis.length - 1]
      const ativo = document.activeElement
      if (e.shiftKey && (ativo === primeiro || ativo === ref.current)) {
        e.preventDefault()
        ultimo.focus()
      } else if (!e.shiftKey && ativo === ultimo) {
        e.preventDefault()
        primeiro.focus()
      } else if (!ref.current.contains(ativo)) {
        e.preventDefault()
        primeiro.focus()
      }
    }
    document.addEventListener('keydown', tecla)
    return () => document.removeEventListener('keydown', tecla)
  }, [presente])

  const fade = { duration: 0.15 }
  const cliques = presente ? 'auto' : 'none'

  return (
    <>
      <motion.div
        className="veu"
        aria-hidden="true"
        style={{ pointerEvents: cliques }}
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        exit={{ opacity: 0 }}
        transition={reduzir ? fade : { duration: 0.25 }}
        onClick={onFechar}
      />
      <motion.section
        ref={ref}
        className="folha"
        role="dialog"
        aria-modal="true"
        aria-label={rotulo}
        tabIndex={-1}
        style={{ pointerEvents: cliques, transformOrigin: g.origem }}
        initial={reduzir ? { opacity: 0 } : g.initial}
        animate={reduzir ? { opacity: 1 } : { opacity: 1, x: 0, scaleX: 1, scaleY: 1, filter: 'blur(0px)' }}
        exit={reduzir ? { opacity: 0 } : g.initial}
        transition={reduzir ? fade : { ...MOLA, opacity: { duration: 0.2 } }}
      >
        {children}
      </motion.section>
    </>
  )
}

/** Cabeçalho padrão da folha: título e botão de fechar. */
export function CabecalhoFolha({ titulo, onFechar }: { titulo: ReactNode; onFechar(): void }) {
  return (
    <header className="folha-topo">
      <h2 className="folha-titulo">{titulo}</h2>
      <button type="button" className="folha-fechar" aria-label="Fechar" onClick={onFechar}>
        <svg viewBox="0 0 16 16" aria-hidden="true">
          <path d="M4 4l8 8M12 4l-8 8" />
        </svg>
      </button>
    </header>
  )
}
