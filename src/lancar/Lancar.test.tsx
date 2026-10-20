import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import * as api from '../api/comandos'
import type { Carro, Motorista } from '../api/tipos'
import { hoje } from '../ecras/comum'
import { Lancar } from './Lancar'

vi.mock('../api/comandos')
const m = vi.mocked(api)

const carro: Carro = {
  id: 1,
  matricula: 'AA-12-BB',
  marca: 'Renault',
  modelo: 'Clio',
  ano: 2020,
  cor: null,
  kmInicial: 0,
  kmAtual: 70000,
  dataAquisicao: null,
  estado: 'ativo',
  dataSaida: null,
  notas: null,
  motoristaAtual: null,
  gastoAno: 0,
  gastoTotal: 0,
}

function motorista(id: number, nome: string): Motorista {
  return { id, nome, telefone: null, cartaNumero: null, cartaValidade: null, ativo: true, notas: null, alertaCarta: null }
}

afterEach(cleanup)

beforeEach(() => {
  vi.resetAllMocks()
  m.carrosListar.mockResolvedValue([carro])
  m.tiposListar.mockResolvedValue([
    { id: 1, nome: 'Mudança de óleo', ativo: true },
    { id: 2, nome: 'Travões', ativo: true },
  ])
  m.oficinasUsadas.mockResolvedValue(['Oficina Auto Lima'])
  m.motoristasListar.mockResolvedValue([motorista(3, 'Ana Ferreira'), motorista(4, 'João Silva')])
  m.condutorSugerido.mockResolvedValue(3)
  m.lancarManutencao.mockResolvedValue(10)
  m.lancarPneu.mockResolvedValue(10)
  m.lancarDocumento.mockResolvedValue(10)
  m.guardarMulta.mockResolvedValue(10)
  m.atualizarKm.mockResolvedValue(10)
})

async function abrir(tipo?: string) {
  const user = userEvent.setup()
  const onLancado = vi.fn()
  const onFechar = vi.fn()
  render(<Lancar aberta origem={null} carroId={1} onFechar={onFechar} onLancado={onLancado} />)
  // O carro vem pré-selecionado (aberto a partir da ficha).
  await screen.findByDisplayValue(/AA-12-BB/)
  if (tipo) await user.click(screen.getByRole('button', { name: tipo }))
  return { user, onLancado, onFechar }
}

const lancar = () => screen.getByRole('button', { name: 'Lançar' })

async function escolherTipoManutencao(user: ReturnType<typeof userEvent.setup>) {
  const tipo = screen.getByLabelText('Tipo')
  await within(tipo).findByRole('option', { name: 'Mudança de óleo' })
  await user.selectOptions(tipo, '1')
}

test("valor '1.234,56' envia 123456 cêntimos", async () => {
  const { user, onLancado, onFechar } = await abrir()
  await escolherTipoManutencao(user)
  await user.type(screen.getByLabelText('Peças e materiais (€)'), '1.234,56')
  await user.click(lancar())
  await waitFor(() => expect(m.lancarManutencao).toHaveBeenCalledTimes(1))
  expect(m.lancarManutencao).toHaveBeenCalledWith(
    expect.objectContaining({
      carroId: 1,
      data: hoje(),
      km: 70000,
      tipoId: 1,
      executadaPor: 'propria',
      valor: 123456,
      valorPecas: 123456,
      valorMaoObra: 0,
      confirmado: false,
    }),
  )
  await waitFor(() => expect(onLancado).toHaveBeenCalled())
  expect(onFechar).toHaveBeenCalled()
  expect(await screen.findByText('Lançado ✓')).toBeTruthy()
})

test('valor inválido mostra erro e não envia', async () => {
  const { user } = await abrir()
  await escolherTipoManutencao(user)
  const campo = screen.getByLabelText('Peças e materiais (€)')
  await user.type(campo, 'abc')
  expect(screen.getByText('Escreva um valor em euros, por exemplo 120,00.')).toBeTruthy()
  expect(campo.getAttribute('aria-invalid')).toBe('true')
  await user.click(lancar())
  expect(m.lancarManutencao).not.toHaveBeenCalled()
})

test('multa exige data da infração', async () => {
  const { user } = await abrir('Multa')
  await user.type(screen.getByLabelText('Valor da multa (€)'), '60')
  await user.click(lancar())
  expect(await screen.findByText('Indique a data da infração.')).toBeTruthy()
  expect(screen.getByLabelText('Data da infração').getAttribute('aria-invalid')).toBe('true')
  expect(m.guardarMulta).not.toHaveBeenCalled()
})

test('multa pré-preenche o condutor sugerido para a data da infração', async () => {
  const { user } = await abrir('Multa')
  fireEvent.change(screen.getByLabelText('Data da infração'), { target: { value: '2026-10-01' } })
  await waitFor(() => expect(m.condutorSugerido).toHaveBeenCalledWith(1, '2026-10-01'))
  const condutor = screen.getByLabelText('Condutor') as HTMLSelectElement
  await waitFor(() => expect(condutor.value).toBe('3'))
  expect(screen.getByText('✓ Atribuído a este carro nesta data')).toBeTruthy()
  await user.type(screen.getByLabelText('Valor da multa (€)'), '60')
  await user.click(lancar())
  await waitFor(() =>
    expect(m.guardarMulta).toHaveBeenCalledWith(
      expect.objectContaining({
        carroId: 1,
        motoristaId: 3,
        dataInfracao: '2026-10-01',
        valor: 6000,
        responsavel: 'empresa',
        valorEmpresa: null,
        empresaAdiantou: false,
      }),
    ),
  )
})

