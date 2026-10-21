import { cleanup, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import * as api from '../api/comandos'
import type { Relatorio } from '../api/tipos'
import { mesesDoPeriodo, periodoDe } from './periodos'
import { Relatorios } from './Relatorios'

vi.mock('../api/comandos')
const m = vi.mocked(api)

const vazio: Relatorio = {
  resumo: { total: 0, porCategoria: {}, porMes: {}, totalPorMes: {}, porCarro: [], porPagar: 0, porPagarVencido: 0, porPagarProximo: 0, porPagarDias: 30 },
  porMotorista: [],
  porTipoManutencao: [],
  manutencao: { interna: 0, oficina: 0, pecas: 0, maoObra: 0, naoDiscriminado: 0, porOficina: [] },
  multas: { n: 0, total: 0, empresa: 0, colaboradores: 0, porReembolsar: 0, porInfracao: [], porMotorista: [] },
}

/** Fixa só a data (os temporizadores continuam reais para o user-event). */
function hojeE(ano: number, mes: number, dia: number) {
  vi.useFakeTimers({ toFake: ['Date'] })
  vi.setSystemTime(new Date(ano, mes - 1, dia, 12))
}

afterEach(() => {
  cleanup()
  vi.useRealTimers()
})

beforeEach(() => {
  vi.resetAllMocks()
  m.relatorio.mockResolvedValue(vazio)
  m.carrosListar.mockResolvedValue([])
  m.motoristasListar.mockResolvedValue([])
})

describe('Relatórios — período', () => {
  test("'Ano anterior' em 2026-10-20 pede 2025-01-01..2025-12-31", async () => {
    hojeE(2026, 10, 20)
    const user = userEvent.setup()
    render(<Relatorios onNavegar={() => {}} />)
    await waitFor(() => expect(m.relatorio).toHaveBeenCalled())
    await user.click(screen.getByRole('button', { name: 'Ano anterior' }))
    await waitFor(() =>
      expect(m.relatorio).toHaveBeenLastCalledWith(
        expect.objectContaining({ de: '2025-01-01', ate: '2025-12-31' }),
      ),
    )
  })

  test("'Mês anterior' em 2026-01-15 pede 2025-12-01..2025-12-31", async () => {
    hojeE(2026, 1, 15)
    const user = userEvent.setup()
    render(<Relatorios onNavegar={() => {}} />)
    await waitFor(() => expect(m.relatorio).toHaveBeenCalled())
    await user.click(screen.getByRole('button', { name: 'Mês anterior' }))
    await waitFor(() =>
      expect(m.relatorio).toHaveBeenLastCalledWith(
        expect.objectContaining({ de: '2025-12-01', ate: '2025-12-31' }),
      ),
    )
  })

  test('presets em datas-limite', () => {
    expect(periodoDe('esteMes', '2026-02-10')).toEqual({ de: '2026-02-01', ate: '2026-02-28' })
    expect(periodoDe('esteMes', '2028-02-10')).toEqual({ de: '2028-02-01', ate: '2028-02-29' })
    expect(periodoDe('esteAno', '2026-10-20')).toEqual({ de: '2026-01-01', ate: '2026-12-31' })
    expect(periodoDe('mesAnterior', '2026-03-31')).toEqual({ de: '2026-02-01', ate: '2026-02-28' })
    expect(mesesDoPeriodo('2025-11-15', '2026-02-03')).toEqual(['2025-11', '2025-12', '2026-01', '2026-02'])
  })
})

describe('Relatórios — conteúdo vindo do núcleo', () => {
  test('mostra total, categorias, ranking e o bloco de infrações', async () => {
    m.carrosListar.mockResolvedValue([
      {
        id: 1, matricula: 'AA-12-BB', marca: null, modelo: null, ano: null, cor: null, kmInicial: 0,
        kmAtual: 0, dataAquisicao: null, estado: 'ativo', dataSaida: null, notas: null,
        motoristaAtual: null, gastoAno: 0, gastoTotal: 0,
      },
    ])
    m.relatorio.mockResolvedValue({
      ...vazio,
      resumo: {
        total: 123456,
        porCategoria: { manutencao: 100000, pneus: 23456 },
        porMes: { '2026-10': { manutencao: 100000, pneus: 23456 } },
        totalPorMes: { '2026-10': 123456 },
        porCarro: [[1, 123456]],
        porPagar: 5000,
        porPagarVencido: 2000,
        porPagarProximo: 3000,
        porPagarDias: 30,
      },
      multas: { ...vazio.multas, n: 2, total: 9000, porReembolsar: 3000 },
    })
    const onNavegar = vi.fn()
    const user = userEvent.setup()
    render(<Relatorios onNavegar={onNavegar} />)
    const total = await screen.findByLabelText('Total no período')
    expect(total.textContent).toMatch(/1\s234,56 €/)
    expect(screen.getByText('Por pagar (hoje)')).toBeTruthy()
    expect(screen.getByText(/vencido 20,00 € · próximos 30 dias 30,00 €/)).toBeTruthy()
    const infracoes = screen.getByRole('region', { name: 'Infrações no período' })
    expect(within(infracoes).getByText(/data da infração/)).toBeTruthy()
    expect(within(infracoes).getByText('30,00 €')).toBeTruthy()
    await user.click(await screen.findByRole('button', { name: /AA-12-BB/ }))
    expect(onNavegar).toHaveBeenCalledWith({ nome: 'carro', id: 1 })
  })

  test('com erro ou com um pedido novo pendente, os valores anteriores desaparecem', async () => {
    const user = userEvent.setup()
    m.relatorio.mockResolvedValueOnce({ ...vazio, resumo: { ...vazio.resumo, total: 4200 } })
    render(<Relatorios onNavegar={() => {}} />)
    expect((await screen.findByLabelText('Total no período')).textContent).toBe('42,00 €')
    // pedido novo que ainda não respondeu
    let responder: (r: Relatorio) => void = () => {}
    m.relatorio.mockReturnValueOnce(new Promise((ok) => (responder = ok)))
    await user.click(screen.getByRole('button', { name: 'Este ano' }))
    expect(await screen.findByRole('status')).toBeTruthy()
    expect(screen.queryByLabelText('Total no período')).toBeNull()
    responder({ ...vazio, resumo: { ...vazio.resumo, total: 9900 } })
    expect((await screen.findByLabelText('Total no período')).textContent).toBe('99,00 €')
    // erro: mostra a mensagem e não os números antigos
    m.relatorio.mockRejectedValueOnce({ codigo: 'io', mensagem: 'Não foi possível ler o relatório.' })
    await user.click(screen.getByRole('button', { name: 'Ano anterior' }))
    expect(await screen.findByText('Não foi possível ler o relatório.')).toBeTruthy()
    expect(screen.queryByLabelText('Total no período')).toBeNull()
  })

  test('filtro de carros fecha com Esc e com clique fora', async () => {
    m.carrosListar.mockResolvedValue([
      {
        id: 1, matricula: 'AA-12-BB', marca: null, modelo: null, ano: null, cor: null, kmInicial: 0,
        kmAtual: 0, dataAquisicao: null, estado: 'ativo', dataSaida: null, notas: null,
        motoristaAtual: null, gastoAno: 0, gastoTotal: 0,
      },
    ])
    const user = userEvent.setup()
    render(<Relatorios onNavegar={() => {}} />)
    const botao = await screen.findByRole('button', { name: /Carros/ })
    expect(botao.getAttribute('aria-expanded')).toBe('false')
    await user.click(botao)
    const caixa = await screen.findByRole('checkbox', { name: 'AA-12-BB' })
    await user.click(caixa)
    await waitFor(() => expect(m.relatorio).toHaveBeenLastCalledWith(expect.objectContaining({ carroIds: [1] })))
    await user.keyboard('{Escape}')
    expect(screen.queryByRole('checkbox', { name: 'AA-12-BB' })).toBeNull()
    expect(document.activeElement).toBe(botao)
    await user.click(botao)
    expect(screen.getByRole('checkbox', { name: 'AA-12-BB' })).toBeTruthy()
    await user.click(document.body)
    expect(screen.queryByRole('checkbox', { name: 'AA-12-BB' })).toBeNull()
  })

  test('filtro por motorista vai no pedido', async () => {
    m.motoristasListar.mockResolvedValue([
      { id: 3, nome: 'Ana', telefone: null, cartaNumero: null, cartaValidade: null, ativo: true, notas: null, alertaCarta: null },
    ])
    const user = userEvent.setup()
    render(<Relatorios onNavegar={() => {}} />)
    await screen.findByRole('option', { name: 'Ana' })
    await user.selectOptions(screen.getByLabelText('Motorista'), '3')
    await waitFor(() =>
      expect(m.relatorio).toHaveBeenLastCalledWith(expect.objectContaining({ motoristaId: 3 })),
    )
  })
})
