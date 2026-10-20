import {
  forwardRef,
  useId,
  type InputHTMLAttributes,
  type ReactNode,
  type SelectHTMLAttributes,
} from 'react'

export type TomAjuda = 'normal' | 'ok' | 'alerta'

interface Comum {
  rotulo: string
  /** Texto de ajuda por baixo do campo. */
  ajuda?: ReactNode
  tomAjuda?: TomAjuda
  /** Mensagem de erro (substitui a ajuda e marca o campo como inválido). */
  erro?: string | null
  classeCampo?: string
}

function useLigacoes(id: string | undefined, erro: string | null | undefined, ajuda: ReactNode) {
  const gerado = useId()
  const idCampo = id ?? gerado
  const idDescricao = `${idCampo}-desc`
  const temDescricao = !!erro || (ajuda !== undefined && ajuda !== null && ajuda !== false)
  return { idCampo, idDescricao, temDescricao }
}

function Descricao({ id, erro, ajuda, tom }: { id: string; erro?: string | null; ajuda?: ReactNode; tom: TomAjuda }) {
  if (erro)
    return (
      <span id={id} className="campo-erro">
        {erro}
      </span>
    )
  if (ajuda === undefined || ajuda === null || ajuda === false) return null
  return (
    <span id={id} className={`campo-ajuda campo-ajuda-${tom}`}>
      {ajuda}
    </span>
  )
}

export type CampoProps = Comum & InputHTMLAttributes<HTMLInputElement>

/** Campo de texto com rótulo, ajuda e erro ligados por `aria-describedby`. */
export const Campo = forwardRef<HTMLInputElement, CampoProps>(function Campo(
  { rotulo, ajuda, tomAjuda = 'normal', erro, classeCampo, id, className, ...resto },
  ref,
) {
  const { idCampo, idDescricao, temDescricao } = useLigacoes(id, erro, ajuda)
  return (
    <div className={['campo', erro && 'campo-invalido', classeCampo].filter(Boolean).join(' ')}>
      <label htmlFor={idCampo}>{rotulo}</label>
      <input
        ref={ref}
        id={idCampo}
        className={['campo-entrada', className].filter(Boolean).join(' ')}
        aria-invalid={erro ? true : undefined}
        aria-describedby={temDescricao ? idDescricao : undefined}
        {...resto}
      />
      <Descricao id={idDescricao} erro={erro} ajuda={ajuda} tom={tomAjuda} />
    </div>
  )
})

export type CampoSelecaoProps = Comum & SelectHTMLAttributes<HTMLSelectElement>

/** Lista pendente com o mesmo aspeto e ligações do `Campo`. */
export const CampoSelecao = forwardRef<HTMLSelectElement, CampoSelecaoProps>(function CampoSelecao(
  { rotulo, ajuda, tomAjuda = 'normal', erro, classeCampo, id, className, children, ...resto },
  ref,
) {
  const { idCampo, idDescricao, temDescricao } = useLigacoes(id, erro, ajuda)
  return (
    <div className={['campo', erro && 'campo-invalido', classeCampo].filter(Boolean).join(' ')}>
      <label htmlFor={idCampo}>{rotulo}</label>
      <select
        ref={ref}
        id={idCampo}
        className={['campo-entrada', className].filter(Boolean).join(' ')}
        aria-invalid={erro ? true : undefined}
        aria-describedby={temDescricao ? idDescricao : undefined}
        {...resto}
      >
        {children}
      </select>
      <Descricao id={idDescricao} erro={erro} ajuda={ajuda} tom={tomAjuda} />
    </div>
  )
})
