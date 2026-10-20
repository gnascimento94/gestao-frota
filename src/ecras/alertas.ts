// Texto dos alertas do Início. O nível, o tipo e as distâncias (km/dias) vêm do núcleo;
// aqui só se escolhe a frase e se formata.

import type { Alerta, Limiares } from '../api/tipos'
import { formatarDiaMes, formatarKm } from '../util/formato'

export interface ContextoAlerta {
  /** "AAAA-MM-DD". */
  hoje: string
  matricula(carroId: number): string
  motorista(id: number): { nome: string; cartaValidade: string | null } | undefined
  /** Para saber se a manutenção é assinalada pelos km ou pelos dias. */
  limiares: Limiares
}

/** `hoje` + `n` dias de calendário, como "AAAA-MM-DD". */
export function somarDias(hoje: string, n: number): string {
  const [a, m, d] = hoje.split('-').map(Number)
  return new Date(Date.UTC(a, m - 1, d + n)).toISOString().slice(0, 10)
}

function faltaKm(k: number): string {
  if (k > 0) return k === 1 ? 'falta 1 km' : `faltam ${formatarKm(k)}`
  if (k === 0) return 'atingiu o KM'
  return `passou ${formatarKm(-k)}`
}

function faltaDias(d: number): string {
  if (d > 1) return `faltam ${d} dias`
  if (d === 1) return 'falta 1 dia'
  if (d === 0) return 'é hoje'
  return d === -1 ? 'passou 1 dia' : `passaram ${-d} dias`
}

/** Manutenção: mostra a distância que dispara o alerta (km primeiro). */
function distanciaManutencao(a: Alerta, lim: Limiares): string {
  const km = a.faltaKm
  const dias = a.faltaDias
  const kmDispara = km !== null && (a.nivel === 'vencido' ? km <= 0 : km < lim.km)
  if (km !== null && (kmDispara || dias === null)) return faltaKm(km)
  if (dias !== null) return faltaDias(dias)
  return ''
}

/** Frase do alerta, p.ex. `"AA-12-BB · Mudança de óleo · faltam 420 km"`. */
export function textoAlerta(a: Alerta, ctx: ContextoAlerta): string {
  const ano = Number(ctx.hoje.slice(0, 4))
  const mat = a.carroId !== null ? ctx.matricula(a.carroId) : ''
  const prazo = a.faltaDias !== null ? formatarDiaMes(somarDias(ctx.hoje, a.faltaDias), ano) : null
  const passou = (a.faltaDias ?? 0) < 0
  switch (a.tipo) {
    case 'manutencao':
      return [mat, a.titulo, distanciaManutencao(a, ctx.limiares)].filter(Boolean).join(' · ')
    case 'documento':
    case 'multa':
      return [mat, a.titulo, prazo && `${passou ? 'venceu' : 'vence'} a ${prazo}`].filter(Boolean).join(' · ')
    case 'identificarCondutor':
      return [mat, a.titulo].filter(Boolean).join(' · ')
    case 'carta': {
      const m = a.motoristaId !== null ? ctx.motorista(a.motoristaId) : undefined
      const validade = m?.cartaValidade ? formatarDiaMes(m.cartaValidade, ano) : prazo
      return `${m?.nome ?? 'Motorista'} · carta ${passou ? 'caducou' : 'caduca'} a ${validade ?? '—'}`
    }
  }
}
