// Formatação e leitura de valores em pt-PT.
// Dinheiro é sempre em cêntimos (inteiros) — nunca em números com vírgula flutuante.

/** Espaço fino inseparável (U+202F), separador de milhares. */
export const ESPACO_FINO = ' '

const MESES = ['jan', 'fev', 'mar', 'abr', 'mai', 'jun', 'jul', 'ago', 'set', 'out', 'nov', 'dez']

function agrupar(inteiro: number): string {
  return String(inteiro).replace(/\B(?=(\d{3})+(?!\d))/g, ESPACO_FINO)
}

function centimos(euros: string, decimais: string | undefined): number | null {
  const c = Number(euros) * 100 + (decimais ? Number(decimais.padEnd(2, '0')) : 0)
  return Number.isSafeInteger(c) ? c : null
}

/**
 * Lê um valor em euros escrito pelo utilizador e devolve cêntimos, ou `null` se não for válido.
 *
 * Aceita `1.234,56`, `1234,56`, `1 234,56`, `45,5`, `45`, `45.5`, `45 €`.
 * O ponto só é separador de milhares em grupos de 3 quando há vírgula decimal (`1.234,56`)
 * ou quando não há ambiguidade (`1.234.567`); um único ponto é decimal (`45.5` = 45,50).
 * Rejeita vazio, negativos, letras e mais de 2 casas decimais (`1,234`, `1.234`).
 */
export function parseEuros(texto: string): number | null {
  const t = texto.replace(/€/g, '').replace(/[\s  ]/g, '')
  if (t === '') return null

  // Vírgula decimal, com ou sem milhares em ponto.
  let m = /^(\d{1,3}(?:\.\d{3})+|\d+),(\d{1,2})$/.exec(t)
  if (m) return centimos(m[1].replace(/\./g, ''), m[2])

  // Milhares em ponto sem ambiguidade (2+ grupos), sem decimais.
  if (/^\d{1,3}(?:\.\d{3}){2,}$/.test(t)) return centimos(t.replace(/\./g, ''), undefined)

  // Inteiro, ou um único ponto decimal.
  m = /^(\d+)(?:\.(\d{1,2}))?$/.exec(t)
  if (m) return centimos(m[1], m[2])

  return null
}

/** `123456` → `"1 234,56 €"` (milhares com espaço fino, espaço normal antes de €). */
export function formatarEuros(c: number): string {
  const n = Math.abs(Math.round(c))
  const sinal = c < 0 && n !== 0 ? '-' : ''
  return `${sinal}${agrupar(Math.floor(n / 100))},${String(n % 100).padStart(2, '0')} €`
}

/** `70000` → `"70 000 km"`. */
export function formatarKm(n: number): string {
  return `${agrupar(Math.round(n))} km`
}

/** `"2026-10-20"` (ou com hora) → `"20 out 2026"`. Texto inválido é devolvido tal como está. */
export function formatarData(iso: string): string {
  const m = /^(\d{4})-(\d{2})-(\d{2})/.exec(iso)
  if (!m) return iso
  const mes = MESES[Number(m[2]) - 1]
  if (!mes) return iso
  return `${Number(m[3])} ${mes} ${m[1]}`
}

/** `"2026-10-20T18:42:07"` → `"18:42"`; `null` se não houver hora. */
export function formatarHora(iso: string): string | null {
  const m = /^\d{4}-\d{2}-\d{2}[T ](\d{2}):(\d{2})/.exec(iso)
  return m ? `${m[1]}:${m[2]}` : null
}
