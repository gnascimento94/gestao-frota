// Gráfico de barras empilhadas por mês (SVG próprio, sem bibliotecas).
// Recebe os valores já calculados pelo núcleo; aqui só se escala e desenha.

import { useState } from 'react'
import type { Categoria } from '../api/tipos'
import { ORDEM_CATEGORIAS, ROTULO_CATEGORIA } from '../ecras/comum'
import { formatarEuros, formatarEurosCurto, formatarMes } from '../util/formato'
import { escala } from './escala'

export interface MesGrafico {
  /** "AAAA-MM". */
  mes: string
  /** Cêntimos por categoria. */
  valores: Partial<Record<Categoria, number>>
}

export interface GraficoProps {
  meses: MesGrafico[]
  cores: Record<Categoria, string>
}

const L = 640
const A = 240
const M = { cima: 12, dir: 8, baixo: 26, esq: 64 }
const RAIO = 4
const FOLGA = 2 // espaço entre segmentos

/** Retângulo com os cantos de cima arredondados. */
function caminhoTopo(x: number, y: number, w: number, h: number, r: number): string {
  const rr = Math.min(r, w / 2, h)
  return `M${x},${y + h}V${y + rr}Q${x},${y} ${x + rr},${y}H${x + w - rr}Q${x + w},${y} ${x + w},${y + rr}V${y + h}Z`
}

function somaMes(m: MesGrafico): number {
  return ORDEM_CATEGORIAS.reduce((s, c) => s + (m.valores[c] ?? 0), 0)
}

export function Grafico({ meses, cores }: GraficoProps) {
  const [ativo, setAtivo] = useState<number | null>(null)
  const totais = meses.map(somaMes)
  const { maximo, marcas } = escala(Math.max(0, ...totais))
  const presentes = ORDEM_CATEGORIAS.filter((c) => meses.some((m) => (m.valores[c] ?? 0) > 0))

  const largura = L - M.esq - M.dir
  const altura = A - M.cima - M.baixo
  const banda = largura / Math.max(meses.length, 1)
  const barra = Math.max(4, Math.min(banda * 0.62, 40))
  const y = (v: number) => M.cima + altura - (v / maximo) * altura
  const variosAnos = new Set(meses.map((m) => m.mes.slice(0, 4))).size > 1
  // Com muitos meses, só alguns rótulos no eixo X para não se sobreporem.
  const cadaRotulo = Math.ceil(meses.length / 12)

  const mesAtivo = ativo !== null ? meses[ativo] : null

  return (
    <div className="grafico" onMouseLeave={() => setAtivo(null)}>
      <svg
        viewBox={`0 0 ${L} ${A}`}
        className="grafico-svg"
        role="img"
        aria-label="Gasto mensal por categoria"
      >
        {marcas.map((v) => (
          <g key={v} className="grafico-marca">
            <line x1={M.esq} x2={L - M.dir} y1={y(v)} y2={y(v)} className={v === 0 ? 'grafico-base' : 'grafico-grelha'} />
            <text x={M.esq - 8} y={y(v)} dy="0.32em" textAnchor="end">
              {formatarEurosCurto(v)}
            </text>
          </g>
        ))}

        {meses.map((m, i) => {
          const x = M.esq + i * banda + (banda - barra) / 2
          const cats = ORDEM_CATEGORIAS.filter((c) => (m.valores[c] ?? 0) > 0)
          let acumulado = 0
          const ano = m.mes.slice(2, 4)
          const rotulo = formatarMes(m.mes, true) + (variosAnos && (i === 0 || m.mes.endsWith('-01')) ? ` ${ano}` : '')
          return (
            <g
              key={m.mes}
              data-mes={m.mes}
              className={ativo === i ? 'grafico-mes grafico-mes-ativo' : 'grafico-mes'}
              onMouseEnter={() => setAtivo(i)}
              onFocus={() => setAtivo(i)}
              onBlur={() => setAtivo(null)}
              tabIndex={0}
              aria-label={`${formatarMes(m.mes)}: ${formatarEuros(totais[i])}`}
            >
              {/* Alvo de hover maior do que a barra (a banda inteira). */}
              <rect x={M.esq + i * banda} y={M.cima} width={banda} height={altura} className="grafico-alvo" />
              {cats.map((c, j) => {
                const v = m.valores[c] ?? 0
                const y0 = y(acumulado)
                acumulado += v
                const y1 = y(acumulado)
                const ultimo = j === cats.length - 1
                // Folga de 2px entre segmentos (sem comer segmentos muito finos).
                const h = Math.max(0.5, y0 - y1 - (j > 0 ? Math.min(FOLGA, (y0 - y1) / 2) : 0))
                return (
                  <path
                    key={c}
                    data-categoria={c}
                    d={ultimo ? caminhoTopo(x, y1, barra, h, RAIO) : `M${x},${y1}h${barra}v${h}h${-barra}Z`}
                    style={{ fill: cores[c] }}
                  />
                )
              })}
              {i % cadaRotulo === 0 && (
                <text x={M.esq + i * banda + banda / 2} y={A - 8} textAnchor="middle" className="grafico-rotulo">
                  {rotulo}
                </text>
              )}
            </g>
          )
        })}
      </svg>

      {mesAtivo && ativo !== null && (
        <div
          className="grafico-dica"
          role="tooltip"
          style={{
            left: `${((M.esq + ativo * banda + banda / 2) / L) * 100}%`,
            transform: ativo > meses.length / 2 ? 'translateX(calc(-100% - 12px))' : 'translateX(12px)',
          }}
        >
          <strong>{formatarMes(mesAtivo.mes)}</strong>
          {ORDEM_CATEGORIAS.filter((c) => (mesAtivo.valores[c] ?? 0) > 0).map((c) => (
            <span key={c} className="grafico-dica-linha">
              <i style={{ background: cores[c] }} aria-hidden="true" />
              {ROTULO_CATEGORIA[c]}
              <b>{formatarEuros(mesAtivo.valores[c] ?? 0)}</b>
            </span>
          ))}
          <span className="grafico-dica-linha grafico-dica-total">
            Total
            <b>{formatarEuros(totais[ativo])}</b>
          </span>
        </div>
      )}

      {presentes.length > 0 && (
        <ul className="grafico-legenda" aria-label="Legenda">
          {presentes.map((c) => (
            <li key={c}>
              <i style={{ background: cores[c] }} aria-hidden="true" />
              {ROTULO_CATEGORIA[c]}
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}
