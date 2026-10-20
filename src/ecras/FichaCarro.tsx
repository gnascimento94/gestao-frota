import { useCallback, useState } from 'react'
import {
  apagarLancamento,
  atribuicoesListar,
  atribuir,
  carroApagar,
  carroGuardar,
  carroObter,
  historico,
  marcarReembolsado,
  motoristasListar,
  pagarDocumento,
  relatorio,
} from '../api/comandos'
import type { Carro, HistoricoItem, TipoLancamento } from '../api/tipos'
import type { Ecra } from '../App'
import { Botao } from '../ui/Botao'
import { CampoSelecao } from '../ui/Campo'
import { Cartao } from '../ui/Cartao'
import { CabecalhoFolha, Folha } from '../ui/Folha'
import { ItemLista, Lista, Pilula, Valor, type Severidade } from '../ui/Lista'
import { useDesfazer } from '../ui/useDesfazer'
import { formatarData, formatarEuros, formatarKm } from '../util/formato'
import { AcaoData } from './AcaoData'
import { FormCarro } from './Carros'
import { ROTULO_CATEGORIA, ROTULO_ESTADO, estadoDaMulta, hoje, paraErroApi, useCarregar, type EstadoMulta } from './comum'
import '../estilo/ecras.css'

type Separador = 'historico' | 'manutencao' | 'pneu' | 'documento' | 'multa' | 'motoristas' | 'km'

const SEPARADORES: [Separador, string][] = [
  ['historico', 'Histórico'],
  ['manutencao', 'Manutenções'],
  ['pneu', 'Pneus'],
  ['documento', 'Documentos'],
  ['multa', 'Multas'],
  ['motoristas', 'Motoristas'],
  ['km', 'KM'],
]

const SEVERIDADE_MULTA: Partial<Record<EstadoMulta, Severidade>> = {
  'por identificar condutor': 'vencido',
  'por pagar': 'proximo',
  'por reembolsar': 'proximo',
}

interface Dados {
  carro: Carro
  gastoAno: number
  gastoTotal: number
  porCategoria: [string, number][]
  itens: HistoricoItem[]
  atribuicoes: Awaited<ReturnType<typeof atribuicoesListar>>
  motoristas: Awaited<ReturnType<typeof motoristasListar>>
}

interface FichaProps {
  id: number
  ano: number
  onNavegar(e: Ecra): void
  /** Abre a folha "Lançar" existente. */
  onLancar(origem: DOMRect | null): void
}

