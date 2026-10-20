import { useState } from 'react'
import { Botao } from '../ui/Botao'
import { hoje, paraErroApi } from './comum'

/** Botão acompanhado de uma data (por omissão hoje), p.ex. "Marcar como pago". */
export function AcaoData({
  rotulo,
  rotuloData,
  onConfirmar,
}: {
  rotulo: string
  rotuloData: string
  onConfirmar(data: string): Promise<unknown>
}) {
  const [data, setData] = useState(hoje())
  const [erro, setErro] = useState<string | null>(null)
  const [aCorrer, setACorrer] = useState(false)

  async function correr() {
    setACorrer(true)
    setErro(null)
    try {
      await onConfirmar(data)
    } catch (e) {
      setErro(paraErroApi(e).mensagem)
    } finally {
      setACorrer(false)
    }
  }

  return (
    <span className="acao-data">
      <input
        type="date"
        aria-label={rotuloData}
        value={data}
        onChange={(e) => setData(e.target.value)}
        aria-invalid={erro ? true : undefined}
      />
      <Botao tamanho="pequeno" onClick={correr} disabled={aCorrer}>
        {rotulo}
      </Botao>
      {erro && (
        <span className="campo-erro" role="alert">
          {erro}
        </span>
      )}
    </span>
  )
}
