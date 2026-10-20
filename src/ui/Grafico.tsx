// Gráfico de barras empilhadas por mês (SVG próprio, sem bibliotecas).
// Valores e totais vêm do núcleo; aqui só se escala e desenha.

import { useRef, useState, type KeyboardEvent } from 'react'
import type { Categoria } from '../api/tipos'
import { ORDEM_CATEGORIAS, ROTULO_CATEGORIA } from '../util/categorias'
import { formatarEuros, formatarEurosCurto, formatarMes } from '../util/formato'
import { escala } from './escala'

export interface MesGrafico {
  /** "AAAA-MM". */
  mes: string
  /** Cêntimos por categoria. */
  valores: Partial<Record<Categoria, number>>
  /** Total do mês (cêntimos), enviado pelo núcleo. */
  total: number
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

/** Texto acessível de um mês: "março 2026: 70,00 € (Manutenção 20,00 €, Pneus 50,00 €)". */
function descreverMes(m: MesGrafico): string {
  const partes = ORDEM_CATEGORIAS.filter((c) => (m.valores[c] ?? 0) > 0).map(
    (c) => `${ROTULO_CATEGORIA[c]} ${formatarEuros(m.valores[c] ?? 0)}`,
  )
  return `${formatarMes(m.mes)}: ${formatarEuros(m.total)}${partes.length ? ` (${partes.join(', ')})` : ''}`
}

export function Grafico({ meses, cores }: GraficoProps) {
  const [ativo, setAtivo] = useState<number | null>(null)
  // Navegação por teclado: um só ponto de tabulação (roving tabindex), setas mudam de mês.
  const [foco, setFoco] = useState(0)
  const refs = useRef<(SVGGElement | null)[]>([])
  const focavel = Math.min(foco, Math.max(meses.length - 1, 0))

  const { maximo, marcas } = escala(Math.max(0, ...meses.map((m) => m.total)))
  const presentes = ORDEM_CATEGORIAS.filter((c) => meses.some((m) => (m.valores[c] ?? 0) > 0))

  const largura = L - M.esq - M.dir
  const altura = A - M.cima - M.baixo
  const banda = largura / Math.max(meses.length, 1)
  const barra = Math.max(4, Math.min(banda * 0.62, 40))
  const y = (v: number) => M.cima + altura - (Math.min(v, maximo) / maximo) * altura
  const variosAnos = new Set(meses.map((m) => m.mes.slice(0, 4))).size > 1
  // Com muitos meses, só alguns rótulos no eixo X para não se sobreporem.
  const cadaRotulo = Math.ceil(meses.length / 12)

  function tecla(e: KeyboardEvent, i: number) {
    const destino =
      e.key === 'ArrowRight' || e.key === 'ArrowDown'
        ? Math.min(i + 1, meses.length - 1)
        : e.key === 'ArrowLeft' || e.key === 'ArrowUp'
          ? Math.max(i - 1, 0)
          : e.key === 'Home'
            ? 0
            : e.key === 'End'
              ? meses.length - 1
              : null
    if (destino === null) return
    e.preventDefault()
    setFoco(destino)
    setAtivo(destino)
    refs.current[destino]?.focus()
  }

  const mesAtivo = ativo !== null ? meses[ativo] : null

  return (
    <div className="grafico" onMouseLeave={() => setAtivo(null)}>
      <svg viewBox={`0 0 ${L} ${A}`} className="grafico-svg" role="group" aria-label="Gasto mensal por categoria">
        <g aria-hidden="true">
          {marcas.map((v) => (
            <g key={v}>
              <line
                x1={M.esq}
                x2={L - M.dir}
                y1={y(v)}
                y2={y(v)}
                className={v === 0 ? 'grafico-base' : 'grafico-grelha'}
              />
              <text x={M.esq - 8} y={y(v)} dy="0.32em" textAnchor="end">
                {formatarEurosCurto(v)}
              </text>
            </g>
          ))}
        </g>

        {meses.map((m, i) => {
          const x = M.esq + i * banda + (banda - barra) / 2
          const cats = ORDEM_CATEGORIAS.filter((c) => (m.valores[c] ?? 0) > 0)
          let acumulado = 0
          const rotulo =
            formatarMes(m.mes, true) + (variosAnos && (i === 0 || m.mes.endsWith('-01')) ? ` ${m.mes.slice(2, 4)}` : '')
          return (
            <g
              key={m.mes}
              ref={(el) => {
                refs.current[i] = el
              }}
              data-mes={m.mes}
              className={ativo === i ? 'grafico-mes grafico-mes-ativo' : 'grafico-mes'}
              role="img"
              aria-label={descreverMes(m)}
              tabIndex={i === focavel ? 0 : -1}
              onMouseEnter={() => setAtivo(i)}
              onFocus={() => {
                setFoco(i)
                setAtivo(i)
              }}
              onBlur={() => setAtivo(null)}
              onKeyDown={(e) => tecla(e, i)}
            >
              {/* Alvo de hover maior do que a barra (a banda inteira). */}
              <rect x={M.esq + i * banda} y={M.cima} width={banda} height={altura} className="grafico-alvo" />
              {cats.map((c, j) => {
                const y0 = y(acumulado)
                acumulado += m.valores[c] ?? 0
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
                <text
                  x={M.esq + i * banda + banda / 2}
                  y={A - 8}
                  textAnchor="middle"
                  className="grafico-rotulo"
                  aria-hidden="true"
                >
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
            <b>{formatarEuros(mesAtivo.total)}</b>
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
