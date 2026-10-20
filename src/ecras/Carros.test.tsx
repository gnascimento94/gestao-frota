import { cleanup, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import * as api from '../api/comandos'
import type { Carro, HistoricoItem, Motorista, MultaItem, Relatorio } from '../api/tipos'
import { DesfazerProvider } from '../ui/Desfazer'
import { InicioOuBoasVindas } from './BoasVindas'
import { Carros } from './Carros'
import { campoDoErro, chaveDoErro, paraErroApi } from './comum'
import { Motoristas } from './Motoristas'
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
    gastoTotal: 99999,
    ...extra,
  }
}

const relatorioVazio: Relatorio = {
  resumo: { total: 0, porCategoria: {}, porMes: {}, totalPorMes: {}, porCarro: [], porPagar: 0 },
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

function item(extra: Partial<HistoricoItem>): HistoricoItem {
  return {
    tipo: 'documento',
    id: 7,
    carroId: 1,
    matricula: 'AA-12-BB',
    data: '2026-10-01',
    km: null,
    titulo: 'IUC',
    valor: 15000,
    detalhe: null,
    porPagar: false,
    vencimento: null,
    motoristaId: null,
    estadoMulta: null,
    ...extra,
  }
}

function montarFicha(onLancar = vi.fn()) {
  m.carroObter.mockResolvedValue(carro(1, 'AA-12-BB', { ano: 2019, cor: 'Azul', notas: 'Carrinha da loja' }))
  render(
    <DesfazerProvider aoFecharJanela={() => () => {}}>
      <FichaCarro id={1} ano={2026} onNavegar={() => {}} onLancar={onLancar} />
    </DesfazerProvider>,
  )
  return onLancar
}

describe('FichaCarro (campos estruturados do núcleo)', () => {
  test('documento por pagar mostra prazo e paga com a data escolhida', async () => {
    const user = userEvent.setup()
    m.historico.mockResolvedValue([item({ porPagar: true, vencimento: '2027-05-31' })])
    m.pagarDocumento.mockResolvedValue(null)
    montarFicha()
    expect(await screen.findByText(/vence a 31 mai 2027/)).toBeTruthy()
    await user.click(screen.getByRole('button', { name: 'Marcar como pago' }))
    await waitFor(() => expect(m.pagarDocumento).toHaveBeenCalledWith(7, expect.stringMatching(/^\d{4}-\d{2}-\d{2}$/)))
  })

  test('multa mostra o estado vindo do núcleo e permite marcar reembolsado', async () => {
    const user = userEvent.setup()
    m.historico.mockResolvedValue([
      item({ tipo: 'multa', id: 9, titulo: 'Multa (velocidade)', estadoMulta: 'porReembolsar', motoristaId: 3 }),
      item({ tipo: 'multa', id: 10, titulo: 'Multa (cinto)', estadoMulta: 'porIdentificarCondutor' }),
    ])
    m.marcarReembolsado.mockResolvedValue(undefined)
    montarFicha()
    expect(await screen.findByText('Por identificar condutor')).toBeTruthy()
    expect(screen.getAllByRole('button', { name: 'Marcar reembolsado' })).toHaveLength(1)
    await user.click(screen.getByRole('button', { name: 'Marcar reembolsado' }))
    await waitFor(() => expect(m.marcarReembolsado).toHaveBeenCalledWith(9, expect.any(String)))
  })

  test('mostra ano, cor e notas e passa o id do carro a + Lançar', async () => {
    const user = userEvent.setup()
    const onLancar = montarFicha()
    expect(await screen.findByText('Carrinha da loja')).toBeTruthy()
    expect(screen.getByText('Azul')).toBeTruthy()
    expect(screen.getByText('2019')).toBeTruthy()
    await user.click(screen.getByRole('button', { name: '+ Lançar' }))
    expect(onLancar).toHaveBeenCalledWith(expect.anything(), 1)
  })
})

describe('Carros: uma linha, um elemento interativo', () => {
  test('cada linha tem um único botão', async () => {
    render(<Carros ano={2026} pesquisa="" onAbrir={() => {}} />)
    await screen.findByText('AA-12-BB')
    const linhas = screen.getAllByRole('row').slice(1)
    expect(linhas).toHaveLength(2)
    for (const l of linhas) expect(l.querySelectorAll('button')).toHaveLength(1)
  })
})

describe('mapa de erros', () => {
  test('km_invalido e data_saida_invalida têm campo próprio', () => {
    expect(campoDoErro('km_invalido')).toBe('km')
    expect(campoDoErro('km_abaixo')).toBe('km')
    expect(campoDoErro('data_saida_invalida')).toBe('dataSaida')
    expect(campoDoErro('data_invalida')).toBe('data')
    expect(campoDoErro('referencia_invalida')).toBe('geral')
  })

  test('o campo indicado pelo núcleo tem prioridade sobre o mapa por código', () => {
    const e = paraErroApi({ codigo: 'data_futura', mensagem: 'x', campo: 'pagoEm' })
    expect(e.campo).toBe('pagoEm')
    expect(chaveDoErro(e)).toBe('pagoEm')
    expect(chaveDoErro(paraErroApi({ codigo: 'data_futura', mensagem: 'x' }))).toBe('data')
  })
})

describe('erros de carregamento visíveis', () => {
  test('boas-vindas mostra o erro em vez de ecrã em branco', async () => {
    m.carrosListar.mockRejectedValue({ codigo: 'io', mensagem: 'Não foi possível ler os dados.' })
    render(
      <InicioOuBoasVindas ano={2026} onNavegar={() => {}}>
        <p>x</p>
      </InicioOuBoasVindas>,
    )
    expect(await screen.findByText('Não foi possível ler os dados.')).toBeTruthy()
  })
})

describe('Motoristas', () => {
  const ana: Motorista = {
    id: 3,
    nome: 'Ana',
    telefone: null,
    cartaNumero: null,
    cartaValidade: '2026-11-01',
    ativo: true,
    notas: null,
    alertaCarta: 'proximo',
  }
  const multa: MultaItem = {
    id: 9,
    carroId: 1,
    matricula: 'AA-12-BB',
    dataInfracao: '2026-09-01',
    infracao: 'velocidade',
    pontos: 2,
    valor: 12000,
    valorEmpresa: 4000,
    valorColaborador: 8000,
    estadoMulta: 'porReembolsar',
    motoristaId: 3,
    vencimento: null,
    descricao: null,
  }

  test('alerta da carta e multas por reembolsar vêm do núcleo', async () => {
    const user = userEvent.setup()
    m.motoristasListar.mockResolvedValue([ana])
    m.multasMotorista.mockResolvedValue({ n: 2, total: 12000, empresa: 4000, colaborador: 8000, porReembolsar: 8000, pontos3Anos: 2 })
    m.multasListar.mockResolvedValue([multa, { ...multa, id: 10, estadoMulta: 'fechada' }])
    m.marcarReembolsado.mockResolvedValue(undefined)
    render(<Motoristas ano={2026} />)
    expect(await screen.findByText('Carta a vencer')).toBeTruthy()
    await user.click(screen.getByRole('button', { name: /Ana/ }))
    expect(m.multasListar).toHaveBeenCalledWith(null, 3)
    const botoes = await screen.findAllByRole('button', { name: 'Marcar reembolsado' })
    expect(botoes).toHaveLength(1)
    await user.click(botoes[0])
    await waitFor(() => expect(m.marcarReembolsado).toHaveBeenCalledWith(9, expect.any(String)))
  })

  test('`inicial` abre esse motorista, mesmo inativo', async () => {
    const rui: Motorista = { ...ana, id: 5, nome: 'Rui', ativo: false, alertaCarta: null }
    m.motoristasListar.mockImplementation(async (soAtivos) => (soAtivos ? [ana] : [ana, rui]))
    m.multasMotorista.mockResolvedValue({ n: 0, total: 0, empresa: 0, colaborador: 0, porReembolsar: 0, pontos3Anos: 0 })
    m.multasListar.mockResolvedValue([])
    render(<Motoristas ano={2026} inicial={5} />)
    expect(await screen.findByRole('heading', { name: 'Rui' })).toBeTruthy()
    expect((screen.getByLabelText('Mostrar inativos') as HTMLInputElement).checked).toBe(true)
    render(<Motoristas ano={2026} inicial={3} />)
    expect(await screen.findByRole('heading', { name: 'Ana' })).toBeTruthy()
  })

  test('erro ao carregar a ficha aparece num aviso', async () => {
    const user = userEvent.setup()
    m.motoristasListar.mockResolvedValue([ana])
    m.relatorio.mockRejectedValue({ codigo: 'io', mensagem: 'Falhou a leitura do relatório.' })
    m.multasMotorista.mockResolvedValue({ n: 0, total: 0, empresa: 0, colaborador: 0, porReembolsar: 0, pontos3Anos: 0 })
    m.multasListar.mockResolvedValue([])
    render(<Motoristas ano={2026} />)
    await user.click(await screen.findByRole('button', { name: /Ana/ }))
    expect(await screen.findByText('Falhou a leitura do relatório.')).toBeTruthy()
  })
})

describe('recarregar após lançar (versao)', () => {
  test('versao nova volta a pedir os carros sem perder o filtro', async () => {
    const user = userEvent.setup()
    const { rerender } = render(<Carros ano={2026} versao={0} pesquisa="" onAbrir={() => {}} />)
    await screen.findByText('AA-12-BB')
    await user.click(screen.getByLabelText('Mostrar inativos/vendidos'))
    await waitFor(() => expect(m.carrosListar).toHaveBeenLastCalledWith(false, 2026))
    const chamadas = m.carrosListar.mock.calls.length
    m.carrosListar.mockResolvedValue([carro(1, 'AA-12-BB'), carro(3, 'EE-56-FF')])
    rerender(<Carros ano={2026} versao={1} pesquisa="" onAbrir={() => {}} />)
    expect(await screen.findByText('EE-56-FF')).toBeTruthy()
    expect(m.carrosListar.mock.calls.length).toBe(chamadas + 1)
    expect(m.carrosListar).toHaveBeenLastCalledWith(false, 2026)
    expect((screen.getByLabelText('Mostrar inativos/vendidos') as HTMLInputElement).checked).toBe(true)
  })

  test('na ficha, versao nova recarrega e mantém o separador escolhido', async () => {
    const user = userEvent.setup()
    m.carroObter.mockResolvedValue(carro(1, 'AA-12-BB'))
    const ficha = (versao: number) => (
      <DesfazerProvider aoFecharJanela={() => () => {}}>
        <FichaCarro id={1} ano={2026} versao={versao} onNavegar={() => {}} onLancar={() => {}} />
      </DesfazerProvider>
    )
    const { rerender } = render(ficha(0))
    await screen.findByRole('heading', { name: /AA-12-BB/ })
    const separador = screen.getAllByRole('tab')[1]
    await user.click(separador)
    expect(separador.getAttribute('aria-selected')).toBe('true')
    const chamadas = m.carroObter.mock.calls.length
    rerender(ficha(1))
    await waitFor(() => expect(m.carroObter.mock.calls.length).toBe(chamadas + 1))
    expect(screen.getAllByRole('tab')[1].getAttribute('aria-selected')).toBe('true')
  })
})