test('coparticipado 120 € com empresa 40 € mostra 33% e colaborador 80,00 €', async () => {
  const { user } = await abrir('Multa')
  await user.type(screen.getByLabelText('Valor da multa (€)'), '120')
  await user.click(screen.getByRole('button', { name: 'Coparticipado' }))
  await user.type(screen.getByLabelText('Parte da empresa (€)'), '40')
  expect(
    screen.getByText((_, el) => el?.classList.contains('reparte-texto') === true &&
      el.textContent === 'Empresa 40,00 € (33%) · Colaborador 80,00 €'),
  ).toBeTruthy()
  expect(screen.getByRole('switch', { name: 'A empresa adiantou o pagamento' })).toBeTruthy()

  // Parte da empresa igual ao valor: erro e não envia.
  fireEvent.change(screen.getByLabelText('Data da infração'), { target: { value: '2026-10-01' } })
  await user.clear(screen.getByLabelText('Parte da empresa (€)'))
  await user.type(screen.getByLabelText('Parte da empresa (€)'), '120')
  expect(
    screen.getByText('A parte da empresa tem de ser maior que 0 e menor que o valor da multa.'),
  ).toBeTruthy()
  await user.click(lancar())
  expect(m.guardarMulta).not.toHaveBeenCalled()
})

test("manutenção na oficina sem nome mostra erro; 'Nós' esconde mão de obra", async () => {
  const { user } = await abrir()
  expect(screen.getByRole('button', { name: 'Nós' }).getAttribute('aria-pressed')).toBe('true')
  expect(screen.queryByLabelText('Mão de obra (€)')).toBeNull()
  await user.click(screen.getByRole('button', { name: 'Oficina' }))
  expect(screen.getByLabelText('Mão de obra (€)')).toBeTruthy()
  await escolherTipoManutencao(user)
  await user.type(screen.getByLabelText('Peças (€)'), '10')
  await user.type(screen.getByLabelText('Mão de obra (€)'), '5')
  await user.click(lancar())
  expect(await screen.findByText('Indique o nome da oficina.')).toBeTruthy()
  expect(m.lancarManutencao).not.toHaveBeenCalled()
})

test('peças 50 + mão de obra 30 envia valor 8000', async () => {
  const { user } = await abrir()
  await user.click(screen.getByRole('button', { name: 'Oficina' }))
  await escolherTipoManutencao(user)
  await user.type(screen.getByLabelText('Oficina'), 'Auto Lima')
  await user.type(screen.getByLabelText('Peças (€)'), '50')
  await user.type(screen.getByLabelText('Mão de obra (€)'), '30')
  expect(screen.getByText('80,00 €')).toBeTruthy()
  await user.click(lancar())
  await waitFor(() =>
    expect(m.lancarManutencao).toHaveBeenCalledWith(
      expect.objectContaining({
        executadaPor: 'oficina',
        oficina: 'Auto Lima',
        valor: 8000,
        valorPecas: 5000,
        valorMaoObra: 3000,
      }),
    ),
  )
})

test('km abaixo pede confirmação e reenvia com confirmado=true', async () => {
  m.atualizarKm
    .mockRejectedValueOnce({ codigo: 'km_abaixo', mensagem: 'O último KM registado era 70 000. Confirma 69 000?' })
    .mockResolvedValueOnce(11)
  const { user, onLancado } = await abrir('Atualizar KM')
  const km = screen.getByLabelText('KM')
  expect((km as HTMLInputElement).value).toBe('70000')
  await user.clear(km)
  await user.type(km, '69000')
  await user.click(lancar())
  expect(await screen.findByText('O último KM registado era 70 000. Confirma 69 000?')).toBeTruthy()
  expect(onLancado).not.toHaveBeenCalled()
  await user.click(screen.getByRole('button', { name: 'Confirmar' }))
  await waitFor(() => expect(m.atualizarKm).toHaveBeenCalledTimes(2))
  expect(m.atualizarKm).toHaveBeenNthCalledWith(1, { carroId: 1, data: hoje(), km: 69000, confirmado: false })
  expect(m.atualizarKm).toHaveBeenNthCalledWith(2, { carroId: 1, data: hoje(), km: 69000, confirmado: true })
  await waitFor(() => expect(onLancado).toHaveBeenCalled())
})

test('IUC vem marcado como anual', async () => {
  const { user } = await abrir('Documento/taxa')
  await user.click(screen.getByRole('button', { name: 'IUC' }))
  const anual = screen.getByRole('switch', { name: 'Anual' }) as HTMLInputElement
  expect(anual.checked).toBe(true)
  await user.click(screen.getByRole('button', { name: 'Inspeção' }))
  expect((screen.getByRole('switch', { name: 'Anual' }) as HTMLInputElement).checked).toBe(false)
  await user.click(screen.getByRole('button', { name: 'Seguro' }))
  expect((screen.getByRole('switch', { name: 'Anual' }) as HTMLInputElement).checked).toBe(true)
})

test('erro do núcleo aparece no campo respetivo (data futura → data)', async () => {
  m.lancarPneu.mockRejectedValue({ codigo: 'data_futura', mensagem: 'A data não pode ser no futuro.' })
  const { user } = await abrir('Pneus')
  await user.type(screen.getByLabelText('Valor (€)'), '80')
  await user.click(lancar())
  expect(await screen.findByText('A data não pode ser no futuro.')).toBeTruthy()
  expect(screen.getByLabelText('Data').getAttribute('aria-invalid')).toBe('true')
  expect(screen.getByRole('dialog')).toBeTruthy()
})
