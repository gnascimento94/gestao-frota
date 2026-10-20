import { useEffect, useState, type FormEvent } from 'react'
import { lancarPneu } from '../api/comandos'
import type { PneuInput, PosicaoPneu } from '../api/tipos'
import { hoje } from '../ecras/comum'
import { Campo, CampoSelecao } from '../ui/Campo'
import { parseEuros } from '../util/formato'
import {
  MSG_CARRO,
  erroData,
  erroEuros,
  erroKm,
  lerKm,
  textoOuNulo,
  useKmSugerido,
  useLancamento,
  vazio,
  type Erros,
  type PropsForm,
} from './logica'
import { CampoKm, ErroGeral, Rodape, Segmentado } from './partes'

const POSICOES: [PosicaoPneu, string][] = [
  ['todos', 'Todos'],
  ['FE', 'Frente esquerda'],
  ['FD', 'Frente direita'],
  ['TE', 'Trás esquerda'],
  ['TD', 'Trás direita'],
  ['suplente', 'Suplente'],
]

const MOTIVOS = [
  ['troca', 'Troca'],
  ['rodizio', 'Rodízio'],
] as const

/** Pneus: troca ou rodízio, posição, marca/modelo e valor. */
export function FormPneu({ carro, seletor, focoNoFormulario, onLancado, onCancelar }: PropsForm) {
  const [data, setData] = useState(hoje)
  const [km, setKm] = useKmSugerido(carro)
  const [posicao, setPosicao] = useState<PosicaoPneu>('todos')
  const [motivo, setMotivo] = useState<PneuInput['motivo']>('troca')
  const [marca, setMarca] = useState('')
  const [modelo, setModelo] = useState('')
  const [valor, setValor] = useState('')
  const l = useLancamento<PneuInput>(lancarPneu, onLancado)
  const { limpar } = l

  useEffect(limpar, [limpar, carro, data, km, posicao, motivo, marca, modelo, valor])

  function validar(tentou: boolean): Erros {
    return {
      carro: tentou && !carro ? MSG_CARRO : undefined,
      data: erroData(data, tentou),
      km: erroKm(km, false, tentou),
      valor: erroEuros(valor, true, tentou),
    }
  }

  function submeter(e: FormEvent) {
    e.preventDefault()
    l.tentar(validar, () => ({
      carroId: carro!.id,
      data,
      km: vazio(km) ? null : lerKm(km),
      posicao,
      marca: textoOuNulo(marca),
      modelo: textoOuNulo(modelo),
      motivo,
      valor: parseEuros(valor)!,
      confirmado: false,
    }))
  }

  const erros = { ...validar(l.tentou), ...l.erros }

  return (
    <form className="form" onSubmit={submeter} noValidate>
      <div className="form-campos">
        <ErroGeral texto={erros.geral} />
        {seletor(erros.carro)}
        <div className="form-par">
          <Campo
            rotulo="Data"
            type="date"
            value={data}
            max={hoje()}
            onChange={(e) => setData(e.target.value)}
            erro={erros.data}
            data-foco-inicial={focoNoFormulario || undefined}
          />
          <CampoKm
            valor={km}
            onMudar={setKm}
            erro={erros.km}
            carro={carro}
            pedeConfirmacao={l.pedeConfirmacao}
            onConfirmar={l.confirmar}
            aEnviar={l.aEnviar}
          />
        </div>
        <Segmentado rotulo="Motivo" opcoes={MOTIVOS} valor={motivo} onMudar={setMotivo} />
        <CampoSelecao
          rotulo="Posição"
          value={posicao}
          onChange={(e) => setPosicao(e.target.value as PosicaoPneu)}
        >
          {POSICOES.map(([v, t]) => (
            <option key={v} value={v}>
              {t}
            </option>
          ))}
        </CampoSelecao>
        <div className="form-par">
          <Campo rotulo="Marca" value={marca} onChange={(e) => setMarca(e.target.value)} />
          <Campo rotulo="Modelo" value={modelo} onChange={(e) => setModelo(e.target.value)} />
        </div>
        <Campo
          rotulo="Valor (€)"
          inputMode="decimal"
          value={valor}
          onChange={(e) => setValor(e.target.value)}
          erro={erros.valor}
          ajuda={motivo === 'rodizio' ? 'Se o rodízio não custou nada, escreva 0.' : undefined}
          autoComplete="off"
        />
      </div>
      <Rodape onCancelar={onCancelar} aEnviar={l.aEnviar} />
    </form>
  )
}
