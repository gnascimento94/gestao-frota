// Texto dos alertas do Início. Nível, distâncias, dimensão (km/dias), datas, matrícula e nome
// vêm todos do núcleo; aqui só se escolhe a frase e se formata.

import type { Alerta } from '../api/tipos'
import { formatarDiaMes, formatarKm } from '../util/formato'

function faltaKm(k: number): string {
  if (k > 0) return k === 1 ? 'falta 1 km' : `faltam ${formatarKm(k)}`
  if (k === 0) return 'atingiu os km'
  return `passou ${formatarKm(-k)}`
}

function faltaDias(d: number): string {
  if (d > 1) return `faltam ${d} dias`
  if (d === 1) return 'falta 1 dia'
  if (d === 0) return 'vence hoje'
  return d === -1 ? 'passou 1 dia' : `passaram ${-d} dias`
}

function distanciaManutencao(a: Alerta): string | null {
  if (a.motivo === 'km' && a.faltaKm !== null) return faltaKm(a.faltaKm)
  if (a.motivo === 'dias' && a.faltaDias !== null) return faltaDias(a.faltaDias)
  return null
}

/** Frase do alerta, p.ex. `"AA-12-BB · Mudança de óleo · faltam 420 km"`. `anoAtual` vem do painel. */
export function textoAlerta(a: Alerta, anoAtual: number): string {
  const dia = a.data ? formatarDiaMes(a.data, anoAtual) : null
  const passou = (a.faltaDias ?? 0) < 0
  const juntar = (...partes: (string | null)[]) => partes.filter(Boolean).join(' · ')
  switch (a.tipo) {
    case 'manutencao':
      return juntar(a.matricula, a.titulo, distanciaManutencao(a))
    case 'documento':
    case 'multa': {
      const prazo = a.faltaDias === 0 ? 'vence hoje' : dia && `${passou ? 'venceu' : 'vence'} a ${dia}`
      return juntar(a.matricula, a.titulo, prazo)
    }
    case 'identificarCondutor':
      return juntar(a.matricula, dia ? `${a.titulo} até ${dia}` : a.titulo)
    case 'carta':
      return juntar(a.nome ?? 'Motorista', dia ? `carta ${passou ? 'caducou' : 'caduca'} a ${dia}` : a.titulo)
  }
}
