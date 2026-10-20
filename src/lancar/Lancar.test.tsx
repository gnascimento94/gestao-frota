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


// ---------- Ronda de correções 1 ----------

const condutorSelect = () => screen.getByLabelText('Condutor') as HTMLSelectElement
const dataInfracao = () => screen.getByLabelText('Data da infração')

test('sugestão de condutor que falha limpa o condutor e mostra a mensagem do núcleo', async () => {
  m.condutorSugerido
    .mockResolvedValueOnce(3)
    .mockRejectedValueOnce({ codigo: 'io', mensagem: 'Não foi possível ler as atribuições.' })
  await abrir('Multa')
  fireEvent.change(dataInfracao(), { target: { value: '2026-10-01' } })
  await waitFor(() => expect(condutorSelect().value).toBe('3'))
  fireEvent.change(dataInfracao(), { target: { value: '2026-09-01' } })
  expect(await screen.findByText('Não foi possível ler as atribuições.')).toBeTruthy()
  expect(condutorSelect().value).toBe('')
  expect(m.condutorSugerido).toHaveBeenLastCalledWith(1, '2026-09-01')
})

test('mudar a data da infração limpa o condutor antes de chegar a nova sugestão', async () => {
  m.condutorSugerido.mockResolvedValueOnce(3).mockReturnValueOnce(new Promise(() => {}))
  await abrir('Multa')
  fireEvent.change(dataInfracao(), { target: { value: '2026-10-01' } })
  await waitFor(() => expect(condutorSelect().value).toBe('3'))
  fireEvent.change(dataInfracao(), { target: { value: '2026-09-01' } })
  expect(condutorSelect().value).toBe('')
  fireEvent.change(dataInfracao(), { target: { value: '' } })
  expect(condutorSelect().value).toBe('')
})

test("'Por identificar' envia motoristaId null", async () => {
  const { user } = await abrir('Multa')
  fireEvent.change(dataInfracao(), { target: { value: '2026-10-01' } })
  await waitFor(() => expect(condutorSelect().value).toBe('3'))
  await user.selectOptions(condutorSelect(), 'Por identificar')
  await user.type(screen.getByLabelText('Valor da multa (€)'), '60')
  await user.click(lancar())
  await waitFor(() =>
    expect(m.guardarMulta).toHaveBeenCalledWith(expect.objectContaining({ motoristaId: null })),
  )
})

test("'A empresa adiantou' aparece em Colaborador e não em Empresa", async () => {
  const { user } = await abrir('Multa')
  await user.type(screen.getByLabelText('Valor da multa (€)'), '120')
  expect(screen.queryByRole('switch', { name: 'A empresa adiantou o pagamento' })).toBeNull()
  await user.click(screen.getByRole('button', { name: 'Colaborador' }))
  const sw = screen.getByRole('switch', { name: 'A empresa adiantou o pagamento' })
  expect(
    screen.getByText((_, el) => el?.classList.contains('reparte-texto') === true &&
      el.textContent === 'Empresa 0,00 € (0%) · Colaborador 120,00 €'),
  ).toBeTruthy()
  await user.click(sw)
  fireEvent.change(dataInfracao(), { target: { value: '2026-10-01' } })
  await user.click(lancar())
  await waitFor(() =>
    expect(m.guardarMulta).toHaveBeenCalledWith(
      expect.objectContaining({ responsavel: 'colaborador', valorEmpresa: null, empresaAdiantou: true }),
    ),
  )
  await user.click(screen.getByRole('button', { name: 'Empresa' }))
  expect(screen.queryByRole('switch', { name: 'A empresa adiantou o pagamento' })).toBeNull()
})

test("'Só tenho o total' envia valorPecas e valorMaoObra a null", async () => {
  const { user } = await abrir()
  await user.click(screen.getByRole('button', { name: 'Oficina' }))
  await escolherTipoManutencao(user)
  await user.type(screen.getByLabelText('Oficina'), 'Auto Lima')
  await user.click(screen.getByRole('switch', { name: 'Só tenho o total' }))
  expect(screen.queryByLabelText('Mão de obra (€)')).toBeNull()
  await user.type(screen.getByLabelText('Total (€)'), '99,90')
  await user.click(lancar())
  await waitFor(() =>
    expect(m.lancarManutencao).toHaveBeenCalledWith(
      expect.objectContaining({ executadaPor: 'oficina', valor: 9990, valorPecas: null, valorMaoObra: null }),
    ),
  )
})

async function manutencaoNaOficina(user: ReturnType<typeof userEvent.setup>) {
  await user.click(screen.getByRole('button', { name: 'Oficina' }))
  await escolherTipoManutencao(user)
  await user.type(screen.getByLabelText('Oficina'), 'Auto Lima')
  await user.type(screen.getByLabelText('Peças (€)'), '50')
  await user.type(screen.getByLabelText('Mão de obra (€)'), '30')
}

