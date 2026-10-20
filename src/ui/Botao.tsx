import { forwardRef, useState, type ButtonHTMLAttributes } from 'react'

export type VarianteBotao = 'primario' | 'secundario' | 'simples' | 'perigo'

export interface BotaoProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variante?: VarianteBotao
  tamanho?: 'normal' | 'pequeno'
}

/** Botão com estado "premido" imediato no `pointerdown` (scale 0.97, 100 ms). */
export const Botao = forwardRef<HTMLButtonElement, BotaoProps>(function Botao(
  {
    variante = 'secundario',
    tamanho = 'normal',
    type = 'button',
    className,
    onPointerDown,
    onPointerUp,
    onPointerLeave,
    onPointerCancel,
    ...resto
  },
  ref,
) {
  const [premido, setPremido] = useState(false)
  const classes = ['botao', `botao-${variante}`, tamanho === 'pequeno' && 'botao-pequeno', className]
    .filter(Boolean)
    .join(' ')
  return (
    <button
      ref={ref}
      type={type}
      className={classes}
      data-premido={premido || undefined}
      onPointerDown={(e) => {
        if (e.button === 0) setPremido(true)
        onPointerDown?.(e)
      }}
      onPointerUp={(e) => {
        setPremido(false)
        onPointerUp?.(e)
      }}
      onPointerLeave={(e) => {
        setPremido(false)
        onPointerLeave?.(e)
      }}
      onPointerCancel={(e) => {
        setPremido(false)
        onPointerCancel?.(e)
      }}
      {...resto}
    />
  )
})
