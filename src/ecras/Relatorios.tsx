import { useCallback, useId, useMemo, useState } from 'react'
import { carrosListar, motoristasListar, relatorio } from '../api/comandos'
import type { Categoria, Infracao, Relatorio } from '../api/tipos'
import type { Ecra } from '../App'
import { CampoSelecao } from '../ui/Campo'
import { Cartao } from '../ui/Cartao'
import { Grafico } from '../ui/Grafico'
import { formatarEuros } from '../util/formato'
import {
  CORES_CATEGORIA,
  ORDEM_CATEGORIAS,
  ROTULO_CATEGORIA,
  ROTULO_INFRACAO,
  hoje,
  useCarregar,
} from './comum'
import { dataValida, mesesDoPeriodo, periodoDe, type Periodo, type Preset } from './periodos'

type Escolha = Preset | 'personalizado'

const PRESETS: [Escolha, string][] = [
  ['esteMes', 'Este mês'],
  ['mesAnterior', 'Mês anterior'],
  ['esteAno', 'Este ano'],
  ['anoAnterior', 'Ano anterior'],
  ['personalizado', 'Personalizado'],
]

interface LinhaBarra {
  chave: string
  rotulo: string
  valor: number
  /** Texto à direita (por omissão, o valor em euros). */
  texto?: string
  cor?: string
  onClick?: () => void
}

/** Barras horizontais: a largura é só a escala de apresentação dos valores vindos do núcleo. */
function Barras({ linhas, rotulo, vazio }: { linhas: LinhaBarra[]; rotulo: string; vazio: string }) {
  if (linhas.length === 0) return <p className="vazio-lista">{vazio}</p>
  const maximo = Math.max(1, ...linhas.map((l) => l.valor))
  return (
    <ul className="barras" aria-label={rotulo}>
      {linhas.map((l) => {
        const conteudo = (
          <>
            <span className="barras-rotulo">
              {l.cor && <i className="barras-amostra" style={{ background: l.cor }} aria-hidden="true" />}
              {l.rotulo}
            </span>
            <span className="barras-pista" aria-hidden="true">
              <i style={{ width: `${(Math.max(0, l.valor) / maximo) * 100}%`, background: l.cor }} />
            </span>
            <span className="barras-valor">{l.texto ?? formatarEuros(l.valor)}</span>
          </>
        )
        return (
          <li key={l.chave}>
            {l.onClick ? (
              <button type="button" className="barras-linha barras-clicavel" onClick={l.onClick}>
                {conteudo}
              </button>
            ) : (
              <div className="barras-linha">{conteudo}</div>
            )}
          </li>
        )
      })}
    </ul>
  )
}

function Metrica({ rotulo, valor, nota }: { rotulo: string; valor: string; nota?: string }) {
  return (
    <div className="metrica">
      <div className="metrica-rotulo">{rotulo}</div>
      <div className="metrica-valor">{valor}</div>
      {nota && <div className="metrica-nota">{nota}</div>}
    </div>
  )
}

/** Escolha de vários carros (vazio = todos). */
function FiltroCarros({
  carros,
  escolhidos,
  onMudar,
}: {
  carros: { id: number; matricula: string }[]
  escolhidos: number[]
  onMudar(ids: number[]): void
}) {
  const resumo =
    escolhidos.length === 0
      ? 'Todos os carros'
      : escolhidos.length === 1
        ? (carros.find((c) => c.id === escolhidos[0])?.matricula ?? '1 carro')
        : `${escolhidos.length} carros`
  return (
    <div className="campo">
      <span className="campo-rotulo-falso" id="rotulo-carros">
        Carros
      </span>
      <details className="filtro-carros">
        <summary className="campo-entrada" aria-describedby="rotulo-carros">
          {resumo}
        </summary>
        <div className="filtro-carros-lista" role="group" aria-labelledby="rotulo-carros">
          <button type="button" className="filtro-carros-todos" onClick={() => onMudar([])}>
            Todos os carros
          </button>
          {carros.map((c) => (
            <label key={c.id} className="filtro-carros-opcao">
              <input
                type="checkbox"
                checked={escolhidos.includes(c.id)}
                onChange={(e) =>
                  onMudar(e.target.checked ? [...escolhidos, c.id] : escolhidos.filter((x) => x !== c.id))
                }
              />
              {c.matricula}
            </label>
          ))}
        </div>
      </details>
    </div>
  )
}

interface RelatoriosProps {
  /** Muda após cada lançamento (folha Lançar): os dados voltam a ser pedidos. */
  versao?: number
  onNavegar(e: Ecra): void
}

