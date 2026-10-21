import { cleanup, render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import * as api from '../api/comandos'
import type { Versao } from '../api/tipos'
import { Arranque } from './Arranque'

vi.mock('../api/comandos')
const m = vi.mocked(api)

afterEach(cleanup)

const versoes: Versao[] = [
  { nome: 'frota_2026-10-20_1842.db', data: '2026-10-20 18:42', tamanho: 4096, carros: 23, lancamentos: 412, restauravel: true },
  { nome: 'frota_2026-10-19.db', data: '2026-10-19', tamanho: 4096, carros: 23, lancamentos: 405, restauravel: true },
  { nome: 'frota_2026-10-18.db', data: '2026-10-18', tamanho: 4096, carros: -1, lancamentos: -1, restauravel: true },
]

beforeEach(() => {
  vi.resetAllMocks()
  m.listarVersoes.mockResolvedValue(versoes)
  m.abrirConsulta.mockResolvedValue(undefined)
  m.restaurarVersao.mockResolvedValue(undefined)
})

const App = () => <p>Aplicação aberta</p>

const danificado = {
  estado: 'corrompido',
  motivo: 'danificado',
  mensagem: 'O ficheiro de dados parece estar danificado. Nada foi alterado.',
} as const
const vazio = {
  estado: 'corrompido',
  motivo: 'vazio',
  mensagem: 'O ficheiro de dados está vazio ou incompleto. Confirme que a pasta Frota está marcada como "Disponível offline".',
} as const

function montar() {
  return render(
    <Arranque>
      <App />
    </Arranque>,
  )
}

test('bloqueado mostra o nome do PC e permite abrir só para consulta', async () => {
  m.arranque.mockResolvedValue({ estado: 'bloqueado', pc: 'ESCRITORIO-2' })
  render(
    <Arranque>
      <App />
    </Arranque>,
  )
  expect(await screen.findByText('O sistema está aberto no PC «ESCRITORIO-2». Feche-o lá primeiro.')).toBeTruthy()
  expect(screen.queryByText('Aplicação aberta')).toBeNull()

  // "Tentar de novo" volta a perguntar ao núcleo
  await userEvent.click(screen.getByRole('button', { name: 'Tentar de novo' }))
  expect(m.arranque).toHaveBeenCalledTimes(2)
  expect(await screen.findByText(/aberto no PC «ESCRITORIO-2»/)).toBeTruthy()

  await userEvent.click(screen.getByRole('button', { name: 'Abrir só para consulta' }))
  expect(m.abrirConsulta).toHaveBeenCalledTimes(1)
  expect(await screen.findByText('Aplicação aberta')).toBeTruthy()
})

test('bloqueado por um PC desconhecido', async () => {
  m.arranque.mockResolvedValue({ estado: 'bloqueado', pc: 'desconhecido' })
  render(
    <Arranque>
      <App />
    </Arranque>,
  )
  expect(await screen.findByText('O sistema parece estar aberto noutro PC.')).toBeTruthy()
})

test('tentar de novo abre a aplicação quando o outro PC fechou', async () => {
  m.arranque
    .mockResolvedValueOnce({ estado: 'bloqueado', pc: 'PC-X' })
    .mockResolvedValueOnce({ estado: 'ok' })
  render(
    <Arranque>
      <App />
    </Arranque>,
  )
  await userEvent.click(await screen.findByRole('button', { name: 'Tentar de novo' }))
  expect(await screen.findByText('Aplicação aberta')).toBeTruthy()
  expect(m.abrirConsulta).not.toHaveBeenCalled()
})

test('ok abre logo a aplicação', async () => {
  m.arranque.mockResolvedValue({ estado: 'ok' })
  render(
    <Arranque>
      <App />
    </Arranque>,
  )
  expect(await screen.findByText('Aplicação aberta')).toBeTruthy()
})

test('corrompido oferece restaurar a versão mais recente', async () => {
  m.arranque.mockResolvedValue(danificado)
  render(
    <Arranque>
      <App />
    </Arranque>,
  )
  const lista = await screen.findByRole('list', { name: 'Cópias disponíveis' })
  const linhas = within(lista).getAllByRole('listitem')
  expect(linhas).toHaveLength(3)
  // a primeira é a mais recente e está assinalada
  expect(within(linhas[0]).getByText('Mais recente')).toBeTruthy()
  expect(within(linhas[0]).getByText(/20 out 2026, 18:42/)).toBeTruthy()
  expect(within(linhas[0]).getByText(/23 carros · 412 lançamentos/)).toBeTruthy()
  expect(within(linhas[0]).getByRole('button', { name: 'Restaurar esta' })).toBeTruthy()
  // contagens ilegíveis
  expect(within(linhas[2]).getByText(/não foi possível ler/)).toBeTruthy()
})

test('restaurar pede confirmação antes de chamar restaurarVersao', async () => {
  m.arranque.mockResolvedValue(danificado)
  const confirmar = vi.spyOn(window, 'confirm')
  render(
    <Arranque>
      <App />
    </Arranque>,
  )
  const lista = await screen.findByRole('list', { name: 'Cópias disponíveis' })
  const linhas = within(lista).getAllByRole('listitem')
  await userEvent.click(within(linhas[0]).getByRole('button', { name: 'Restaurar esta' }))
  expect(m.restaurarVersao).not.toHaveBeenCalled()
  expect(confirmar).not.toHaveBeenCalled()

  // cancelar não restaura
  await userEvent.click(screen.getByRole('button', { name: 'Cancelar' }))
  expect(m.restaurarVersao).not.toHaveBeenCalled()

  await userEvent.click(within(linhas[0]).getByRole('button', { name: 'Restaurar esta' }))
  const dialogo = screen.getByRole('alertdialog')
  expect(within(dialogo).getByText(/20 out 2026, 18:42/)).toBeTruthy()
  await userEvent.click(within(dialogo).getByRole('button', { name: 'Restaurar' }))
  expect(m.restaurarVersao).toHaveBeenCalledWith('frota_2026-10-20_1842.db')
  expect(await screen.findByText('Aplicação aberta')).toBeTruthy()
})

test('erro ao restaurar fica à vista e não abre a aplicação', async () => {
  m.arranque.mockResolvedValue(danificado)
  m.restaurarVersao.mockRejectedValue({
    codigo: 'corrompido',
    mensagem: 'A versão escolhida parece estar danificada. Nada foi alterado.',
  })
  render(
    <Arranque>
      <App />
    </Arranque>,
  )
  const lista = await screen.findByRole('list', { name: 'Cópias disponíveis' })
  await userEvent.click(within(lista).getAllByRole('button', { name: 'Restaurar esta' })[1])
  await userEvent.click(within(screen.getByRole('alertdialog')).getByRole('button', { name: 'Restaurar' }))
  expect(await screen.findByText('A versão escolhida parece estar danificada. Nada foi alterado.')).toBeTruthy()
  expect(screen.queryByText('Aplicação aberta')).toBeNull()
})

test('versão mais recente permite abrir só para consulta', async () => {
  m.arranque.mockResolvedValue({ estado: 'versaoMaisRecente' })
  render(
    <Arranque>
      <App />
    </Arranque>,
  )
  expect(await screen.findByText('Estes dados foram criados por uma versão mais recente do programa.')).toBeTruthy()
  expect(screen.queryByRole('button', { name: 'Tentar de novo' })).toBeNull()
  await userEvent.click(screen.getByRole('button', { name: 'Abrir só para consulta' }))
  expect(await screen.findByText('Aplicação aberta')).toBeTruthy()
})

test('sem pasta mostra a mensagem do núcleo e permite tentar de novo', async () => {
  m.arranque.mockResolvedValue({ estado: 'semPasta', mensagem: 'Não foi possível aceder à pasta de dados (G:\\Frota).' })
  render(
    <Arranque>
      <App />
    </Arranque>,
  )
  expect(await screen.findByText('Não foi possível aceder à pasta de dados (G:\\Frota).')).toBeTruthy()
  await userEvent.click(screen.getByRole('button', { name: 'Tentar de novo' }))
  expect(m.arranque).toHaveBeenCalledTimes(2)
  expect(screen.queryByRole('button', { name: 'Abrir só para consulta' })).toBeNull()
})

test('vazio: "Tentar de novo" é a ação principal e abre quando o ficheiro chega', async () => {
  m.arranque.mockResolvedValueOnce(vazio).mockResolvedValueOnce(vazio).mockResolvedValueOnce({ estado: 'ok' })
  montar()
  expect(await screen.findByText(/Disponível offline/)).toBeTruthy()
  expect(screen.getByText('O ficheiro dos dados ainda não está disponível neste PC.')).toBeTruthy()
  // restaurar fica em segundo plano: a lista não aparece logo
  expect(screen.queryByRole('list', { name: 'Cópias disponíveis' })).toBeNull()
  await userEvent.click(screen.getByRole('button', { name: 'Tentar de novo' }))
  expect(await screen.findByText(/Ainda não está disponível/)).toBeTruthy()
  await userEvent.click(screen.getByRole('button', { name: 'Tentar de novo' }))
  expect(await screen.findByText('Aplicação aberta')).toBeTruthy()
  expect(m.arranque).toHaveBeenCalledTimes(3)
  expect(m.restaurarVersao).not.toHaveBeenCalled()
})

test('vazio: restaurar continua possível como opção secundária', async () => {
  m.arranque.mockResolvedValue(vazio)
  montar()
  await userEvent.click(await screen.findByRole('button', { name: 'Restaurar uma cópia…' }))
  const lista = await screen.findByRole('list', { name: 'Cópias disponíveis' })
  expect(within(lista).getAllByRole('button', { name: 'Restaurar esta' })).toHaveLength(3)
  expect(screen.getByText(/a cópia pode ser mais antiga/)).toBeTruthy()
  expect(screen.getByRole('button', { name: 'Tentar de novo' })).toBeTruthy()
})

test('danificado: o ficheiro posto de lado aparece identificado e não é restaurável', async () => {
  m.arranque.mockResolvedValue(danificado)
  m.listarVersoes.mockResolvedValue([
    { nome: 'frota_danificado_2026-10-20_1900.db', data: '2026-10-20 19:00', tamanho: 4096, carros: -1, lancamentos: -1, restauravel: false },
    ...versoes,
  ])
  montar()
  const lista = await screen.findByRole('list', { name: 'Cópias disponíveis' })
  const linhas = within(lista).getAllByRole('listitem')
  expect(within(linhas[0]).getByText(/Ficheiro danificado guardado à parte/)).toBeTruthy()
  expect(within(linhas[0]).queryByRole('button')).toBeNull()
  // "Mais recente" fica na primeira cópia restaurável
  expect(within(linhas[1]).getByText('Mais recente')).toBeTruthy()
})

test('danificado sem cópias: não fica sem saída (tentar de novo + ajuda do Drive)', async () => {
  m.arranque.mockResolvedValue(danificado)
  m.listarVersoes.mockResolvedValueOnce([]).mockResolvedValueOnce(versoes)
  montar()
  expect(await screen.findByText('Não há cópias guardadas nesta pasta.')).toBeTruthy()
  expect(screen.getByText(/histórico de versões do ficheiro frota\.db/)).toBeTruthy()
  await userEvent.click(screen.getByRole('button', { name: 'Tentar de novo' }))
  expect(await screen.findByRole('list', { name: 'Cópias disponíveis' })).toBeTruthy()
  expect(m.listarVersoes).toHaveBeenCalledTimes(2)
})

test('danificado com a lista a falhar: mostra o erro, a ajuda e "Tentar de novo"', async () => {
  m.arranque.mockResolvedValue(danificado)
  m.listarVersoes.mockRejectedValueOnce({ codigo: 'io', mensagem: 'Acesso negado' }).mockResolvedValueOnce(versoes)
  montar()
  expect(await screen.findByText(/Não foi possível ler a lista de cópias: Acesso negado/)).toBeTruthy()
  expect(screen.getByText(/Gerir versões/)).toBeTruthy()
  await userEvent.click(screen.getByRole('button', { name: 'Tentar de novo' }))
  expect(await screen.findByRole('list', { name: 'Cópias disponíveis' })).toBeTruthy()
})