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
