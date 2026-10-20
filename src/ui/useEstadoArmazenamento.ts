import { useEffect, useRef, useState } from 'react'
import { estadoArmazenamento } from '../api/comandos'
import type { EstadoArmazenamento } from '../api/tipos'

export interface EstadoGravacao {
  /** Último estado conhecido (`null` até à primeira resposta). */
  estado: EstadoArmazenamento | null
  /** A última consulta falhou (o estado mostrado pode estar desatualizado). */
  falhou: boolean
}

/** Consulta `estadoArmazenamento()` de imediato e depois a cada `intervaloMs` (sem sobreposição). */
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
