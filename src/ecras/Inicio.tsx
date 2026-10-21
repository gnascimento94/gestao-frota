import { useCallback, type ReactNode } from 'react'
import { carrosListar, painel } from '../api/comandos'
import type { Alerta, HistoricoItem } from '../api/tipos'
import type { Ecra } from '../App'
import { Cartao } from '../ui/Cartao'
import { ItemLista, Lista, Pilula, Valor } from '../ui/Lista'
import { formatarData, formatarEuros, formatarKm, nomeMes } from '../util/formato'
import { textoAlerta } from './alertas'
import { textoPorPagar, useCarregar } from './comum'

interface InicioProps {
  /** Ano pedido à lista de carros (só para contar inativos e vendidos). */
  ano: number
  /** Muda após cada lançamento (folha Lançar): os dados voltam a ser pedidos. */
  versao?: number
  onNavegar(e: Ecra): void
}

/** Valor em euros com o símbolo mais pequeno, como na maquete. */
function Euros({ centimos }: { centimos: number }) {
  return (
    <>
      {formatarEuros(centimos).replace(/ €$/, '')}
      <small> €</small>
    </>
  )
}

function Kpi({
  rotulo,
  detalhe,
  aviso,
  children,
}: {
  rotulo: string
  detalhe?: string
  aviso?: boolean
  children: ReactNode
}) {
  return (
    <div className={aviso ? 'kpi kpi-aviso' : 'kpi'}>
      <div className="kpi-rotulo">{rotulo}</div>
      <div className="kpi-valor">{children}</div>
      {detalhe && <div className="kpi-detalhe">{detalhe}</div>}
    </div>
  )
}

/** Destino de um alerta: ficha do carro ou ficha do motorista. */
function destino(a: Alerta): Ecra | null {
  if (a.carroId !== null) return { nome: 'carro', id: a.carroId }
  if (a.motoristaId !== null) return { nome: 'motoristas', id: a.motoristaId }
  return null
}

function detalheRecente(i: HistoricoItem): string {
  return [i.matricula, formatarData(i.data), i.km !== null ? formatarKm(i.km) : null, i.detalhe]
    .filter(Boolean)
    .join(' · ')
}

function plural(n: number, um: string, varios: string): string | null {
  return n > 0 ? `${n} ${n === 1 ? um : varios}` : null
}

export function Inicio({ ano, versao = 0, onNavegar }: InicioProps) {
  const buscar = useCallback(async () => {
    const [p, carros] = await Promise.all([painel(), carrosListar(false, ano)])
    return { p, carros }
  }, [ano])
  const { dados, erro } = useCarregar(buscar, versao)

  if (erro && !dados) {
    return (
      <p className="form-erro" role="alert">
        {erro}
      </p>
    )
  }
  if (!dados) return null
  const { p, carros } = dados
  const outros = [
    plural(carros.filter((c) => c.estado === 'inativo').length, 'inativo', 'inativos'),
    plural(carros.filter((c) => c.estado === 'vendido').length, 'vendido', 'vendidos'),
  ]
    .filter(Boolean)
    .join(' · ')

  return (
    <>
      {erro && (
        <p className="form-erro" role="alert">
          {erro}
        </p>
      )}
      <section className="kpis" aria-label="Resumo">
        <Kpi rotulo="Gasto este mês" detalhe={nomeMes(p.mes)}>
          <Euros centimos={p.gastoMes} />
        </Kpi>
        <Kpi rotulo="Gasto no ano" detalhe={String(p.ano)}>
          <Euros centimos={p.gastoAno} />
        </Kpi>
        <Kpi
          rotulo="Por pagar (hoje)"
          detalhe={textoPorPagar(p.porPagarVencido, p.porPagarProximo, p.porPagarDias)}
          aviso={p.porPagar > 0}
        >
          <Euros centimos={p.porPagar} />
        </Kpi>
        <Kpi rotulo="Carros ativos" detalhe={outros || undefined}>
          {p.carrosAtivos}
        </Kpi>
      </section>

      <div className="inicio-grelha">
        <Cartao titulo="Próximos vencimentos" contagem={p.alertas.length} semMargem>
          {p.alertas.length === 0 ? (
            <p className="vazio-lista">Nada a assinalar. Está tudo em dia.</p>
          ) : (
            <Lista rotulo="Alertas">
              {p.alertas.map((a) => {
                const ir = destino(a)
                return (
                  <ItemLista
                    key={`${a.tipo}-${a.refId}-${a.carroId ?? a.motoristaId}`}
                    severidade={a.nivel}
                    titulo={textoAlerta(a, p.ano)}
                    fim={<Pilula severidade={a.nivel}>{a.nivel === 'vencido' ? 'Vencido' : 'Próximo'}</Pilula>}
                    onClick={ir ? () => onNavegar(ir) : undefined}
                  />
                )
              })}
            </Lista>
          )}
        </Cartao>

        <Cartao titulo="Últimos lançamentos" semMargem>
          {p.recentes.length === 0 ? (
            <p className="vazio-lista">Ainda não há lançamentos.</p>
          ) : (
            <Lista rotulo="Últimos lançamentos">
              {p.recentes.map((i) => (
                <ItemLista
                  key={`${i.tipo}-${i.id}`}
                  titulo={i.titulo}
                  detalhe={detalheRecente(i)}
                  fim={i.valor !== null ? <Valor>{formatarEuros(i.valor)}</Valor> : <Valor discreto>—</Valor>}
                  onClick={() => onNavegar({ nome: 'carro', id: i.carroId })}
                />
              ))}
            </Lista>
          )}
        </Cartao>
      </div>
    </>
  )
}
