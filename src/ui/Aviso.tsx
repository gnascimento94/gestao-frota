// Banner sobre a gravação: erro ao guardar ou programa aberto só para consulta.
// O estado vem de useEstadoArmazenamento (./useEstadoArmazenamento), que consulta a cada 5 s.

import { useCallback, useState } from 'react'
import { dispensarAviso } from '../api/comandos'
import { paraErroApi } from '../ecras/comum'
import type { EstadoGravacao } from './useEstadoArmazenamento'

/**
 * Banner de aviso sobre a gravação. Não mostra nada quando está tudo bem. Uma `notaGravacao`
 * (gravado, mas algo secundário falhou) é informativa: nunca aparece como erro.
 */
export function Aviso({ estado, falhou }: EstadoGravacao) {
  let tom: 'erro' | 'alerta' | 'info' | null = null
  let titulo = ''
  let texto = ''
  let detalhe: string | null = null

  if (estado?.erro) {
    tom = 'erro'
    titulo = 'Não foi possível guardar na pasta da frota.'
    texto =
      'As alterações estão seguras neste computador. O sistema tenta guardar de novo a cada minuto.'
    detalhe = estado.erro
  } else if (estado?.soConsulta) {
    tom = 'alerta'
    titulo = 'Aberto só para consulta.'
    texto = 'Pode ver todos os dados, mas não é possível fazer alterações nesta sessão.'
  } else if (falhou) {
    tom = 'alerta'
    titulo = 'Não foi possível confirmar o estado da gravação.'
    texto = 'O sistema volta a verificar dentro de momentos.'
  } else if (estado?.notaGravacao) {
    tom = 'info'
    titulo = 'Dados guardados.'
    texto = estado.notaGravacao
  }

  if (!tom) return null
  return (
    <div className={`aviso aviso-${tom}`} role={tom === 'erro' ? 'alert' : 'status'}>
      {tom === 'info' ? <IconeInfo /> : <Icone />}
      <div>
        <strong>{titulo}</strong> {texto}
        {detalhe && <span className="aviso-detalhe">Detalhe: {detalhe}</span>}
      </div>
    </div>
  )
}

function IconeInfo() {
  return (
    <svg className="aviso-icone" viewBox="0 0 24 24" aria-hidden="true">
      <circle cx="12" cy="12" r="9" />
      <path d="M12 11v5.5M12 7.5v.01" />
    </svg>
  )
}

function Icone() {
  return (
    <svg className="aviso-icone" viewBox="0 0 24 24" aria-hidden="true">
      <path d="M12 3l9.5 17h-19z" />
      <path d="M12 10v4.5M12 17.5v.01" />
    </svg>
  )
}

/**
 * Aviso do arranque (p.ex. alterações deste PC guardadas em Backup por conflito com outro PC).
 * Fica visível até o utilizador o fechar; `onFechar` chama `dispensarAviso()`. `erro`: o fecho
 * falhou (o aviso volta a ficar visível com a explicação).
 */
export function AvisoArranque({
  texto,
  erro,
  onFechar,
}: {
  texto: string | null
  erro?: string | null
  onFechar(): void
}) {
  if (!texto) return null
  return (
    <div className="aviso aviso-alerta" role="alert">
      <Icone />
      <div className="aviso-texto">
        {texto}
        {erro && <span className="aviso-detalhe">{erro}</span>}
      </div>
      <button type="button" className="aviso-fechar" aria-label="Fechar aviso" onClick={onFechar}>
        <svg viewBox="0 0 16 16" aria-hidden="true">
          <path d="M4 4l8 8M12 4l-8 8" />
        </svg>
      </button>
    </div>
  )
}

/**
 * `AvisoArranque` ligado ao núcleo: esconde-se logo ao fechar (sem esperar pela próxima
 * consulta do estado) e chama `dispensarAviso()`; se isso falhar, volta a aparecer com o erro.
 */
export function AvisoArranqueDispensavel({ texto }: { texto: string | null }) {
  const [fechado, setFechado] = useState<string | null>(null)
  const [erro, setErro] = useState<string | null>(null)
  const fechar = useCallback(() => {
    setFechado(texto)
    setErro(null)
    dispensarAviso().catch((e: unknown) => {
      setFechado(null)
      setErro(`Não foi possível fechar o aviso: ${paraErroApi(e).mensagem}`)
    })
  }, [texto])
  return <AvisoArranque texto={texto !== fechado ? texto : null} erro={erro} onFechar={fechar} />
}