export function FichaCarro({ id, ano, onNavegar, onLancar }: FichaProps) {
  const [separador, setSeparador] = useState<Separador>('historico')
  const [editar, setEditar] = useState<{ aberta: boolean; origem: DOMRect | null }>({
    aberta: false,
    origem: null,
  })
  const [avisoApagar, setAvisoApagar] = useState<string | null>(null)
  const [erroGeral, setErroGeral] = useState<string | null>(null)
  const { agendar } = useDesfazer()

  const buscar = useCallback(async (): Promise<Dados> => {
    const [carro, rAno, rTotal, itens, atribuicoes, motoristas] = await Promise.all([
      carroObter(id, ano),
      relatorio({ de: `${ano}-01-01`, ate: `${ano}-12-31`, carroIds: [id] }),
      relatorio({ de: '2000-01-01', ate: '2099-12-31', carroIds: [id] }),
      historico(id, 1000),
      atribuicoesListar(id, null),
      motoristasListar(true),
    ])
    return {
      carro,
      gastoAno: rAno.resumo.total,
      gastoTotal: rTotal.resumo.total,
      porCategoria: Object.entries(rTotal.resumo.porCategoria).filter(([, v]) => (v ?? 0) !== 0) as [
        string,
        number,
      ][],
      itens,
      atribuicoes,
      motoristas,
    }
  }, [id, ano])
  const { dados, erro, recarregar } = useCarregar(buscar)

  if (erro && !dados) {
    return (
      <p className="form-erro" role="alert">
        {erro}
      </p>
    )
  }
  if (!dados) return null
  const { carro } = dados

  async function apagar() {
    setAvisoApagar(null)
    setErroGeral(null)
    try {
      await carroApagar(id)
      onNavegar({ nome: 'carros' })
    } catch (e) {
      const { codigo, mensagem } = paraErroApi(e)
      if (codigo === 'tem_historico') setAvisoApagar(mensagem)
      else setErroGeral(mensagem)
    }
  }

  async function marcarVendido() {
    try {
      await carroGuardar({
        id: carro.id,
        matricula: carro.matricula,
        marca: carro.marca,
        modelo: carro.modelo,
        ano: carro.ano,
        cor: carro.cor,
        kmInicial: carro.kmInicial,
        dataAquisicao: carro.dataAquisicao,
        estado: 'vendido',
        dataSaida: carro.dataSaida ?? hoje(),
        notas: carro.notas,
      })
      setAvisoApagar(null)
      recarregar()
    } catch (e) {
      setErroGeral(paraErroApi(e).mensagem)
    }
  }

  function apagarItem(item: HistoricoItem) {
    agendar(`Apagado: ${item.titulo}`, async () => {
      await apagarLancamento(item.tipo as TipoLancamento, item.id)
      recarregar()
    })
  }

  const fecharEditar = () => setEditar((f) => ({ ...f, aberta: false }))
  const filtrados = separador === 'historico' ? dados.itens : dados.itens.filter((i) => i.tipo === separador)

  return (
    <>
      <div className="ficha-topo">
        <Botao variante="simples" onClick={() => onNavegar({ nome: 'carros' })}>
          ‹ Carros
        </Botao>
        <h2 className="matricula-grande">{carro.matricula}</h2>
        <div className="ficha-titulos">
          <div className="item-titulo">{[carro.marca, carro.modelo].filter(Boolean).join(' ') || 'Carro'}</div>
          <div className="ficha-sub">
            {formatarKm(carro.kmAtual)}
            {carro.estado !== 'ativo' && ` · ${ROTULO_ESTADO[carro.estado]}`}
            {carro.motoristaAtual && ` · ${carro.motoristaAtual[1]}`}
          </div>
        </div>
        <div className="ficha-acoes">
          <Botao variante="primario" onClick={(e) => onLancar(e.currentTarget.getBoundingClientRect())}>
            + Lançar
          </Botao>
          <Botao
            onClick={(e) => setEditar({ aberta: true, origem: e.currentTarget.getBoundingClientRect() })}
          >
            Editar
          </Botao>
          <Botao variante="perigo" onClick={apagar}>
            Apagar
          </Botao>
        </div>
      </div>

      {avisoApagar && (
        <div className="form-erro aviso-linha" role="alert">
          <span>{avisoApagar}</span>
          <Botao tamanho="pequeno" onClick={marcarVendido}>
            Marcar como vendido
          </Botao>
        </div>
      )}
      {erroGeral && (
        <p className="form-erro" role="alert">
          {erroGeral}
        </p>
      )}

      <div className="metricas">
        <div className="metrica">
          <div className="metrica-rotulo">Gasto total</div>
          <div className="metrica-valor">{formatarEuros(dados.gastoTotal)}</div>
        </div>
        <div className="metrica">
          <div className="metrica-rotulo">Gasto em {ano}</div>
          <div className="metrica-valor">{formatarEuros(dados.gastoAno)}</div>
        </div>
        {dados.porCategoria.map(([cat, valor]) => (
          <div className="metrica" key={cat}>
            <div className="metrica-rotulo">{ROTULO_CATEGORIA[cat] ?? cat}</div>
            <div className="metrica-valor">{formatarEuros(valor)}</div>
          </div>
        ))}
      </div>

      <div className="separadores" role="tablist" aria-label="Secções da ficha">
        {SEPARADORES.map(([chave, nome]) => (
          <button
            key={chave}
            type="button"
            role="tab"
            className="separador"
            aria-selected={separador === chave}
            onClick={() => setSeparador(chave)}
          >
            {nome}
          </button>
        ))}
      </div>

      {separador === 'motoristas' ? (
        <SeparadorMotoristas dados={dados} carroId={id} aoAtribuir={recarregar} />
      ) : (
        <Cartao semMargem>
          {filtrados.length === 0 ? (
            <p className="vazio-lista">Sem registos.</p>
          ) : (
            <Lista rotulo={SEPARADORES.find(([c]) => c === separador)?.[1]}>
              {filtrados.map((item) => (
                <LinhaItem
                  key={`${item.tipo}-${item.id}`}
                  item={item}
                  onApagar={() => apagarItem(item)}
                  onAlterado={recarregar}
                />
              ))}
            </Lista>
          )}
        </Cartao>
      )}

      <Folha aberta={editar.aberta} origem={editar.origem} onFechar={fecharEditar} rotulo="Editar carro">
        <CabecalhoFolha titulo="Editar carro" onFechar={fecharEditar} />
        <FormCarro
          carro={carro}
          onGuardado={() => {
            fecharEditar()
            recarregar()
          }}
          onCancelar={fecharEditar}
        />
      </Folha>
    </>
  )
}

