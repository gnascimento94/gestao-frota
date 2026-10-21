import { useEffect, useState, type FormEvent } from 'react'
import { lancarPneu } from '../api/comandos'
import type { PneuInput, PosicaoPneu } from '../api/tipos'
import { hoje } from '../ecras/comum'
import { Campo } from '../ui/Campo'
import { parseEuros } from '../util/formato'
import {
  MSG_CARRO,
  arrumarErros,
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

/** Pela ordem da grelha: frente em cima, trás em baixo, como o carro visto de cima. */
const POSICOES: [PosicaoPneu, string][] = [
  ['FE', 'Frente esquerda'],
  ['FD', 'Frente direita'],
  ['TE', 'Trás esquerda'],
  ['TD', 'Trás direita'],
  ['suplente', 'Suplente'],
]

const MSG_POSICOES = 'Escolha pelo menos um pneu.'

const MOTIVOS = [
  ['troca', 'Troca'],
  ['rodizio', 'Rodízio'],
] as const

/** Pneus: troca ou rodízio, que pneus, marca/modelo e valor total. */
export function FormPneu({ carro, seletor, focoNoFormulario, onLancado, onCancelar }: PropsForm) {
  const [data, setData] = useState(hoje)
  const [km, setKm] = useKmSugerido(carro)
  const [posicoes, setPosicoes] = useState<ReadonlySet<PosicaoPneu>>(new Set())
  const [motivo, setMotivo] = useState<PneuInput['motivo']>('troca')
  const [marca, setMarca] = useState('')
  const [modelo, setModelo] = useState('')
  const [valor, setValor] = useState('')
  const l = useLancamento<PneuInput>(lancarPneu, onLancado)
  const { limpar } = l

  useEffect(limpar, [limpar, carro, data, km, posicoes, motivo, marca, modelo, valor])

  function validar(tentou: boolean): Erros {
    return {
      carro: tentou && !carro ? MSG_CARRO : undefined,
      data: erroData(data, tentou),
      km: erroKm(km, false, tentou),
      posicoes: tentou && posicoes.size === 0 ? MSG_POSICOES : undefined,
      valor: erroEuros(valor, true, tentou),
    }
  }

  function submeter(e: FormEvent) {
    e.preventDefault()
    l.tentar(validar, () => ({
      carroId: carro!.id,
      data,
      km: vazio(km) ? null : lerKm(km),
      posicoes: POSICOES.map(([p]) => p).filter((p) => posicoes.has(p)),
      marca: textoOuNulo(marca),
      modelo: textoOuNulo(modelo),
      motivo,
      valor: parseEuros(valor)!,
      confirmado: false,
    }))
  }

  function marcar(p: PosicaoPneu, sim: boolean) {
    setPosicoes((s) => {
      const n = new Set(s)
      if (sim) n.add(p)
      else n.delete(p)
      return n
    })
  }

  const erros: Erros = {
    ...validar(l.tentou),
    ...arrumarErros(l.erros, ['carro', 'data', 'km', 'posicoes', 'valor']),
  }
  const ajudaValor = [
    posicoes.size > 1 && `Valor total dos ${posicoes.size} pneus.`,
    motivo === 'rodizio' && 'Se o rodízio não custou nada, escreva 0.',
  ]
    .filter(Boolean)
    .join(' ')

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
        <fieldset
          className="pneus-posicoes"
          aria-invalid={erros.posicoes ? true : undefined}
          aria-describedby={erros.posicoes ? 'pneus-posicoes-erro' : undefined}
        >
          <legend className="campo-rotulo">Pneus</legend>
          {POSICOES.map(([p, nome]) => (
            <label key={p} className="pneu-opcao">
              <input
                type="checkbox"
                checked={posicoes.has(p)}
                onChange={(e) => marcar(p, e.target.checked)}
              />
              {nome}
            </label>
          ))}
          {erros.posicoes && (
            <p id="pneus-posicoes-erro" className="campo-erro" role="alert">
              {erros.posicoes}
            </p>
          )}
        </fieldset>
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
          ajuda={ajudaValor || undefined}
          autoComplete="off"
        />
      </div>
      <Rodape onCancelar={onCancelar} aEnviar={l.aEnviar} />
    </form>
  )
}
