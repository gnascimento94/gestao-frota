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
  { nome: 'frota_2026-10-20_1842.db', data: '2026-10-20 18:42', tamanho: 4096, carros: 23, lancamentos: 412 },
  { nome: 'frota_2026-10-19.db', data: '2026-10-19', tamanho: 4096, carros: 23, lancamentos: 405 },
  { nome: 'frota_2026-10-18.db', data: '2026-10-18', tamanho: 4096, carros: -1, lancamentos: -1 },
]

beforeEach(() => {
  vi.resetAllMocks()
  m.listarVersoes.mockResolvedValue(versoes)
  m.abrirConsulta.mockResolvedValue(undefined)
  m.restaurarVersao.mockResolvedValue(undefined)
})

const App = () => <p>Aplicação aberta</p>

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
  m.arranque.mockResolvedValue({ estado: 'corrompido' })
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
  m.arranque.mockResolvedValue({ estado: 'corrompido' })
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
  m.arranque.mockResolvedValue({ estado: 'corrompido' })
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