function LinhaItem({
  item,
  onApagar,
  onAlterado,
}: {
  item: HistoricoItem
  onApagar(): void
  onAlterado(): void
}) {
  const multa = item.tipo === 'multa' ? estadoDaMulta(item.detalhe) : null
  const porPagar = item.tipo === 'documento' && !!item.detalhe?.includes('vence a')
  const detalheTexto = multa ? multa.resto : item.detalhe
  const partes = [formatarData(item.data), item.km != null ? formatarKm(item.km) : null, detalheTexto]
    .filter(Boolean)
    .join(' · ')

  return (
    <ItemLista
      severidade={multa?.estado ? (SEVERIDADE_MULTA[multa.estado] ?? null) : null}
      titulo={item.titulo}
      detalhe={partes}
      fim={
        <span className="item-fim">
          {multa?.estado && (
            <Pilula severidade={SEVERIDADE_MULTA[multa.estado] ?? 'proximo'}>{multa.estado}</Pilula>
          )}
          {porPagar && (
            <AcaoData
              rotulo="Marcar como pago"
              rotuloData="Pago em"
              onConfirmar={async (data) => {
                await pagarDocumento(item.id, data)
                onAlterado()
              }}
            />
          )}
          {multa?.estado === 'por reembolsar' && (
            <AcaoData
              rotulo="Marcar reembolsado"
              rotuloData="Reembolsado em"
              onConfirmar={async (data) => {
                await marcarReembolsado(item.id, data)
                onAlterado()
              }}
            />
          )}
          {item.valor != null && <Valor>{formatarEuros(item.valor)}</Valor>}
          <Botao variante="simples" tamanho="pequeno" onClick={onApagar} aria-label={`Apagar ${item.titulo}`}>
            Apagar
          </Botao>
        </span>
      }
    />
  )
}

function SeparadorMotoristas({
  dados,
  carroId,
  aoAtribuir,
}: {
  dados: Dados
  carroId: number
  aoAtribuir(): void
}) {
  const atual = dados.carro.motoristaAtual
  const [motorista, setMotorista] = useState(atual ? String(atual[0]) : '')
  const [data, setData] = useState(hoje())
  const [erro, setErro] = useState<string | null>(null)

  async function aplicar() {
    setErro(null)
    try {
      await atribuir(carroId, motorista === '' ? null : Number(motorista), data)
      aoAtribuir()
    } catch (e) {
      setErro(paraErroApi(e).mensagem)
    }
  }

  return (
    <Cartao titulo="Atribuir ou trocar motorista">
      <div className="cartao-corpo" style={{ padding: 0 }}>
        {erro && (
          <p className="form-erro" role="alert">
            {erro}
          </p>
        )}
        <div className="form-par">
          <CampoSelecao rotulo="Motorista" value={motorista} onChange={(e) => setMotorista(e.target.value)}>
            <option value="">Sem motorista</option>
            {dados.motoristas.map((m) => (
              <option key={m.id} value={m.id}>
                {m.nome}
              </option>
            ))}
          </CampoSelecao>
          <div className="campo">
            <label htmlFor="data-atribuicao">A partir de</label>
            <input
              id="data-atribuicao"
              className="campo-entrada"
              type="date"
              value={data}
              onChange={(e) => setData(e.target.value)}
            />
          </div>
        </div>
        <div>
          <Botao variante="primario" onClick={aplicar}>
            Atribuir
          </Botao>
        </div>
      </div>
      {dados.atribuicoes.length > 0 && (
        <Lista rotulo="Atribuições">
          {dados.atribuicoes.map((a) => (
            <ItemLista
              key={a.id}
              titulo={a.motoristaNome}
              detalhe={`${formatarData(a.de)} — ${a.ate ? formatarData(a.ate) : 'atual'}`}
              fim={a.ate ? undefined : <span className="pilula pilula-ok">Atual</span>}
            />
          ))}
        </Lista>
      )}
    </Cartao>
  )
}
