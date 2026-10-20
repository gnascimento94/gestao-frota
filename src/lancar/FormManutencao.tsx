import { useEffect, useId, useState, type FormEvent } from 'react'
import { lancarManutencao, oficinasUsadas, tiposListar } from '../api/comandos'
import type { ManutencaoInput, TipoManutencao } from '../api/tipos'
import { hoje, paraErroApi } from '../ecras/comum'
import { Campo, CampoSelecao } from '../ui/Campo'
import { formatarEuros, parseEuros } from '../util/formato'
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
import { CampoKm, ErroGeral, Interruptor, Rodape, Segmentado } from './partes'

type Por = ManutencaoInput['executadaPor']

const FEITA_POR = [
  ['propria', 'Nós'],
  ['oficina', 'Oficina'],
] as const

/**
 * Manutenção. "Nós": só peças e materiais (mão de obra própria = 0).
 * "Oficina": nome, peças + mão de obra (total à vista) ou, com "Só tenho o total", um único valor.
 */
export function FormManutencao({ carro, seletor, focoNoFormulario, onLancado, onCancelar }: PropsForm) {
  const idOficinas = useId()
  const [data, setData] = useState(hoje)
  const [tipoId, setTipoId] = useState('')
  const [km, setKm] = useKmSugerido(carro)
  const [por, setPor] = useState<Por>('propria')
  const [oficina, setOficina] = useState('')
  const [pecas, setPecas] = useState('')
  const [maoObra, setMaoObra] = useState('')
  const [soTotal, setSoTotal] = useState(false)
  const [total, setTotal] = useState('')
  const [descricao, setDescricao] = useState('')
  const [tipos, setTipos] = useState<TipoManutencao[]>([])
  const [oficinas, setOficinas] = useState<string[]>([])
  const [erroCarga, setErroCarga] = useState<string | null>(null)
  const l = useLancamento<ManutencaoInput>(lancarManutencao, onLancado)
  const { limpar } = l

  useEffect(() => {
    let vivo = true
    tiposListar().then(
      (t) => vivo && setTipos(t.filter((x) => x.ativo)),
      (e) => vivo && setErroCarga(paraErroApi(e).mensagem),
    )
    // As sugestões de oficinas são só uma ajuda: se falharem, o campo continua a funcionar.
    oficinasUsadas().then(
      (o) => vivo && setOficinas(o),
      () => {},
    )
    return () => {
      vivo = false
    }
  }, [])

  useEffect(limpar, [limpar, carro, data, tipoId, km, por, oficina, pecas, maoObra, soTotal, total, descricao])

  const naOficina = por === 'oficina'
  const discriminado = naOficina && !soTotal
  const p = parseEuros(pecas)
  const m = parseEuros(maoObra)

  function validar(tentou: boolean): Erros {
    return {
      carro: tentou && !carro ? MSG_CARRO : undefined,
      data: erroData(data, tentou),
      tipo: tentou && !tipoId ? 'Escolha o tipo de manutenção.' : undefined,
      km: erroKm(km, false, tentou),
      oficina: naOficina && tentou && vazio(oficina) ? 'Indique o nome da oficina.' : undefined,
      // "Nós" usa o campo das peças como valor; "Só tenho o total" usa o campo do total.
      valor: !naOficina
        ? erroEuros(pecas, true, tentou)
        : soTotal
          ? erroEuros(total, true, tentou)
          : undefined,
      pecas: discriminado ? erroEuros(pecas, true, tentou) : undefined,
      maoObra: discriminado ? erroEuros(maoObra, true, tentou) : undefined,
    }
  }

  function valores(): Pick<ManutencaoInput, 'valor' | 'valorPecas' | 'valorMaoObra'> {
    if (!naOficina) return { valor: p!, valorPecas: p!, valorMaoObra: 0 }
    if (soTotal) return { valor: parseEuros(total)!, valorPecas: null, valorMaoObra: null }
    return { valor: p! + m!, valorPecas: p!, valorMaoObra: m! }
  }

  function submeter(e: FormEvent) {
    e.preventDefault()
    l.tentar(validar, () => ({
      carroId: carro!.id,
      data,
      km: vazio(km) ? null : lerKm(km),
      tipoId: Number(tipoId),
      executadaPor: por,
      oficina: naOficina ? oficina.trim() : null,
      ...valores(),
      descricao: textoOuNulo(descricao),
      confirmado: false,
    }))
  }

  const erros = { ...validar(l.tentou), ...l.erros }

  return (
    <form className="form" onSubmit={submeter} noValidate>
      <div className="form-campos">
        <ErroGeral texto={erros.geral ?? erroCarga} />
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
          <CampoSelecao
            rotulo="Tipo"
            value={tipoId}
            onChange={(e) => setTipoId(e.target.value)}
            erro={erros.tipo}
          >
            <option value="" disabled>
              Escolha…
            </option>
            {tipos.map((t) => (
              <option key={t.id} value={t.id}>
                {t.nome}
              </option>
            ))}
          </CampoSelecao>
        </div>
        <CampoKm
          valor={km}
          onMudar={setKm}
          erro={erros.km}
          carro={carro}
          pedeConfirmacao={l.pedeConfirmacao}
          onConfirmar={l.confirmar}
          aEnviar={l.aEnviar}
        />
        <Segmentado rotulo="Feita por" opcoes={FEITA_POR} valor={por} onMudar={setPor} />

        {!naOficina && (
          <Campo
            rotulo="Peças e materiais (€)"
            inputMode="decimal"
            value={pecas}
            onChange={(e) => setPecas(e.target.value)}
            erro={erros.valor}
            ajuda="Mão de obra própria: 0 €"
            autoComplete="off"
          />
        )}

        {naOficina && (
          <>
            <Campo
              rotulo="Oficina"
              value={oficina}
              onChange={(e) => setOficina(e.target.value)}
              list={idOficinas}
              erro={erros.oficina}
              autoComplete="off"
            />
            <datalist id={idOficinas}>
              {oficinas.map((o) => (
                <option key={o} value={o} />
              ))}
            </datalist>
            <Interruptor rotulo="Só tenho o total" ligado={soTotal} onMudar={setSoTotal} />
            {soTotal ? (
              <Campo
                rotulo="Total (€)"
                inputMode="decimal"
                value={total}
                onChange={(e) => setTotal(e.target.value)}
                erro={erros.valor}
                autoComplete="off"
              />
            ) : (
              <>
                <div className="form-par">
                  <Campo
                    rotulo="Peças (€)"
                    inputMode="decimal"
                    value={pecas}
                    onChange={(e) => setPecas(e.target.value)}
                    erro={erros.pecas}
                    autoComplete="off"
                  />
                  <Campo
                    rotulo="Mão de obra (€)"
                    inputMode="decimal"
                    value={maoObra}
                    onChange={(e) => setMaoObra(e.target.value)}
                    erro={erros.maoObra}
                    autoComplete="off"
                  />
                </div>
                <div className="reparte" aria-live="polite">
                  <div className="reparte-linha">
                    <span>Total</span>
                    <b>{p !== null && m !== null ? formatarEuros(p + m) : '—'}</b>
                  </div>
                  {erros.valor && <span className="campo-erro">{erros.valor}</span>}
                </div>
              </>
            )}
          </>
        )}

        <Campo rotulo="Descrição (opcional)" value={descricao} onChange={(e) => setDescricao(e.target.value)} />
      </div>
      <Rodape onCancelar={onCancelar} aEnviar={l.aEnviar} />
    </form>
  )
}