export function Relatorios({ versao = 0, onNavegar }: RelatoriosProps) {
  const [escolha, setEscolha] = useState<Escolha>('esteMes')
  const [livre, setLivre] = useState<Periodo>(() => periodoDe('esteMes', hoje()))
  const [carroIds, setCarroIds] = useState<number[]>([])
  const [motoristaId, setMotoristaId] = useState<number | null>(null)
  const idTotal = useId()

  const dia = hoje()
  const periodo = useMemo(
    () => (escolha === 'personalizado' ? livre : periodoDe(escolha, dia)),
    [escolha, livre, dia],
  )
  const invalido =
    !dataValida(periodo.de) || !dataValida(periodo.ate)
      ? 'Indique as duas datas.'
      : periodo.de > periodo.ate
        ? 'A data inicial é depois da data final.'
        : null

  const listas = useCallback(async () => {
    const [carros, motoristas] = await Promise.all([
      carrosListar(false, Number(hoje().slice(0, 4))),
      motoristasListar(false),
    ])
    return { carros, motoristas }
  }, [])
  const { dados: opcoes } = useCarregar(listas, versao)

  const chaveCarros = carroIds.join(',')
  const buscar = useCallback(async (): Promise<Relatorio | null> => {
    if (invalido) return null
    return relatorio({
      de: periodo.de,
      ate: periodo.ate,
      carroIds: chaveCarros ? chaveCarros.split(',').map(Number) : null,
      motoristaId,
    })
  }, [invalido, periodo.de, periodo.ate, chaveCarros, motoristaId])
  const { dados: r, erro } = useCarregar(buscar, versao)

  const matricula = (id: number) => opcoes?.carros.find((c) => c.id === id)?.matricula ?? `Carro ${id}`
  const meses = mesesDoPeriodo(periodo.de, periodo.ate).map((mes) => ({
    mes,
    valores: r?.resumo.porMes[mes] ?? {},
  }))

  function escolher(e: Escolha) {
    if (e === 'personalizado' && escolha !== 'personalizado') setLivre(periodo)
    setEscolha(e)
  }

  const categorias: LinhaBarra[] = r
    ? ORDEM_CATEGORIAS.filter((c) => (r.resumo.porCategoria[c] ?? 0) !== 0).map((c: Categoria) => ({
        chave: c,
        rotulo: ROTULO_CATEGORIA[c],
        valor: r.resumo.porCategoria[c] ?? 0,
        cor: CORES_CATEGORIA[c],
      }))
    : []
  const mt = r?.multas
  const mn = r?.manutencao

  return (
    <>
      <div className="relatorio-filtros">
        <div className="separadores" role="group" aria-label="Período">
          {PRESETS.map(([chave, nome]) => (
            <button
              key={chave}
              type="button"
              className="separador"
              aria-pressed={escolha === chave}
              onClick={() => escolher(chave)}
            >
              {nome}
            </button>
          ))}
        </div>
        <div className="relatorio-campos">
          {escolha === 'personalizado' && (
            <>
              <div className="campo">
                <label htmlFor="rel-de">De</label>
                <input
                  id="rel-de"
                  className="campo-entrada"
                  type="date"
                  value={livre.de}
                  onChange={(e) => setLivre((p) => ({ ...p, de: e.target.value }))}
                />
              </div>
              <div className="campo">
                <label htmlFor="rel-ate">Até</label>
                <input
                  id="rel-ate"
                  className="campo-entrada"
                  type="date"
                  value={livre.ate}
                  onChange={(e) => setLivre((p) => ({ ...p, ate: e.target.value }))}
                />
              </div>
            </>
          )}
          <FiltroCarros carros={opcoes?.carros ?? []} escolhidos={carroIds} onMudar={setCarroIds} />
          <CampoSelecao
            rotulo="Motorista"
            value={motoristaId ?? ''}
            onChange={(e) => setMotoristaId(e.target.value === '' ? null : Number(e.target.value))}
          >
            <option value="">Todos os motoristas</option>
            {(opcoes?.motoristas ?? []).map((m) => (
              <option key={m.id} value={m.id}>
                {m.nome}
              </option>
            ))}
          </CampoSelecao>
        </div>
      </div>

      {(invalido || erro) && (
        <p className="form-erro" role="alert">
          {invalido ?? erro}
        </p>
      )}

      {r && !invalido && (
        <>
          <Cartao titulo="Gasto no período">
            <div className="relatorio-corpo">
              <dl className="relatorio-totais">
                <div>
                  <dt id={idTotal}>Total no período</dt>
                  <dd className="relatorio-total" aria-labelledby={idTotal}>
                    {formatarEuros(r.resumo.total)}
                  </dd>
                </div>
                <div>
                  <dt>Por pagar (hoje)</dt>
                  <dd className="relatorio-por-pagar">{formatarEuros(r.resumo.porPagar)}</dd>
                  <dd className="metrica-nota">Dívida atual; não depende do período.</dd>
                </div>
              </dl>
              <Barras linhas={categorias} rotulo="Gasto por categoria" vazio="Sem gastos no período." />
            </div>
          </Cartao>

          <Cartao titulo="Gasto mensal">
            <div className="relatorio-corpo">
              <Grafico meses={meses} cores={CORES_CATEGORIA} />
            </div>
          </Cartao>

          <div className="relatorio-grelha">
            <Cartao titulo="Carros por gasto">
              <Barras
                rotulo="Carros por gasto"
                vazio="Sem gastos no período."
                linhas={r.resumo.porCarro.map(([id, v]) => ({
                  chave: String(id),
                  rotulo: matricula(id),
                  valor: v,
                  onClick: () => onNavegar({ nome: 'carro', id }),
                }))}
              />
            </Cartao>
            <Cartao titulo="Gasto por motorista">
              <Barras
                rotulo="Gasto por motorista"
                vazio="Sem gastos no período."
                linhas={r.porMotorista.map(([id, nome, v]) => ({ chave: String(id ?? 'sem'), rotulo: nome, valor: v }))}
              />
            </Cartao>
          </div>

          <div className="relatorio-grelha">
            <Cartao titulo="Tipos de manutenção mais caros">
              <Barras
                rotulo="Tipos de manutenção mais caros"
                vazio="Sem manutenções no período."
                linhas={r.porTipoManutencao.map(([tipo, v]) => ({ chave: tipo, rotulo: tipo, valor: v }))}
              />
            </Cartao>

            {mn && (
              <Cartao titulo="Manutenção">
                <div className="relatorio-corpo">
                  <div className="metricas">
                    <Metrica rotulo="Feita por nós" valor={formatarEuros(mn.interna)} />
                    <Metrica rotulo="Oficina" valor={formatarEuros(mn.oficina)} />
                  </div>
                  <div className="metricas">
                    <Metrica rotulo="Peças" valor={formatarEuros(mn.pecas)} />
                    <Metrica rotulo="Mão de obra" valor={formatarEuros(mn.maoObra)} />
                    <Metrica rotulo="Não discriminado" valor={formatarEuros(mn.naoDiscriminado)} />
                  </div>
                  <h3 className="relatorio-sub">Por oficina</h3>
                  <Barras
                    rotulo="Gasto por oficina"
                    vazio="Sem manutenções em oficina no período."
                    linhas={mn.porOficina.map(([of, v]) => ({ chave: of, rotulo: of || 'Sem nome', valor: v }))}
                  />
                </div>
              </Cartao>
            )}
          </div>

          {mt && (
            <Cartao titulo="Infrações no período">
              <div className="relatorio-corpo">
                <p className="relatorio-legenda">
                  Contadas pela data da infração; o gasto acima conta pela data de pagamento.
                </p>
                <div className="metricas">
                  <Metrica rotulo="N.º de multas" valor={String(mt.n)} />
                  <Metrica rotulo="Total" valor={formatarEuros(mt.total)} />
                  <Metrica rotulo="Parte da empresa" valor={formatarEuros(mt.empresa)} />
                  <Metrica rotulo="Parte dos colaboradores" valor={formatarEuros(mt.colaboradores)} />
                  <Metrica rotulo="Por reembolsar" valor={formatarEuros(mt.porReembolsar)} />
                </div>
                <div className="relatorio-grelha">
                  <div>
                    <h3 className="relatorio-sub">Por infração</h3>
                    <Barras
                      rotulo="Multas por infração"
                      vazio="Sem multas no período."
                      linhas={mt.porInfracao.map(([inf, n, total]) => ({
                        chave: inf,
                        rotulo: `${ROTULO_INFRACAO[inf as Infracao] ?? inf} (${n})`,
                        valor: total,
                      }))}
                    />
                  </div>
                  <div>
                    <h3 className="relatorio-sub">Por motorista</h3>
                    <Barras
                      rotulo="Multas por motorista"
                      vazio="Sem multas no período."
                      linhas={mt.porMotorista.map(([id, nome, n, pontos]) => ({
                        chave: String(id ?? 'sem'),
                        rotulo: nome,
                        valor: n,
                        texto: `${n} ${n === 1 ? 'multa' : 'multas'} · ${pontos} ${pontos === 1 ? 'ponto' : 'pontos'}`,
                      }))}
                    />
                  </div>
                </div>
              </div>
            </Cartao>
          )}
        </>
      )}
    </>
  )
}
