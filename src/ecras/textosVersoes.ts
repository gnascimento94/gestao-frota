// Textos das versões (cópias em dados\versoes).

import type { Versao } from '../api/tipos'

/**
 * Ficheiro posto de lado pelo núcleo (não é uma cópia feita pelo sistema): o ficheiro de dados
 * danificado ou vazio antes de um restauro, ou alterações de uma versão anterior do programa.
 * Só serve para o rótulo e para "a cópia mais recente"; se é restaurável decide o núcleo
 * (`Versao.restauravel`).
 */
export function ePostoDeLado(nome: string): boolean {
  return /^frota_(danificado|posto_de_parte|legado)_.*\.db$/.test(nomeOriginal(nome))
}

/**
 * Uma cópia com nome chama-se `frota_guardada_<original>__<nome>.db`; devolve o nome do ficheiro
 * original (`frota_<original>.db`), que diz de onde veio a cópia. Sem nome devolve o próprio.
 */
export function nomeOriginal(nome: string): string {
  const m = /^frota_guardada_(.+?)__.+\.db$/.exec(nome)
  return m ? `frota_${m[1]}.db` : nome
}

/** Cópias que se podem restaurar (decide o núcleo, em `restauravel`). */
export function restauraveis(versoes: Versao[]): Versao[] {
  return versoes.filter((v) => v.restauravel)
}

/** Cópias feitas pelo sistema (diárias, manuais, antes da atualização), mais recente primeiro. */
export function copiasDoSistema(versoes: Versao[]): Versao[] {
  return versoes.filter((v) => !ePostoDeLado(v.nome))
}

/** Origem da cópia, pelo nome do ficheiro. */
export function tipoDaVersao(ficheiro: string): string {
  const nome = nomeOriginal(ficheiro)
  if (/^frota_danificado_.*\.db$/.test(nome)) return 'Ficheiro danificado guardado à parte'
  if (/^frota_posto_de_parte_.*\.db$/.test(nome)) return 'Ficheiro posto de parte'
  if (/^frota_legado_.*\.db$/.test(nome)) return 'Alterações por gravar de uma versão anterior do programa'
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
