import { useEffect, useState, type FormEvent } from 'react'
import { lancarDocumento } from '../api/comandos'
import type { DocumentoInput, TipoDocumento } from '../api/tipos'
import { hoje } from '../ecras/comum'
import { Campo } from '../ui/Campo'
import { parseEuros } from '../util/formato'
import {
  MSG_CARRO,
  campoDaData,
  erroData,
  erroEuros,
  erroFuturo,
  textoOuNulo,
  useLancamento,
  type Erros,
  type PropsForm,
} from './logica'
import { ErroGeral, Interruptor, Rodape, Segmentado } from './partes'

const TIPOS = [
  ['IUC', 'IUC'],
  ['inspecao', 'Inspeção'],
  ['seguro', 'Seguro'],
  ['outro', 'Outro'],
] as const

/** IUC e seguro repetem-se todos os anos; por isso vêm marcados como anuais. */
const anualPorOmissao = (t: TipoDocumento) => t === 'IUC' || t === 'seguro'

/** Documento ou taxa (IUC, inspeção, seguro, outro): valor, vencimento e, se já foi pago, quando. */
export function FormDocumento({ carro, seletor, focoNoFormulario, onLancado, onCancelar }: PropsForm) {
  const [tipo, setTipo] = useState<TipoDocumento>('IUC')
  const [descricao, setDescricao] = useState('')
  const [valor, setValor] = useState('')
  const [vencimento, setVencimento] = useState('')
  const [pagoEm, setPagoEm] = useState('')
  const [anual, setAnual] = useState(anualPorOmissao('IUC'))
  const l = useLancamento<DocumentoInput>(lancarDocumento, onLancado)
  const { limpar } = l

  useEffect(limpar, [limpar, carro, tipo, descricao, valor, vencimento, pagoEm, anual])

  function mudarTipo(t: TipoDocumento) {
    setTipo(t)
    setAnual(anualPorOmissao(t))
  }

  function validar(tentou: boolean): Erros {
    return {
      carro: tentou && !carro ? MSG_CARRO : undefined,
      valor: erroEuros(valor, true, tentou),
      vencimento: erroData(vencimento, tentou, 'Indique a data de vencimento.'),
      pagoEm: erroFuturo(pagoEm),
    }
  }

  function submeter(e: FormEvent) {
    e.preventDefault()
    l.tentar(validar, () => ({
      carroId: carro!.id,
      tipo,
      descricao: textoOuNulo(descricao),
      valor: parseEuros(valor)!,
      vencimento,
      pagoEm: pagoEm || null,
      anual,
    }))
  }

  const erros: Erros = { ...validar(l.tentou), ...l.erros }
  // Erros de data do núcleo: no campo cuja data a mensagem refere; senão no pagamento
  // (se indicado, é o único que não pode ser futuro) ou no vencimento.
  if (l.erros.data) {
    const alvo = campoDaData(l.erros.data, { vencimento, pagoEm }) ?? (pagoEm ? 'pagoEm' : 'vencimento')
    erros[alvo] = l.erros.data
  }

  return (
    <form className="form" onSubmit={submeter} noValidate>
      <div className="form-campos">
        <ErroGeral texto={erros.geral} />
        {seletor(erros.carro)}
        <Segmentado rotulo="Tipo" opcoes={TIPOS} valor={tipo} onMudar={mudarTipo} />
        <Campo
          rotulo={tipo === 'outro' ? 'Descrição' : 'Descrição (opcional)'}
          value={descricao}
          onChange={(e) => setDescricao(e.target.value)}
          placeholder={tipo === 'outro' ? 'Por exemplo, Via Verde' : undefined}
          data-foco-inicial={focoNoFormulario || undefined}
        />
        <Campo
          rotulo="Valor (€)"
          inputMode="decimal"
          value={valor}
          onChange={(e) => setValor(e.target.value)}
          erro={erros.valor}
          autoComplete="off"
        />
        <div className="form-par">
          <Campo
            rotulo="Vencimento"
            type="date"
            value={vencimento}
            onChange={(e) => setVencimento(e.target.value)}
            erro={erros.vencimento}
          />
          <Campo
            rotulo="Pago em"
            type="date"
            value={pagoEm}
            max={hoje()}
            onChange={(e) => setPagoEm(e.target.value)}
            erro={erros.pagoEm}
            ajuda="Em branco = por pagar."
          />
        </div>
        <Interruptor
          rotulo="Anual"
          detalhe="Ao marcar como pago, fica criado o do ano seguinte."
          ligado={anual}
          onMudar={setAnual}
        />
      </div>
      <Rodape onCancelar={onCancelar} aEnviar={l.aEnviar} />
    </form>
  )
}
