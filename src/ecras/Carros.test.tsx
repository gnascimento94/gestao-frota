import { cleanup, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import * as api from '../api/comandos'
import type { Carro, Relatorio } from '../api/tipos'
import { DesfazerProvider } from '../ui/Desfazer'
import { InicioOuBoasVindas } from './BoasVindas'
import { Carros } from './Carros'
import { FichaCarro } from './FichaCarro'

vi.mock('../api/comandos')
const m = vi.mocked(api)

function carro(id: number, matricula: string, extra: Partial<Carro> = {}): Carro {
  return {
    id,
    matricula,
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
    gastoAno: 12345,
    ...extra,
  }
}

const relatorioVazio: Relatorio = {
  resumo: { total: 0, porCategoria: {}, porMes: {}, porCarro: [], porPagar: 0 },
  porMotorista: [],
  porTipoManutencao: [],
  manutencao: { interna: 0, oficina: 0, pecas: 0, maoObra: 0, naoDiscriminado: 0, porOficina: [] },
  multas: { n: 0, total: 0, empresa: 0, colaboradores: 0, porReembolsar: 0, porInfracao: [], porMotorista: [] },
}

afterEach(cleanup)

beforeEach(() => {
  vi.resetAllMocks()
  m.carrosListar.mockResolvedValue([carro(1, 'AA-12-BB'), carro(2, 'CC-34-DD', { modelo: 'Megane' })])
  m.relatorio.mockResolvedValue(relatorioVazio)
  m.historico.mockResolvedValue([])
  m.atribuicoesListar.mockResolvedValue([])
  m.motoristasListar.mockResolvedValue([])
})

describe('Carros', () => {
  test("pesquisa encontra 'aa12' em AA-12-BB", async () => {
    render(<Carros ano={2026} pesquisa="aa12" onAbrir={() => {}} />)
    expect(await screen.findByText('AA-12-BB')).toBeTruthy()
    expect(screen.queryByText('CC-34-DD')).toBeNull()
  })

  test('matrícula duplicada mostra erro no campo e não fecha a folha', async () => {
    const user = userEvent.setup()
    m.carroGuardar.mockRejectedValue({ codigo: 'matricula_duplicada', mensagem: 'Esta matrícula já existe.' })
    render(<Carros ano={2026} pesquisa="" onAbrir={() => {}} />)
    await screen.findByText('AA-12-BB')
    await user.click(screen.getByRole('button', { name: 'Novo carro' }))
    await user.type(await screen.findByLabelText('Matrícula'), 'AA-12-BB')
    await user.click(screen.getByRole('button', { name: 'Guardar' }))
    const erro = await screen.findByText('Esta matrícula já existe.')
    expect(screen.getByLabelText('Matrícula').getAttribute('aria-invalid')).toBe('true')
    expect(erro.id).toBe(screen.getByLabelText('Matrícula').getAttribute('aria-describedby'))
    expect(screen.getByRole('dialog')).toBeTruthy()
  })
})

describe('FichaCarro', () => {
  test('apagar carro com histórico propõe marcar como vendido', async () => {
    const user = userEvent.setup()
    m.carroObter.mockResolvedValue(carro(1, 'AA-12-BB'))
    m.carroApagar.mockRejectedValue({
      codigo: 'tem_historico',
      mensagem: 'Este carro tem histórico e não pode ser apagado.',
    })
    m.carroGuardar.mockResolvedValue(1)
    render(
      <DesfazerProvider aoFecharJanela={() => () => {}}>
        <FichaCarro id={1} ano={2026} onNavegar={() => {}} onLancar={() => {}} />
      </DesfazerProvider>,
    )
    await screen.findByRole('heading', { name: /AA-12-BB/ })
    await user.click(screen.getByRole('button', { name: 'Apagar' }))
    expect(await screen.findByText('Este carro tem histórico e não pode ser apagado.')).toBeTruthy()
    await user.click(screen.getByRole('button', { name: 'Marcar como vendido' }))
    await waitFor(() =>
      expect(m.carroGuardar).toHaveBeenCalledWith(expect.objectContaining({ id: 1, estado: 'vendido' })),
    )
  })
})

describe('BoasVindas', () => {
  test('lista vazia mostra boas-vindas', async () => {
    m.carrosListar.mockResolvedValue([])
    render(
      <InicioOuBoasVindas ano={2026} onNavegar={() => {}}>
        <p>Conteúdo do início</p>
      </InicioOuBoasVindas>,
    )
    expect(await screen.findByText('Vamos registar o primeiro carro')).toBeTruthy()
    expect(screen.queryByText('Conteúdo do início')).toBeNull()
  })
})
