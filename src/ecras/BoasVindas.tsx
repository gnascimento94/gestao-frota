import { useCallback, type ReactNode } from 'react'
import { carrosListar } from '../api/comandos'
import type { Ecra } from '../App'
import { Cartao } from '../ui/Cartao'
import { FormCarro } from './Carros'
import { useCarregar } from './comum'
import '../estilo/ecras.css'

/** Primeira execução: convida a registar o primeiro carro. */
export function BoasVindas({ onCriado }: { onCriado(id: number): void }) {
  return (
    <div className="boas-vindas">
      <Cartao>
        <h2>Vamos registar o primeiro carro</h2>
        <p>Indique a matrícula e, se quiser, os restantes dados. Pode completar o resto mais tarde.</p>
        <FormCarro onGuardado={onCriado} />
      </Cartao>
    </div>
  )
}

/**
 * Mostra as boas-vindas quando não existe nenhum carro (nem inativos/vendidos);
 * caso contrário, o conteúdo normal do Início.
 */
export function InicioOuBoasVindas({
  ano,
  onNavegar,
  children,
}: {
  ano: number
  onNavegar(e: Ecra): void
  children: ReactNode
}) {
  const buscar = useCallback(() => carrosListar(false, ano), [ano])
  const { dados, recarregar } = useCarregar(buscar)
  if (dados === null) return null
  if (dados.length === 0) {
    return (
      <BoasVindas
        onCriado={(id) => {
          recarregar()
          onNavegar({ nome: 'carro', id })
        }}
      />
    )
  }
  return <>{children}</>
}
