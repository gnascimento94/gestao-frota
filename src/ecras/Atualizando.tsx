// Atualização automática ao abrir (spec §3): depois de um arranque com dados abertos
// pergunta ao núcleo se há versão nova (≤ 5 s; sem internet abre normalmente, sem aviso).
// Se houver, mostra "A atualizar para a versão X.Y.Z…" e aplica-a: o programa reinicia
// sozinho. Em erro, mostra a mensagem e deixa continuar com a versão atual.

import { useEffect, useRef, useState, type ReactNode } from 'react'
import { aplicarAtualizacao, verificarAtualizacao } from '../api/comandos'
import type { NovaVersao } from '../api/tipos'
import { Botao } from '../ui/Botao'
import { EcraArranque } from './Arranque'
import { paraErroApi } from './comum'

/** Rede de segurança: o núcleo desiste ao fim de 5 s; a interface não espera mais do que isto. */
const LIMITE_VERIFICAR_MS = 6000

type Fase =
  | { tipo: 'aVerificar' }
  | { tipo: 'atual' }
  | { tipo: 'aAtualizar'; versao: string }
  | { tipo: 'erro'; versao: string; mensagem: string }

export function VerificarAtualizacao({ children }: { children: ReactNode }) {
  const [fase, setFase] = useState<Fase>({ tipo: 'aVerificar' })
  // Os pedidos fazem-se uma só vez (o StrictMode corre os efeitos duas vezes).
  const verificacao = useRef<Promise<NovaVersao | null> | null>(null)
  const aplicacao = useRef<Promise<void> | null>(null)

  useEffect(() => {
    let vivo = true
    const seAinda = (f: Fase) => vivo && setFase((atual) => (atual.tipo === 'aVerificar' ? f : atual))
    const limite = setTimeout(() => seAinda({ tipo: 'atual' }), LIMITE_VERIFICAR_MS)
    verificacao.current ??= verificarAtualizacao()
    verificacao.current.then(
      (nova) => seAinda(nova ? { tipo: 'aAtualizar', versao: nova.versao } : { tipo: 'atual' }),
      () => seAinda({ tipo: 'atual' }),
    )
    return () => {
      vivo = false
      clearTimeout(limite)
    }
  }, [])

  const aAtualizar = fase.tipo === 'aAtualizar' ? fase.versao : null
  useEffect(() => {
    if (aAtualizar === null) return
    let vivo = true
    aplicacao.current ??= aplicarAtualizacao()
    // Em sucesso o programa reinicia; só o erro volta aqui.
    aplicacao.current.catch(
      (e: unknown) => vivo && setFase({ tipo: 'erro', versao: aAtualizar, mensagem: paraErroApi(e).mensagem }),
    )
    return () => {
      vivo = false
    }
  }, [aAtualizar])

  switch (fase.tipo) {
    case 'atual':
      return <>{children}</>
    case 'aVerificar':
      return <div className="arranque" aria-busy="true" />
    case 'aAtualizar':
      return <Atualizando versao={fase.versao} />
    case 'erro':
      return (
        <EcraArranque
          titulo={`Não foi possível atualizar para a versão ${fase.versao}.`}
          botoes={
            <Botao variante="primario" onClick={() => setFase({ tipo: 'atual' })}>
              Continuar com a versão atual
            </Botao>
          }
        >
          <p>{fase.mensagem}</p>
        </EcraArranque>
      )
  }
}

export function Atualizando({ versao }: { versao: string }) {
  return (
    <EcraArranque titulo={`A atualizar para a versão ${versao}…`}>
      <div className="atualizando" aria-live="polite">
        <span className="atualizando-indicador" role="progressbar" aria-label="A atualizar" />
        <p>Os dados ficam guardados. O programa volta a abrir sozinho dentro de momentos.</p>
      </div>
    </EcraArranque>
  )
}
