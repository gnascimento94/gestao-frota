// Folha "+ Lançar": escolhe-se o tipo e o carro e preenche-se o formulário do tipo.
// Após um lançamento bem-sucedido a folha fecha, aparece "Lançado ✓" e o ecrã visível recarrega.

import { AnimatePresence, motion, useReducedMotion } from 'motion/react'
import { useCallback, useEffect, useState, type ReactNode } from 'react'
import { createPortal } from 'react-dom'
import { carrosListar } from '../api/comandos'
import type { Carro } from '../api/tipos'
import { paraErroApi } from '../ecras/comum'
import { CabecalhoFolha, Folha } from '../ui/Folha'
import { FormDocumento } from './FormDocumento'
import { FormKm } from './FormKm'
import { FormManutencao } from './FormManutencao'
import { FormMulta } from './FormMulta'
import { FormPneu } from './FormPneu'
import type { PropsForm } from './logica'
import { ErroGeral, Segmentado, SeletorCarro } from './partes'

export interface LancarProps {
  aberta: boolean
  /** Retângulo do botão que abriu a folha. */
  origem: DOMRect | null
  /** Carro pré-selecionado (folha aberta a partir da ficha). */
  carroId?: number | null
  onFechar(): void
  /** Chamado depois de cada lançamento gravado (para o ecrã visível recarregar). */
  onLancado(): void
  /** Contentor onde os toasts se empilham (ver `.zona-toasts`); sem ele, o toast fica aqui. */
  zonaToasts?: HTMLElement | null
}

type Tipo = 'manutencao' | 'pneu' | 'documento' | 'multa' | 'km'

const TIPOS = [
  ['manutencao', 'Manutenção'],
  ['pneu', 'Pneus'],
  ['documento', 'Documento/taxa'],
  ['multa', 'Multa'],
  ['km', 'Atualizar KM'],
] as const

const DURACAO_TOAST_MS = 1800

export function Lancar({ aberta, origem, carroId = null, onFechar, onLancado, zonaToasts }: LancarProps) {
  const reduzir = useReducedMotion()
  // Cada lançamento incrementa o contador (o toast reinicia mesmo em lançamentos seguidos).
  const [toast, setToast] = useState(0)

  useEffect(() => {
    if (toast === 0) return
    const t = setTimeout(() => setToast(0), DURACAO_TOAST_MS)
    return () => clearTimeout(t)
  }, [toast])

  // O toast vai para a zona de toasts partilhada (empilhado com o "Desfazer"), se existir.
  const portalOuLocal = (no: ReactNode) => (zonaToasts ? createPortal(no, zonaToasts) : no)

  const lancado = useCallback(() => {
    onFechar()
    onLancado()
    setToast((n) => n + 1)
  }, [onFechar, onLancado])

  return (
    <>
      <Folha aberta={aberta} origem={origem} onFechar={onFechar} rotulo="Lançar">
        <CabecalhoFolha titulo="Lançar" onFechar={onFechar} />
        <ConteudoLancar carroInicial={carroId} onLancado={lancado} onCancelar={onFechar} />
      </Folha>
      {portalOuLocal(
        <div className="toast-zona" role="status" aria-live="polite">
        <AnimatePresence>
          {toast > 0 && (
            <motion.div
              key={toast}
              className="toast"
              initial={reduzir ? { opacity: 0 } : { opacity: 0, y: 16, scale: 0.98 }}
              animate={{ opacity: 1, y: 0, scale: 1 }}
              exit={reduzir ? { opacity: 0 } : { opacity: 0, y: 8 }}
              transition={reduzir ? { duration: 0.15 } : { type: 'spring', bounce: 0, duration: 0.35 }}
            >
              Lançado ✓
            </motion.div>
          )}
        </AnimatePresence>
        </div>,
      )}
    </>
  )
}

/** Carros ativos primeiro; depois por matrícula. */
function ordenar(carros: Carro[]): Carro[] {
  return [...carros].sort(
    (a, b) =>
      Number(a.estado !== 'ativo') - Number(b.estado !== 'ativo') ||
      a.matricula.localeCompare(b.matricula, 'pt'),
  )
}

function ConteudoLancar({
  carroInicial,
  onLancado,
  onCancelar,
}: {
  carroInicial: number | null
  onLancado(): void
  onCancelar(): void
}) {
  const [tipo, setTipo] = useState<Tipo>('manutencao')
  const [carroId, setCarroId] = useState<number | null>(carroInicial)
  const [carros, setCarros] = useState<Carro[] | null>(null)
  const [erroCarga, setErroCarga] = useState<string | null>(null)

  useEffect(() => {
    let vivo = true
    // Todos os carros (um inativo/vendido ainda pode receber uma multa ou um documento).
    carrosListar(false, new Date().getFullYear()).then(
      (cs) => vivo && setCarros(ordenar(cs)),
      (e) => vivo && setErroCarga(paraErroApi(e).mensagem),
    )
    return () => {
      vivo = false
    }
  }, [])

  const carro = carros?.find((c) => c.id === carroId) ?? null

  const props: PropsForm = {
    carro,
    seletor: (erro) => (
      <SeletorCarro
        carros={carros ?? []}
        carro={carro}
        onEscolher={setCarroId}
        erro={erro}
        focoInicial={carroInicial === null}
      />
    ),
    focoNoFormulario: carroInicial !== null,
    onLancado,
    onCancelar,
  }

  return (
    <div className="lancar">
      <div className="lancar-tipos">
        <Segmentado
          rotulo="Tipo de lançamento"
          rotuloVisivel={false}
          opcoes={TIPOS}
          valor={tipo}
          onMudar={setTipo}
          classe="seg-tipos"
        />
        <ErroGeral texto={erroCarga} />
      </div>
      {tipo === 'manutencao' && <FormManutencao {...props} />}
      {tipo === 'pneu' && <FormPneu {...props} />}
      {tipo === 'documento' && <FormDocumento {...props} />}
      {tipo === 'multa' && <FormMulta {...props} />}
      {tipo === 'km' && <FormKm {...props} />}
    </div>
  )
}
