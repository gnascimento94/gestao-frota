// Barra lateral: logo, navegação, botão "+ Lançar" e linha de estado da gravação.

import type { Ref } from 'react'
import type { Ecra } from '../App'
import { formatarHora } from '../util/formato'
import type { EstadoGravacao } from './useEstadoArmazenamento'
import { Botao } from './Botao'

type NomeNav = Exclude<Ecra['nome'], 'carro'>

const ITENS: { nome: NomeNav; rotulo: string; icone: string }[] = [
  { nome: 'inicio', rotulo: 'Início', icone: 'M3 11l9-7 9 7v9a1 1 0 0 1-1 1h-5v-6H9v6H4a1 1 0 0 1-1-1z' },
  {
    nome: 'carros',
    rotulo: 'Carros',
    icone:
      'M5 16l1.5-5.5A2 2 0 0 1 8.4 9h7.2a2 2 0 0 1 1.9 1.5L19 16M4.5 16h15a1.5 1.5 0 0 1 1.5 1.5V20H3v-2.5A1.5 1.5 0 0 1 4.5 16zM7 20v1M17 20v1',
  },
  { nome: 'motoristas', rotulo: 'Motoristas', icone: 'M12 4a4 4 0 1 1 0 8 4 4 0 0 1 0-8zM4 21c1-4 4.5-6 8-6s7 2 8 6' },
  { nome: 'relatorios', rotulo: 'Relatórios', icone: 'M4 20V10M10 20V4M16 20v-7M22 20H2' },
  {
    nome: 'planos',
    rotulo: 'Planos de manutenção',
    icone: 'M14.7 6.3a4 4 0 0 0-5.4 5.4L3 18l3 3 6.3-6.3a4 4 0 0 0 5.4-5.4l-2.5 2.5-2.4-.6-.6-2.4z',
  },
  {
    nome: 'backup',
    rotulo: 'Backup',
    icone: 'M7 18a5 5 0 1 1 1-9.9A6 6 0 0 1 19 10a4 4 0 0 1 0 8zM12 11v6M9.5 13.5L12 11l2.5 2.5',
  },
]

/** Item da navegação ativo para o ecrã (a ficha de um carro pertence a "Carros"). */
function navAtiva(ecra: Ecra): NomeNav {
  return ecra.nome === 'carro' ? 'carros' : ecra.nome
}

export interface BarraProps {
  ecra: Ecra
  onNavegar(ecra: Ecra): void
  /** Recebe o retângulo do botão "+ Lançar" (origem da animação da folha). */
  onLancar(origem: DOMRect | null): void
  refLancar?: Ref<HTMLButtonElement>
  gravacao: EstadoGravacao
}

export function Barra({ ecra, onNavegar, onLancar, refLancar, gravacao }: BarraProps) {
  const ativa = navAtiva(ecra)
  return (
    <aside className="barra" aria-label="Navegação principal">
      <div className="marca">
        <span className="marca-logo" role="img" aria-label="Logótipo" />
        <div>
          <span className="marca-nome">Frota</span>
          <small className="marca-sub">Gestão de frota</small>
        </div>
      </div>

      <nav className="nav">
        {ITENS.map((item) => (
          <button
            key={item.nome}
            type="button"
            className="nav-item"
            aria-current={ativa === item.nome ? 'page' : undefined}
            onClick={() => onNavegar({ nome: item.nome })}
          >
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <path d={item.icone} />
            </svg>
            <span>{item.rotulo}</span>
          </button>
        ))}
      </nav>

      <div className="barra-espaco" />

      <Botao
        ref={refLancar}
        variante="primario"
        className="botao-lancar"
        aria-haspopup="dialog"
        aria-keyshortcuts="Control+N"
        onClick={(e) => onLancar(e.currentTarget.getBoundingClientRect())}
      >
        + Lançar <kbd aria-hidden="true">Ctrl N</kbd>
      </Botao>

      <LinhaEstado {...gravacao} />
    </aside>
  )
}

function LinhaEstado({ estado, falhou }: EstadoGravacao) {
  let tom: 'ok' | 'erro' | 'alerta' | 'neutro' = 'neutro'
  let texto = 'A verificar…'
  let titulo: string | undefined

  if (estado?.erro) {
    tom = 'erro'
    texto = 'Por guardar · erro'
  } else if (estado?.soConsulta) {
    tom = 'alerta'
    texto = 'Só para consulta'
  } else if (estado) {
    const hora = estado.ultimaGravacao ? formatarHora(estado.ultimaGravacao) : null
    tom = hora ? 'ok' : 'neutro'
    texto = hora ? `Guardado · ${hora}` : 'Sem alterações por guardar'
  } else if (falhou) {
    tom = 'alerta'
    texto = 'Estado desconhecido'
  }
  if (estado) titulo = `${estado.pasta}${estado.noGoogleDrive ? ' (Google Drive)' : ''} · v${estado.versaoApp}`

  return (
    <div className="estado-gravacao" title={titulo} role="status">
      <i className={`ponto ponto-${tom}`} aria-hidden="true" />
      {texto}
    </div>
  )
}
