import { useEffect, useState, type FormEvent } from 'react'
import { atualizarKm } from '../api/comandos'
import type { KmInput } from '../api/tipos'
import { hoje } from '../ecras/comum'
import { Campo } from '../ui/Campo'
import {
  MSG_CARRO,
  erroData,
  erroKm,
  lerKm,
  useKmSugerido,
  useLancamento,
  type Erros,
  type PropsForm,
} from './logica'
import { CampoKm, ErroGeral, Rodape } from './partes'

/** Atualizar KM: data e leitura do conta-quilómetros. */
export function FormKm({ carro, seletor, focoNoFormulario, onLancado, onCancelar }: PropsForm) {
  const [data, setData] = useState(hoje)
  const [km, setKm] = useKmSugerido(carro)
  const l = useLancamento<KmInput>(atualizarKm, onLancado)
  const { limpar } = l

  useEffect(limpar, [limpar, carro, data, km])

  function validar(tentou: boolean): Erros {
    return {
      carro: tentou && !carro ? MSG_CARRO : undefined,
      data: erroData(data, tentou),
      km: erroKm(km, true, tentou),
    }
  }

  function submeter(e: FormEvent) {
    e.preventDefault()
    l.tentar(validar, () => ({ carroId: carro!.id, data, km: lerKm(km)!, confirmado: false }))
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
      </div>
      <Rodape onCancelar={onCancelar} aEnviar={l.aEnviar} />
    </form>
  )
}
