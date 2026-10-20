// Textos dos planos de manutenção (ecrã Planos).

import type { Plano } from '../api/tipos'
import { formatarKm } from '../util/formato'

/** Sugestão quando ainda não há planos (pré-preenche o formulário). */
export const SUGESTAO = { tipoNome: 'Mudança de óleo', cadaKm: 15000, cadaMeses: 12 } as const

/** "a cada 15 000 km ou 12 meses". */
export function textoIntervalo(cadaKm: number | null, cadaMeses: number | null): string {
  const partes: string[] = []
  if (cadaKm !== null) partes.push(formatarKm(cadaKm))
  if (cadaMeses !== null) partes.push(cadaMeses === 1 ? '1 mês' : `${cadaMeses} meses`)
  return `a cada ${partes.join(' ou ')}`
}

/** "todos os carros", as matrículas (até 2) ou "N carros". */
export function textoAlvo(p: Pick<Plano, 'todosCarros' | 'carros'>, matriculas: ReadonlyMap<number, string>): string {
  if (p.todosCarros) return 'todos os carros'
  if (p.carros.length > 2) return `${p.carros.length} carros`
  return p.carros.map((id) => matriculas.get(id) ?? `carro n.º ${id}`).join(', ')
}

/** "Mudança de óleo · a cada 15 000 km ou 12 meses · todos os carros". */
export function textoPlano(p: Plano, matriculas: ReadonlyMap<number, string>): string {
  return `${p.tipoNome} · ${textoIntervalo(p.cadaKm, p.cadaMeses)} · ${textoAlvo(p, matriculas)}`
}
