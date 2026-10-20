// Textos das versões (cópias em dados\versoes).

import type { Versao } from '../api/tipos'

/** Origem da cópia, pelo nome do ficheiro. */
export function tipoDaVersao(nome: string): string {
  const antes = /^frota_antes-da-v(.+)\.db$/.exec(nome)
  if (antes) return `Antes da atualização para a v${antes[1]}`
  if (/^frota_\d{4}-\d{2}-\d{2}_\d{4}(-\d+)?\.db$/.test(nome)) return 'Cópia manual'
  if (/^frota_\d{4}-\d{2}-\d{2}\.db$/.test(nome)) return 'Cópia diária'
  return 'Cópia'
}

const plural = (n: number, um: string, varios: string) => `${n} ${n === 1 ? um : varios}`

/** "23 carros · 412 lançamentos"; contagens ilegíveis (−1) → "—". */
export function contagensDaVersao(v: Versao): string {
  if (v.carros < 0 || v.lancamentos < 0) return '— · não foi possível ler'
  return `${plural(v.carros, 'carro', 'carros')} · ${plural(v.lancamentos, 'lançamento', 'lançamentos')}`
}
