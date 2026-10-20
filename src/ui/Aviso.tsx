// Estado da gravação: consulta `estadoArmazenamento()` a cada 5 s e mostra um aviso
// quando a gravação falhou ou o programa está aberto só para consulta.

import { useEffect, useRef, useState } from 'react'
import { estadoArmazenamento } from '../api/comandos'
import type { EstadoArmazenamento } from '../api/tipos'

export interface EstadoGravacao {
  /** Último estado conhecido (`null` até à primeira resposta). */
  estado: EstadoArmazenamento | null
  /** A última consulta falhou (o estado mostrado pode estar desatualizado). */
  falhou: boolean
}

export function useEstadoArmazenamento(
  intervaloMs = 5000,
  consultar: () => Promise<EstadoArmazenamento> = estadoArmazenamento,
): EstadoGravacao {
  const [valor, setValor] = useState<EstadoGravacao>({ estado: null, falhou: false })
  const consultarRef = useRef(consultar)
  useEffect(() => {
    consultarRef.current = consultar
  })

  useEffect(() => {
    let ativo = true
    let emCurso = false
    const atualizar = async () => {
      if (emCurso) return
      emCurso = true
      try {
        const estado = await consultarRef.current()
        if (ativo) setValor({ estado, falhou: false })
      } catch {
        if (ativo) setValor((v) => ({ estado: v.estado, falhou: true }))
      } finally {
        emCurso = false
      }
    }
    void atualizar()
    const t = setInterval(() => void atualizar(), intervaloMs)
    return () => {
      ativo = false
      clearInterval(t)
    }
  }, [intervaloMs])

  return valor
}

/** Banner de aviso sobre a gravação. Não mostra nada quando está tudo bem. */
export function Aviso({ estado, falhou }: EstadoGravacao) {
  let tom: 'erro' | 'alerta' | null = null
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
  }

  if (!tom) return null
  return (
    <div className={`aviso aviso-${tom}`} role={tom === 'erro' ? 'alert' : 'status'}>
      <svg className="aviso-icone" viewBox="0 0 24 24" aria-hidden="true">
        <path d="M12 3l9.5 17h-19z" />
        <path d="M12 10v4.5M12 17.5v.01" />
      </svg>
      <div>
        <strong>{titulo}</strong> {texto}
        {detalhe && <span className="aviso-detalhe">Detalhe: {detalhe}</span>}
      </div>
    </div>
  )
}
