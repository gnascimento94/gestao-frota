import { useEffect, useState, type FormEvent } from 'react'
import { condutorSugerido, guardarMulta, motoristasListar } from '../api/comandos'
import type { Infracao, Motorista, MultaInput, ResponsavelMulta } from '../api/tipos'
import { ROTULO_INFRACAO, hoje, paraErroApi } from '../ecras/comum'
import { Campo, CampoSelecao } from '../ui/Campo'
import { formatarEuros, parseEuros } from '../util/formato'
import {
  MSG_CARRO,
  arrumarErros,
  erroData,
  erroEuros,
  erroFormatoData,
  erroFuturo,
  textoOuNulo,
  useLancamento,
  vazio,
  type Erros,
  type PropsForm,
} from './logica'
import { ErroGeral, Interruptor, Rodape, Segmentado } from './partes'

const INFRACOES = Object.entries(ROTULO_INFRACAO) as [Infracao, string][]

const QUEM_PAGA = [
  ['empresa', 'Empresa'],
  ['colaborador', 'Colaborador'],
  ['coparticipado', 'Coparticipado'],
] as const

const DATA_COMPLETA = /^\d{4}-\d{2}-\d{2}$/
const MSG_COPART = 'A parte da empresa tem de ser maior que 0 e menor que o valor da multa.'

/**
 * Multa: o condutor é sugerido pela atribuição do carro na data da infração (editável ou
 * "Por identificar"); "Quem paga" reparte o valor entre a empresa e o colaborador, à vista.
 */
