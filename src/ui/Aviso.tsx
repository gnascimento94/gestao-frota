// Banner sobre a gravação: erro ao guardar ou programa aberto só para consulta.
// O estado vem de useEstadoArmazenamento (./useEstadoArmazenamento), que consulta a cada 5 s.

import type { EstadoGravacao } from './useEstadoArmazenamento'

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
      <Icone />
      <div>
        <strong>{titulo}</strong> {texto}
        {detalhe && <span className="aviso-detalhe">Detalhe: {detalhe}</span>}
      </div>
    </div>
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
 * Fica visível até o utilizador o fechar; `onFechar` chama `dispensarAviso()`.
 */
export function AvisoArranque({ texto, onFechar }: { texto: string | null; onFechar(): void }) {
  if (!texto) return null
  return (
    <div className="aviso aviso-alerta" role="alert">
      <Icone />
      <div className="aviso-texto">{texto}</div>
      <button type="button" className="aviso-fechar" aria-label="Fechar aviso" onClick={onFechar}>
        <svg viewBox="0 0 16 16" aria-hidden="true">
          <path d="M4 4l8 8M12 4l-8 8" />
        </svg>
      </button>
    </div>
  )
}
