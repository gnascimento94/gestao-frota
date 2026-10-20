import { cleanup, render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import * as api from '../api/comandos'
import type { Plano, TipoManutencao } from '../api/tipos'
import { DesfazerProvider } from '../ui/Desfazer'
import { ESPACO_FINO } from '../util/formato'
import { Planos } from './Planos'
import { textoPlano } from './textosPlanos'

vi.mock('../api/comandos')
const m = vi.mocked(api)

afterEach(cleanup)

const tipos: TipoManutencao[] = [
  { id: 1, nome: 'Mudança de óleo', ativo: true },
  { id: 2, nome: 'Filtros', ativo: true },
]

const oleo: Plano = { id: 7, tipoId: 1, tipoNome: 'Mudança de óleo', cadaKm: 15000, cadaMeses: 12, todosCarros: true, carros: [] }

beforeEach(() => {
  vi.resetAllMocks()
  m.tiposListar.mockResolvedValue(tipos)
  m.carrosListar.mockResolvedValue([])
  m.limiaresObter.mockResolvedValue({ km: 1000, dias: 30 })
  m.planoGuardar.mockResolvedValue(1)
})

function montar(soConsulta = false) {
  return render(
    <DesfazerProvider aoFecharJanela={() => () => {}}>
      <Planos ano={2026} soConsulta={soConsulta} />
    </DesfazerProvider>,
  )
}

test('texto de cada plano', () => {
  const sem = new Map<number, string>([[3, 'AA-00-AA']])
  expect(textoPlano(oleo, sem)).toBe(`Mudança de óleo · a cada 15${ESPACO_FINO}000 km ou 12 meses · todos os carros`)
  expect(textoPlano({ ...oleo, cadaKm: null, cadaMeses: 1, todosCarros: false, carros: [3] }, sem)).toBe(
    'Mudança de óleo · a cada 1 mês · AA-00-AA',
  )
  expect(textoPlano({ ...oleo, cadaMeses: null, todosCarros: false, carros: [3, 4, 5] }, sem)).toBe(
    `Mudança de óleo · a cada 15${ESPACO_FINO}000 km · 3 carros`,
  )
})

test('sem planos sugere a mudança de óleo e "Criar" pré-preenche o formulário', async () => {
  m.planosListar.mockResolvedValue([])
  montar()
  expect(await screen.findByText(/Sugestão: Mudança de óleo a cada 15.000 km ou 12 meses/)).toBeTruthy()
  await userEvent.click(screen.getByRole('button', { name: 'Criar' }))
  const folha = await screen.findByRole('dialog', { name: 'Novo plano' })
  expect((within(folha).getByLabelText('Tipo de manutenção') as HTMLSelectElement).value).toBe('1')
  expect((within(folha).getByLabelText('A cada (km)') as HTMLInputElement).value).toBe('15000')
  expect((within(folha).getByLabelText('A cada (meses)') as HTMLInputElement).value).toBe('12')
  await userEvent.click(within(folha).getByRole('button', { name: 'Guardar' }))
  expect(m.planoGuardar).toHaveBeenCalledWith({
    id: null,
    tipoId: 1,
    cadaKm: 15000,
    cadaMeses: 12,
    todosCarros: true,
    carros: [],
  })
})

test('erro do núcleo aparece junto dos intervalos', async () => {
  m.planosListar.mockResolvedValue([])
  m.planoGuardar.mockRejectedValue({ codigo: 'plano_vazio', mensagem: 'Indique os km, os meses ou ambos.' })
  montar()
  await userEvent.click(await screen.findByRole('button', { name: 'Novo plano' }))
  const folha = await screen.findByRole('dialog', { name: 'Novo plano' })
  await userEvent.click(within(folha).getByRole('button', { name: 'Guardar' }))
  expect(await within(folha).findByText('Indique os km, os meses ou ambos.')).toBeTruthy()
})

test('lista os planos e, só para consulta, não deixa alterar', async () => {
  m.planosListar.mockResolvedValue([oleo])
  montar(true)
  const lista = await screen.findByRole('list', { name: 'Planos' })
  expect(within(lista).getByText(/a cada 15.000 km ou 12 meses · todos os carros/)).toBeTruthy()
  expect((within(lista).getByRole('button', { name: /^Apagar/ }) as HTMLButtonElement).disabled).toBe(true)
  expect((screen.getByRole('button', { name: 'Novo plano' }) as HTMLButtonElement).disabled).toBe(true)
})