export function FormMulta({ carro, seletor, focoNoFormulario, onLancado, onCancelar }: PropsForm) {
  const [dataInfracao, setDataInfracao] = useState('')
  /** '' = por identificar. */
  const [condutor, setCondutor] = useState('')
  /** Sugestão do núcleo para "carroId|data": id do motorista ou null (ninguém atribuído). */
  const [sugestao, setSugestao] = useState<{ chave: string; id: number | null; erro?: string } | null>(
    null,
  )
  const [infracao, setInfracao] = useState<Infracao>('velocidade')
  const [pontos, setPontos] = useState('0')
  const [valor, setValor] = useState('')
  const [dataNotificacao, setDataNotificacao] = useState('')
  const [vencimento, setVencimento] = useState('')
  const [identificadoEm, setIdentificadoEm] = useState('')
  const [pagoEm, setPagoEm] = useState('')
  const [responsavel, setResponsavel] = useState<ResponsavelMulta>('empresa')
  const [valorEmpresa, setValorEmpresa] = useState('')
  const [adiantou, setAdiantou] = useState(false)
  const [descricao, setDescricao] = useState('')
  const [motoristas, setMotoristas] = useState<Motorista[]>([])
  const [erroCarga, setErroCarga] = useState<string | null>(null)
  const l = useLancamento<MultaInput>(guardarMulta, onLancado)
  const { limpar } = l

  useEffect(() => {
    let vivo = true
    motoristasListar(false).then(
      (ms) => vivo && setMotoristas(ms),
      (e) => vivo && setErroCarga(paraErroApi(e).mensagem),
    )
    return () => {
      vivo = false
    }
  }, [])

  // Condutor sugerido para o carro na data da infração (só vale para o par carro/data pedido).
  const carroId = carro?.id ?? null
  const chave = `${carroId}|${dataInfracao}`
  const sugestaoAtual = sugestao?.chave === chave ? sugestao : null
  const sugerido = sugestaoAtual && sugestaoAtual.erro === undefined ? sugestaoAtual.id : undefined
  const erroSugestao = sugestaoAtual?.erro

  // Carro ou data mudaram: o condutor anterior deixa de valer até chegar a nova sugestão.
  const [chaveVista, setChaveVista] = useState(chave)
  if (chave !== chaveVista) {
    setChaveVista(chave)
    setCondutor('')
  }

  useEffect(() => {
    if (carroId === null || !DATA_COMPLETA.test(dataInfracao)) return
    let vivo = true
    const pedido = `${carroId}|${dataInfracao}`
    condutorSugerido(carroId, dataInfracao).then(
      (id) => {
        if (!vivo) return
        setSugestao({ chave: pedido, id })
        setCondutor(id === null ? '' : String(id))
      },
      (e) => {
        if (!vivo) return
        setSugestao({ chave: pedido, id: null, erro: paraErroApi(e).mensagem })
        setCondutor('')
      },
    )
    return () => {
      vivo = false
    }
  }, [carroId, dataInfracao])

  useEffect(limpar, [
    limpar,
    carro,
    dataInfracao,
    condutor,
    infracao,
    pontos,
    valor,
    dataNotificacao,
    vencimento,
    identificadoEm,
    pagoEm,
    responsavel,
    valorEmpresa,
    adiantou,
    descricao,
  ])

  // Repartição (apresentação do que o utilizador escreveu; o núcleo valida de novo).
  const v = parseEuros(valor)
  const copart = responsavel === 'coparticipado'
  const empresa =
    responsavel === 'empresa' ? v : responsavel === 'colaborador' ? (v === null ? null : 0) : parseEuros(valorEmpresa)

  function erroEmpresa(tentou: boolean): string | undefined {
    if (!copart) return undefined
    if (vazio(valorEmpresa)) return tentou ? 'Escreva a parte da empresa em euros.' : undefined
    if (empresa === null) return 'Escreva a parte da empresa em euros.'
    if (v !== null && (empresa <= 0 || empresa >= v)) return MSG_COPART
    return undefined
  }

  const reparteValida = v !== null && empresa !== null && !erroEmpresa(false)
  const colaborador = reparteValida ? v - empresa : 0
  const percentagem = reparteValida && v > 0 ? Math.round((empresa * 100) / v) : 0
  const mostraAdiantou = reparteValida && colaborador > 0

  function validar(tentou: boolean): Erros {
    const pts = pontos.trim()
    return {
      carro: tentou && !carro ? MSG_CARRO : undefined,
      dataInfracao: erroData(dataInfracao, tentou, 'Indique a data da infração.'),
      pontos: /^\d{1,2}$/.test(pts) && Number(pts) <= 12 ? undefined : 'Os pontos têm de estar entre 0 e 12.',
      valor: erroEuros(valor, true, tentou),
      valorEmpresa: erroEmpresa(tentou),
      dataNotificacao: erroFuturo(dataNotificacao),
      condutorIdentificadoEm: erroFuturo(identificadoEm),
      vencimento: erroFormatoData(vencimento),
      pagoEm:
        erroFuturo(pagoEm) ??
        (!vazio(pagoEm) && !vazio(dataInfracao) && pagoEm < dataInfracao
          ? 'O pagamento não pode ser anterior à infração.'
          : undefined),
    }
  }

  function submeter(e: FormEvent) {
    e.preventDefault()
    l.tentar(validar, () => ({
      carroId: carro!.id,
      motoristaId: condutor === '' ? null : Number(condutor),
      dataInfracao,
      dataNotificacao: dataNotificacao || null,
      condutorIdentificadoEm: identificadoEm || null,
      infracao,
      pontos: Number(pontos.trim()),
      valor: v!,
      vencimento: vencimento || null,
      pagoEm: pagoEm || null,
      responsavel,
      valorEmpresa: copart ? empresa : null,
      empresaAdiantou: mostraAdiantou && adiantou,
      descricao: textoOuNulo(descricao),
    }))
  }

  // Erros do núcleo: no campo que o núcleo indica (`campo`); um erro de data sem campo vai para a
  // data da infração; o que não tem campo visível (ex.: reembolsadoEm) vai para a faixa.
  const erros: Erros = {
    ...validar(l.tentou),
    ...arrumarErros(
      l.erros,
      [
        'carro',
        'dataInfracao',
        'dataNotificacao',
        'condutorIdentificadoEm',
        'vencimento',
        'pagoEm',
        'pontos',
        'valor',
        ...(copart ? ['valorEmpresa'] : []),
      ],
      { data: 'dataInfracao' },
    ),
  }

  // Motoristas ativos, mais o escolhido/sugerido se estiver inativo.
  const opcoesCondutor = motoristas.filter(
    (mo) => mo.ativo || String(mo.id) === condutor || mo.id === sugerido,
  )

  let ajudaCondutor: string | undefined
  let tomCondutor: 'ok' | 'normal' | 'alerta' = 'normal'
  if (erroSugestao) {
    ajudaCondutor = erroSugestao
    tomCondutor = 'alerta'
  } else if (sugerido !== undefined) {
    if (sugerido !== null && condutor === String(sugerido)) {
      ajudaCondutor = '✓ Atribuído a este carro nesta data'
      tomCondutor = 'ok'
    } else if (sugerido === null) {
      ajudaCondutor = 'Este carro não tinha motorista atribuído nesta data.'
    } else {
      ajudaCondutor = 'Diferente do motorista atribuído a este carro nesta data.'
      tomCondutor = 'alerta'
    }
  }

  return (
    <form className="form" onSubmit={submeter} noValidate>
      <div className="form-campos">
        <ErroGeral texto={erros.geral ?? erroCarga} />
        {seletor(erros.carro)}
        <Campo
          rotulo="Data da infração"
          type="date"
          value={dataInfracao}
          max={hoje()}
          onChange={(e) => setDataInfracao(e.target.value)}
          erro={erros.dataInfracao}
          data-foco-inicial={focoNoFormulario || undefined}
        />
        <CampoSelecao
          rotulo="Condutor"
          value={condutor}
          onChange={(e) => setCondutor(e.target.value)}
          ajuda={ajudaCondutor}
          tomAjuda={tomCondutor}
        >
          {opcoesCondutor.map((mo) => (
            <option key={mo.id} value={mo.id}>
              {mo.nome}
            </option>
          ))}
          <option value="">Por identificar</option>
        </CampoSelecao>
        <div className="form-par">
          <CampoSelecao
            rotulo="Infração"
            value={infracao}
            onChange={(e) => setInfracao(e.target.value as Infracao)}
          >
            {INFRACOES.map(([valorInf, texto]) => (
              <option key={valorInf} value={valorInf}>
                {texto}
              </option>
            ))}
          </CampoSelecao>
          <Campo
            rotulo="Pontos retirados"
            type="number"
            inputMode="numeric"
            min={0}
            max={12}
            value={pontos}
            onChange={(e) => setPontos(e.target.value)}
            erro={erros.pontos}
          />
        </div>
        <div className="form-par">
          <Campo
            rotulo="Data da notificação"
            type="date"
            value={dataNotificacao}
            max={hoje()}
            onChange={(e) => setDataNotificacao(e.target.value)}
            erro={erros.dataNotificacao}
          />
          <Campo
            rotulo="Prazo de pagamento"
            type="date"
            value={vencimento}
            onChange={(e) => setVencimento(e.target.value)}
            erro={erros.vencimento}
          />
        </div>
        <div className="form-par">
          <Campo
            rotulo="Condutor já identificado em"
            type="date"
            value={identificadoEm}
            max={hoje()}
            onChange={(e) => setIdentificadoEm(e.target.value)}
            erro={erros.condutorIdentificadoEm}
          />
          <Campo
            rotulo="Paga em"
            type="date"
            value={pagoEm}
            max={hoje()}
            onChange={(e) => setPagoEm(e.target.value)}
            erro={erros.pagoEm}
            ajuda="Em branco = por pagar."
          />
        </div>
        <Campo
          rotulo="Valor da multa (€)"
          inputMode="decimal"
          value={valor}
          onChange={(e) => setValor(e.target.value)}
          erro={erros.valor}
          autoComplete="off"
        />

        <Segmentado rotulo="Quem paga" opcoes={QUEM_PAGA} valor={responsavel} onMudar={setResponsavel} />
        {copart && (
          <Campo
            rotulo="Parte da empresa (€)"
            inputMode="decimal"
            value={valorEmpresa}
            onChange={(e) => setValorEmpresa(e.target.value)}
            erro={erros.valorEmpresa}
            autoComplete="off"
          />
        )}
        <div className="reparte" aria-live="polite">
          <div className="barra-reparte" aria-hidden="true">
            <i style={{ width: `${percentagem}%` }} />
          </div>
          <p className="reparte-texto">
            <span>
              Empresa <b>{reparteValida ? `${formatarEuros(empresa)} (${percentagem}%)` : '—'}</b>
            </span>
            <span aria-hidden="true" className="reparte-sep">
              {' · '}
            </span>
            <span>
              Colaborador <b>{reparteValida ? formatarEuros(colaborador) : '—'}</b>
            </span>
          </p>
        </div>
        {mostraAdiantou && (
          <Interruptor
            rotulo="A empresa adiantou o pagamento"
            detalhe={`Os ${formatarEuros(colaborador)} do colaborador ficam “por reembolsar”.`}
            ligado={adiantou}
            onMudar={setAdiantou}
          />
        )}
        <Campo rotulo="Descrição (opcional)" value={descricao} onChange={(e) => setDescricao(e.target.value)} />
      </div>
      <Rodape onCancelar={onCancelar} aEnviar={l.aEnviar} />
    </form>
  )
}