test('falta_oficina do núcleo aparece no campo Oficina', async () => {
  m.lancarManutencao.mockRejectedValue({ codigo: 'falta_oficina', mensagem: 'Indique o nome da oficina.' })
  const { user } = await abrir()
  await manutencaoNaOficina(user)
  await user.click(lancar())
  expect(await screen.findByText('Indique o nome da oficina.')).toBeTruthy()
  expect(screen.getByLabelText('Oficina').getAttribute('aria-invalid')).toBe('true')
})

test('valor_nao_bate do núcleo aparece junto ao total', async () => {
  m.lancarManutencao.mockRejectedValue({
    codigo: 'valor_nao_bate',
    mensagem: 'O total tem de ser igual a peças + mão de obra.',
  })
  const { user } = await abrir()
  await manutencaoNaOficina(user)
  await user.click(lancar())
  const erro = await screen.findByText('O total tem de ser igual a peças + mão de obra.')
  expect(erro.closest('.reparte')).toBeTruthy()
  expect(screen.queryByRole('alert')).toBeNull() // não vai para a faixa geral
})

test('coparticipacao_invalida do núcleo aparece em "Parte da empresa"', async () => {
  m.guardarMulta.mockRejectedValue({
    codigo: 'coparticipacao_invalida',
    mensagem: 'A parte da empresa tem de ser maior que 0 e menor que o valor da multa.',
  })
  const { user } = await abrir('Multa')
  fireEvent.change(dataInfracao(), { target: { value: '2026-10-01' } })
  await user.type(screen.getByLabelText('Valor da multa (€)'), '120')
  await user.click(screen.getByRole('button', { name: 'Coparticipado' }))
  await user.type(screen.getByLabelText('Parte da empresa (€)'), '40')
  await user.click(lancar())
  expect(
    await screen.findByText('A parte da empresa tem de ser maior que 0 e menor que o valor da multa.'),
  ).toBeTruthy()
  expect(screen.getByLabelText('Parte da empresa (€)').getAttribute('aria-invalid')).toBe('true')
})

test.each([
  ['referencia_invalida', 'O carro escolhido já não existe.'],
  ['so_consulta', 'Os dados estão abertos só para consulta.'],
])('%s vai para a faixa de erro', async (codigo, mensagem) => {
  m.atualizarKm.mockRejectedValue({ codigo, mensagem })
  const { user } = await abrir('Atualizar KM')
  await user.click(lancar())
  const faixa = await screen.findByRole('alert')
  expect(faixa.textContent).toBe(mensagem)
  expect(screen.getByLabelText('KM').getAttribute('aria-invalid')).toBeNull()
})

test('multa: pagamento antes da infração aparece em "Paga em" e não envia', async () => {
  const { user } = await abrir('Multa')
  fireEvent.change(dataInfracao(), { target: { value: '2026-10-01' } })
  fireEvent.change(screen.getByLabelText('Paga em'), { target: { value: '2026-09-01' } })
  await user.type(screen.getByLabelText('Valor da multa (€)'), '60')
  await user.click(lancar())
  expect(screen.getByText('O pagamento não pode ser anterior à infração.')).toBeTruthy()
  expect(screen.getByLabelText('Paga em').getAttribute('aria-invalid')).toBe('true')
  expect(m.guardarMulta).not.toHaveBeenCalled()
})

test("multa: erro do núcleo com campo 'vencimento' vai para o Prazo de pagamento", async () => {
  m.guardarMulta.mockRejectedValue({ codigo: 'data_invalida', mensagem: 'Data inválida.', campo: 'vencimento' })
  const { user } = await abrir('Multa')
  fireEvent.change(dataInfracao(), { target: { value: '2026-10-01' } })
  await user.type(screen.getByLabelText('Valor da multa (€)'), '60')
  await user.click(lancar())
  expect(await screen.findByText('Data inválida.')).toBeTruthy()
  expect(screen.getByLabelText('Prazo de pagamento').getAttribute('aria-invalid')).toBe('true')
  expect(dataInfracao().getAttribute('aria-invalid')).toBeNull()
})

test("multa: erro do núcleo com campo 'pagoEm' vai para Paga em; sem campo, para a data da infração", async () => {
  m.guardarMulta
    .mockRejectedValueOnce({ codigo: 'data_futura', mensagem: 'A data não pode ser no futuro.', campo: 'pagoEm' })
    .mockRejectedValueOnce({ codigo: 'data_antes_aquisicao', mensagem: 'A data é anterior à data de aquisição do carro.' })
  const { user } = await abrir('Multa')
  fireEvent.change(dataInfracao(), { target: { value: '2026-10-01' } })
  await user.type(screen.getByLabelText('Valor da multa (€)'), '60')
  await user.click(lancar())
  expect(await screen.findByText('A data não pode ser no futuro.')).toBeTruthy()
  expect(screen.getByLabelText('Paga em').getAttribute('aria-invalid')).toBe('true')
  await user.click(lancar())
  expect(await screen.findByText('A data é anterior à data de aquisição do carro.')).toBeTruthy()
  expect(dataInfracao().getAttribute('aria-invalid')).toBe('true')
})

