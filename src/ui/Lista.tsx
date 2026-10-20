import type { ReactNode } from 'react'

/** Gravidade de um alerta: vencido (vermelho) ou próximo (laranja). */
export type Severidade = 'vencido' | 'proximo'

export function Lista({ rotulo, children }: { rotulo?: string; children: ReactNode }) {
  return (
    <ul className="lista" aria-label={rotulo}>
      {children}
    </ul>
  )
}

export interface ItemListaProps {
  /** Faixa de cor à esquerda. */
  severidade?: Severidade | null
  /** Elemento à esquerda (matrícula, ícone). */
  inicio?: ReactNode
  titulo: ReactNode
  detalhe?: ReactNode
  /** Elemento à direita (valor, pílula). */
  fim?: ReactNode
  /** Torna a linha um botão. */
  onClick?: () => void
  /** Destaca a linha escolhida. */
  selecionado?: boolean
}

export function ItemLista({ severidade, inicio, titulo, detalhe, fim, onClick, selecionado }: ItemListaProps) {
  const conteudo = (
    <>
      {severidade && <i className={`faixa faixa-${severidade}`} aria-hidden="true" />}
      {inicio}
      <span className="item-texto">
        <span className="item-titulo">{titulo}</span>
        {detalhe && <span className="item-detalhe">{detalhe}</span>}
      </span>
      {fim}
    </>
  )
  return (
    <li className={selecionado ? 'item-lista item-selecionado' : 'item-lista'}>
      {onClick ? (
        <button
          type="button"
          className="item-corpo item-clicavel"
          onClick={onClick}
          aria-current={selecionado ? 'true' : undefined}
        >
          {conteudo}
        </button>
      ) : (
        <div className="item-corpo">{conteudo}</div>
      )}
    </li>
  )
}

/** Matrícula com o aspeto de chapa portuguesa. */
export function Matricula({ children }: { children: ReactNode }) {
  return <span className="matricula">{children}</span>
}

/** Pílula de estado colorida pela severidade. */
export function Pilula({ severidade, children }: { severidade: Severidade; children: ReactNode }) {
  return <span className={`pilula pilula-${severidade}`}>{children}</span>
}

/** Valor numérico alinhado (algarismos tabulares). */
export function Valor({ children, discreto }: { children: ReactNode; discreto?: boolean }) {
  return <span className={discreto ? 'valor valor-discreto' : 'valor'}>{children}</span>
}
