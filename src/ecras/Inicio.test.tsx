import { cleanup, render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import * as api from '../api/comandos'
import type { Alerta, Painel } from '../api/tipos'
import { textoAlerta } from './alertas'
import { Inicio } from './Inicio'

vi.mock('../api/comandos')
const m = vi.mocked(api)

afterEach(cleanup)

function alerta(extra: Partial<Alerta>): Alerta {
  return {
    nivel: 'proximo',
    tipo: 'manutencao',
    carroId: 1,
    motoristaId: null,
    refId: 1,
    titulo: 'Mudança de óleo',
    faltaKm: null,
    faltaDias: null,
    motivo: null,
    data: null,
    matricula: 'AA-12-BB',
    nome: null,
    ...extra,
  }
}

describe('textos dos alertas (dados do núcleo)', () => {
  test('manutenção: a dimensão indicada pelo núcleo', () => {
    expect(textoAlerta(alerta({ motivo: 'km', faltaKm: 420 }), 2026)).toBe('AA-12-BB · Mudança de óleo · faltam 420 km')
    expect(textoAlerta(alerta({ nivel: 'vencido', motivo: 'km', faltaKm: -200 }), 2026)).toBe(
      'AA-12-BB · Mudança de óleo · passou 200 km',
    )
    expect(textoAlerta(alerta({ motivo: 'dias', faltaDias: 5, data: '2026-10-25' }), 2026)).toBe(
      'AA-12-BB · Mudança de óleo · faltam 5 dias',
    )
    expect(textoAlerta(alerta({ nivel: 'vencido', motivo: 'dias', faltaDias: 0, data: '2026-10-20' }), 2026)).toBe(
      'AA-12-BB · Mudança de óleo · vence hoje',
    )
  })

  test('documento, carta, multa e identificar condutor usam a data do núcleo', () => {
    const iuc = alerta({ tipo: 'documento', titulo: 'IUC' })
    expect(textoAlerta({ ...iuc, faltaDias: 223, data: '2027-05-31' }, 2026)).toBe('AA-12-BB · IUC · vence a 31 mai 2027')
    expect(textoAlerta({ ...iuc, faltaDias: 11, data: '2026-10-31' }, 2026)).toBe('AA-12-BB · IUC · vence a 31 out')
    expect(textoAlerta({ ...iuc, faltaDias: 0, data: '2026-10-20' }, 2026)).toBe('AA-12-BB · IUC · vence hoje')
    expect(
      textoAlerta(
        alerta({
          tipo: 'carta', carroId: null, matricula: null, motoristaId: 4, nome: 'João Silva',
          titulo: 'Carta de condução', faltaDias: 53, data: '2026-12-12',
        }),
        2026,
      ),
    ).toBe('João Silva · carta caduca a 12 dez')
    expect(
      textoAlerta(alerta({ tipo: 'identificarCondutor', titulo: 'Identificar condutor', faltaDias: 3, data: '2026-10-03' }), 2026),
    ).toBe('AA-12-BB · Identificar condutor até 3 out')
    expect(
      textoAlerta(alerta({ tipo: 'multa', nivel: 'vencido', titulo: 'Multa por pagar', faltaDias: -2, data: '2026-10-18' }), 2026),
    ).toBe('AA-12-BB · Multa por pagar · venceu a 18 out')
  })
})

describe('Início', () => {
  const painel: Painel = {
    ano: 2025,
    mes: 12,
    gastoMes: 128450,
    gastoAno: 1894230,
    porPagar: 61200,
    carrosAtivos: 23,
    alertas: [
      alerta({ nivel: 'vencido', motivo: 'km', faltaKm: -200 }),
      alerta({
        tipo: 'carta', carroId: null, matricula: null, motoristaId: 4, refId: 4, nome: 'João Silva',
        titulo: 'Carta de condução', faltaDias: 53, data: '2026-12-12',
      }),
    ],
    recentes: [
      {
        tipo: 'manutencao', id: 5, carroId: 1, matricula: 'AA-12-BB', data: '2026-10-20', km: 152340,
        titulo: 'Mudança de óleo', valor: 4680, detalhe: null, porPagar: false, vencimento: null,
        motoristaId: null, estadoMulta: null,
      },
    ],
  }

  beforeEach(() => {
    vi.resetAllMocks()
    m.painel.mockResolvedValue(painel)
    m.carrosListar.mockResolvedValue([])
  })

  test('cartões com os valores do núcleo', async () => {
    render(<Inicio ano={2026} onNavegar={() => {}} />)
    const kpi = async (rotulo: string) => (await screen.findByText(rotulo)).parentElement!.textContent
    expect(await kpi('Gasto este mês')).toMatch(/1\s284,50 €dezembro/)
    // o ano mostrado é o que o núcleo usou
    expect(await kpi('Gasto no ano')).toMatch(/18\s942,30 €2025/)
    expect(await kpi('Por pagar (hoje)')).toMatch(/612,00 €/)
    expect(await kpi('Carros ativos')).toMatch(/23/)
    expect(m.limiaresObter).not.toHaveBeenCalled()
  })

  test('vencimentos com severidade e navegação', async () => {
    const onNavegar = vi.fn()
    const user = userEvent.setup()
    render(<Inicio ano={2026} onNavegar={onNavegar} />)
    const venc = await screen.findByRole('region', { name: /^Próximos vencimentos/ })
    const linhas = within(venc).getAllByRole('button')
    expect(linhas[0].textContent).toContain('AA-12-BB · Mudança de óleo · passou 200 km')
    expect(linhas[1].textContent).toContain('João Silva · carta caduca a 12 dez 2026')
    expect(linhas[0].querySelector('.faixa-vencido')).toBeTruthy()
    expect(linhas[1].querySelector('.faixa-proximo')).toBeTruthy()
    await user.click(linhas[0])
    expect(onNavegar).toHaveBeenLastCalledWith({ nome: 'carro', id: 1 })
    await user.click(linhas[1])
    expect(onNavegar).toHaveBeenLastCalledWith({ nome: 'motoristas', id: 4 })
    const recentes = screen.getByRole('region', { name: 'Últimos lançamentos' })
    expect(within(recentes).getByText('Mudança de óleo')).toBeTruthy()
  })
})
