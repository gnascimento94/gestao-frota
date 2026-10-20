import { useId, type ReactNode } from 'react'

export interface CartaoProps {
  titulo?: ReactNode
  /** Número discreto ao lado do título (p.ex. n.º de itens). */
  contagem?: number
  /** Conteúdo à direita do título (botão, ligação…). */
  acao?: ReactNode
  /** Sem espaçamento interno — para listas e tabelas que vão até à borda. */
  semMargem?: boolean
  className?: string
  children?: ReactNode
}

export function Cartao({ titulo, contagem, acao, semMargem, className, children }: CartaoProps) {
  const id = useId()
  return (
    <section
      className={['cartao', semMargem && 'cartao-sem-margem', className].filter(Boolean).join(' ')}
      aria-labelledby={titulo ? id : undefined}
    >
      {titulo && (
        <header className="cartao-topo">
          <h2 id={id} className="cartao-titulo">
            {titulo}
            {contagem !== undefined && <span className="cartao-contagem">{contagem}</span>}
          </h2>
          {acao && <div className="cartao-acao">{acao}</div>}
        </header>
      )}
      {children}
    </section>
  )
}
