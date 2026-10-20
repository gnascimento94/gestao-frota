// Categorias de gasto: ordem, nomes e cores (partilhado por ecrãs e pelo gráfico).

import type { Categoria } from '../api/tipos'

/** Ordem fixa das categorias (listas, empilhamento do gráfico e legenda). */
export const ORDEM_CATEGORIAS: Categoria[] = ['manutencao', 'pneus', 'iuc', 'inspecao', 'seguro', 'multa', 'outro']

export const ROTULO_CATEGORIA: Record<Categoria, string> = {
  manutencao: 'Manutenção',
  pneus: 'Pneus',
  iuc: 'IUC',
  inspecao: 'Inspeção',
  seguro: 'Seguro',
  multa: 'Multas',
  outro: 'Outros',
}

/** Cor de cada categoria: tokens de tokens.css (com variante para o tema escuro). */
export const CORES_CATEGORIA: Record<Categoria, string> = {
  manutencao: 'var(--cat-manutencao)',
  pneus: 'var(--cat-pneus)',
  iuc: 'var(--cat-iuc)',
  inspecao: 'var(--cat-inspecao)',
  seguro: 'var(--cat-seguro)',
  multa: 'var(--cat-multa)',
  outro: 'var(--cat-outro)',
}
