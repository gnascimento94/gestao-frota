// Períodos dos relatórios. Só datas (a interface não calcula valores): o núcleo soma.

export type Preset = 'esteMes' | 'mesAnterior' | 'esteAno' | 'anoAnterior'

export interface Periodo {
  de: string
  ate: string
}

const dois = (n: number) => String(n).padStart(2, '0')

/** Último dia do mês (1–12). */
function ultimoDia(ano: number, mes: number): number {
  return new Date(Date.UTC(ano, mes, 0)).getUTCDate()
}

function mesInteiro(ano: number, mes: number): Periodo {
  return { de: `${ano}-${dois(mes)}-01`, ate: `${ano}-${dois(mes)}-${dois(ultimoDia(ano, mes))}` }
}

/** Datas de/até de um período predefinido, relativas a `hoje` ("AAAA-MM-DD"). */
export function periodoDe(preset: Preset, hoje: string): Periodo {
  const ano = Number(hoje.slice(0, 4))
  const mes = Number(hoje.slice(5, 7))
  switch (preset) {
    case 'esteMes':
      return mesInteiro(ano, mes)
    case 'mesAnterior':
      return mes === 1 ? mesInteiro(ano - 1, 12) : mesInteiro(ano, mes - 1)
    case 'esteAno':
      return { de: `${ano}-01-01`, ate: `${ano}-12-31` }
    case 'anoAnterior':
      return { de: `${ano - 1}-01-01`, ate: `${ano - 1}-12-31` }
  }
}

/** Todos os meses ("AAAA-MM") tocados por [de, até], incluindo os sem gasto. */
export function mesesDoPeriodo(de: string, ate: string): string[] {
  let a = Number(de.slice(0, 4))
  let m = Number(de.slice(5, 7))
  const fimA = Number(ate.slice(0, 4))
  const fimM = Number(ate.slice(5, 7))
  const out: string[] = []
  // Limite de segurança: no máximo 50 anos de meses.
  while ((a < fimA || (a === fimA && m <= fimM)) && out.length < 600) {
    out.push(`${a}-${dois(m)}`)
    m += 1
    if (m > 12) {
      m = 1
      a += 1
    }
  }
  return out
}

/** Data válida "AAAA-MM-DD". */
export function dataValida(s: string): boolean {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(s)
  if (!m) return false
  const mes = Number(m[2])
  const dia = Number(m[3])
  return mes >= 1 && mes <= 12 && dia >= 1 && dia <= ultimoDia(Number(m[1]), mes)
}
