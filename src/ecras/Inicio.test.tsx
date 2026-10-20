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
    ...extra,
  }
}

const ctx = {
  hoje: '2026-10-20',
  matricula: () => 'AA-12-BB',
  motorista: () => ({ nome: 'João Silva', cartaValidade: '2026-12-12' }),
  limiares: { km: 1000, dias: 30 },
}

describe('textos dos alertas', () => {
  test('manutenção por KM e por dias', () => {
    expect(textoAlerta(alerta({ faltaKm: 420 }), ctx)).toBe('AA-12-BB · Mudança de óleo · faltam 420 km')
    expect(textoAlerta(alerta({ nivel: 'vencido', faltaKm: -200, faltaDias: 100 }), ctx)).toBe(
      'AA-12-BB · Mudança de óleo · passou 200 km',
    )
    expect(textoAlerta(alerta({ faltaKm: 5000, faltaDias: 5 }), ctx)).toBe(
      'AA-12-BB · Mudança de óleo · faltam 5 dias',
    )
  })

  test('documento, carta, multa e identificar condutor', () => {
    expect(
      textoAlerta(alerta({ tipo: 'documento', titulo: 'IUC', faltaDias: 223 }), { ...ctx, hoje: '2026-10-20' }),
    ).toBe('AA-12-BB · IUC · vence a 31 mai 2027')
    expect(textoAlerta(alerta({ tipo: 'documento', titulo: 'IUC', faltaDias: 11 }), ctx)).toBe(
      'AA-12-BB · IUC · vence a 31 out',
    )
    expect(
      textoAlerta(alerta({ tipo: 'carta', carroId: null, motoristaId: 4, titulo: 'Carta de condução', faltaDias: 53 }), ctx),
    ).toBe('João Silva · carta caduca a 12 dez')
    expect(
      textoAlerta(alerta({ tipo: 'identificarCondutor', titulo: 'Identificar condutor até 23 out', faltaDias: 3 }), ctx),
    ).toBe('AA-12-BB · Identificar condutor até 23 out')
    expect(textoAlerta(alerta({ tipo: 'multa', titulo: 'Multa por pagar', faltaDias: -2 }), ctx)).toBe(
      'AA-12-BB · Multa por pagar · venceu a 18 out',
    )
  })
})

describe('Início', () => {
  const painel: Painel = {
    gastoMes: 128450,
    gastoAno: 1894230,
    porPagar: 61200,
    carrosAtivos: 23,
    alertas: [
      alerta({ nivel: 'vencido', faltaKm: -200 }),
      alerta({ tipo: 'carta', carroId: null, motoristaId: 4, refId: 4, titulo: 'Carta de condução', faltaDias: 53 }),
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
    m.carrosListar.mockResolvedValue([
      {
        id: 1, matricula: 'AA-12-BB', marca: null, modelo: null, ano: null, cor: null, kmInicial: 0,
        kmAtual: 0, dataAquisicao: null, estado: 'ativo', dataSaida: null, notas: null,
        motoristaAtual: null, gastoAno: 0, gastoTotal: 0,
      },
    ])
    m.motoristasListar.mockResolvedValue([
      { id: 4, nome: 'João Silva', telefone: null, cartaNumero: null, cartaValidade: '2026-12-12', ativo: true, notas: null, alertaCarta: 'proximo' },
    ])
    m.limiaresObter.mockResolvedValue({ km: 1000, dias: 30 })
  })

  test('cartões, vencimentos com severidade e navegação', async () => {
    const onNavegar = vi.fn()
    const user = userEvent.setup()
    render(<Inicio ano={2026} onNavegar={onNavegar} />)
    const venc = await screen.findByRole('region', { name: /^Próximos vencimentos/ })
    expect(screen.getByText('Por pagar (hoje)')).toBeTruthy()
    expect(screen.getByText('Carros ativos')).toBeTruthy()
    const linhas = within(venc).getAllByRole('button')
    expect(linhas[0].textContent).toContain('AA-12-BB · Mudança de óleo · passou 200 km')
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