test('erro do núcleo num campo que o formulário não mostra vai para a faixa', async () => {
  m.guardarMulta.mockRejectedValue({ codigo: 'data_futura', mensagem: 'A data não pode ser no futuro.', campo: 'reembolsadoEm' })
  const { user } = await abrir('Multa')
  fireEvent.change(dataInfracao(), { target: { value: '2026-10-01' } })
  await user.type(screen.getByLabelText('Valor da multa (€)'), '60')
  await user.click(lancar())
  expect((await screen.findByRole('alert')).textContent).toBe('A data não pode ser no futuro.')
})

test("manutenção: erro do núcleo com campo 'valorMaoObra' vai para Mão de obra", async () => {
  m.lancarManutencao.mockRejectedValue({ codigo: 'valor_invalido', mensagem: 'O valor não pode ser negativo.', campo: 'valorMaoObra' })
  const { user } = await abrir()
  await manutencaoNaOficina(user)
  await user.click(lancar())
  expect(await screen.findByText('O valor não pode ser negativo.')).toBeTruthy()
  expect(screen.getByLabelText('Mão de obra (€)').getAttribute('aria-invalid')).toBe('true')
})

test("documento: erro do núcleo com campo 'pagoEm' vai para Pago em", async () => {
  m.lancarDocumento.mockRejectedValue({ codigo: 'data_futura', mensagem: 'A data não pode ser no futuro.', campo: 'pagoEm' })
  const { user } = await abrir('Documento/taxa')
  await user.type(screen.getByLabelText('Valor (€)'), '35')
  fireEvent.change(screen.getByLabelText('Vencimento'), { target: { value: '2026-11-30' } })
  await user.click(lancar())
  expect(await screen.findByText('A data não pode ser no futuro.')).toBeTruthy()
  expect(screen.getByLabelText('Pago em').getAttribute('aria-invalid')).toBe('true')
  expect(screen.getByLabelText('Vencimento').getAttribute('aria-invalid')).toBeNull()
})

test("documento envia tipo 'inspecao' e pagoEm null quando não foi pago", async () => {
  const { user } = await abrir('Documento/taxa')
  await user.click(screen.getByRole('button', { name: 'Inspeção' }))
  await user.type(screen.getByLabelText('Valor (€)'), '35')
  fireEvent.change(screen.getByLabelText('Vencimento'), { target: { value: '2026-11-30' } })
  await user.click(lancar())
  await waitFor(() =>
    expect(m.lancarDocumento).toHaveBeenCalledWith({
      carroId: 1,
      tipo: 'inspecao',
      descricao: null,
      valor: 3500,
      vencimento: '2026-11-30',
      pagoEm: null,
      anual: false,
    }),
  )
})

test('seletor de carro aceita o rótulo da lista sem "·" (carro vendido sem marca/modelo)', async () => {
  const user = userEvent.setup()
  m.carrosListar.mockResolvedValue([
    carro,
    { ...carro, id: 2, matricula: 'CC-34-DD', marca: null, modelo: null, estado: 'vendido', kmAtual: 1000 },
  ])
  render(<Lancar aberta origem={null} onFechar={() => {}} onLancado={() => {}} />)
  await waitFor(() => expect(document.querySelectorAll('datalist option')).toHaveLength(2))
  const campo = screen.getByLabelText('Carro')
  expect(document.activeElement).toBe(campo) // sem carro pré-selecionado, o foco vai para o carro
  fireEvent.change(campo, { target: { value: 'CC-34-DD (vendido)' } })
  expect(screen.getByText(/KM atual: 1/)).toBeTruthy()
  await user.click(screen.getByRole('button', { name: 'Atualizar KM' }))
  expect((screen.getByLabelText('KM') as HTMLInputElement).value).toBe('1000') // KM atual do carro escolhido
  await user.clear(screen.getByLabelText('KM'))
  await user.type(screen.getByLabelText('KM'), '1500')
  await user.click(lancar())
  await waitFor(() =>
    expect(m.atualizarKm).toHaveBeenCalledWith(expect.objectContaining({ carroId: 2, km: 1500 })),
  )
})

test('pesquisa "cc34" escolhe CC-34-DD', async () => {
  const user = userEvent.setup()
  m.carrosListar.mockResolvedValue([carro, { ...carro, id: 2, matricula: 'CC-34-DD' }])
  render(<Lancar aberta origem={null} onFechar={() => {}} onLancado={() => {}} />)
  await waitFor(() => expect(document.querySelectorAll('datalist option')).toHaveLength(2))
  await user.type(screen.getByLabelText('Carro'), 'cc34')
  expect(screen.getByText(/KM atual/)).toBeTruthy()
})

test('o toast "Lançado ✓" vai para a zona de toasts partilhada', async () => {
  const zona = document.createElement('div')
  document.body.appendChild(zona)
  const user = userEvent.setup()
  render(<Lancar aberta origem={null} carroId={1} onFechar={() => {}} onLancado={() => {}} zonaToasts={zona} />)
  await screen.findByDisplayValue(/AA-12-BB/)
  await user.click(screen.getByRole('button', { name: 'Atualizar KM' }))
  await user.click(lancar())
  await waitFor(() => expect(zona.textContent).toContain('Lançado ✓'))
  zona.remove()
})