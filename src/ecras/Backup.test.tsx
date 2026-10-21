import { cleanup, render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { useState } from 'react'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import * as api from '../api/comandos'
import type { EstadoArmazenamento, Versao } from '../api/tipos'
import { Backup } from './Backup'

vi.mock('../api/comandos')
const m = vi.mocked(api)

afterEach(cleanup)

const dois = (n: number) => String(n).padStart(2, '0')
const d = new Date()
const hojeIso = `${d.getFullYear()}-${dois(d.getMonth() + 1)}-${dois(d.getDate())}`

const versoes: Versao[] = [
  { nome: 'frota_x_1842.db', data: `${hojeIso} 18:42`, tamanho: 1, carros: 23, lancamentos: 412 },
  { nome: 'frota_2020-01-01.db', data: '2020-01-01', tamanho: 1, carros: -1, lancamentos: -1 },
]

const estado = (extra: Partial<EstadoArmazenamento> = {}): EstadoArmazenamento => ({
  pasta: 'C:\\Frota',
  noGoogleDrive: false,
  ultimaGravacao: null,
  erro: null,
  avisoArranque: null,
  soConsulta: false,
  versaoApp: '0.1.0',
  ...extra,
})

beforeEach(() => {
  vi.resetAllMocks()
  m.listarVersoes.mockResolvedValue(versoes)
  m.fazerCopia.mockResolvedValue(undefined)
  m.restaurarVersao.mockResolvedValue(undefined)
})

test('mostra a última cópia, a pasta, o aviso fora do Drive e a versão', async () => {
  render(<Backup estado={estado()} onRestaurado={() => {}} />)
  expect(await screen.findByText(/Última cópia: hoje, 18:42/)).toBeTruthy()
  expect(screen.getByText('C:\\Frota')).toBeTruthy()
  expect(screen.getByText('Esta pasta não está no Google Drive — os dados não têm cópia na nuvem.')).toBeTruthy()
  expect(screen.getByText(/versão 0\.1\.0/)).toBeTruthy()
  expect(screen.getByText(/não foi possível ler/)).toBeTruthy()
})

test('no Google Drive não há aviso; o erro de gravação aparece junto da última cópia', async () => {
  render(<Backup estado={estado({ noGoogleDrive: true, erro: 'Acesso negado' })} onRestaurado={() => {}} />)
  expect((await screen.findByRole('alert')).textContent).toBe('Acesso negado')
  expect(await screen.findByText(/Última cópia: hoje, 18:42/)).toBeTruthy()
  expect(screen.queryByText(/não está no Google Drive/)).toBeNull()
})

test('a última cópia ignora o ficheiro danificado posto de lado', async () => {
  m.listarVersoes.mockResolvedValue([
    { nome: 'frota_danificado_2030-01-01_0900.db', data: '2030-01-01 09:00', tamanho: 1, carros: -1, lancamentos: -1 },
    ...versoes,
  ])
  render(<Backup estado={estado()} onRestaurado={() => {}} />)
  expect(await screen.findByText(/Última cópia: hoje, 18:42/)).toBeTruthy()
  expect(screen.getByText(/Ficheiro danificado guardado à parte/)).toBeTruthy()
})

test('depois de restaurar a lista é pedida uma só vez', async () => {
  function ComApp() {
    const [versao, setVersao] = useState(0)
    return <Backup estado={estado()} versao={versao} onRestaurado={() => setVersao((v) => v + 1)} />
  }
  render(<ComApp />)
  const lista = await screen.findByRole('list', { name: 'Cópias disponíveis' })
  expect(m.listarVersoes).toHaveBeenCalledTimes(1)
  await userEvent.click(within(lista).getAllByRole('button', { name: 'Restaurar…' })[0])
  await userEvent.click(within(screen.getByRole('alertdialog')).getByRole('button', { name: 'Restaurar' }))
  expect(await screen.findByText(/restaurada ✓/)).toBeTruthy()
  expect(m.listarVersoes).toHaveBeenCalledTimes(2)
})

test('fazer cópia agora e restaurar com confirmação', async () => {
  const onRestaurado = vi.fn()
  render(<Backup estado={estado()} onRestaurado={onRestaurado} />)
  await userEvent.click(await screen.findByRole('button', { name: 'Fazer cópia agora' }))
  expect(m.fazerCopia).toHaveBeenCalledTimes(1)

  const lista = screen.getByRole('list', { name: 'Cópias disponíveis' })
  await userEvent.click(within(lista).getAllByRole('button', { name: 'Restaurar…' })[0])
  expect(m.restaurarVersao).not.toHaveBeenCalled()
  const dialogo = screen.getByRole('alertdialog')
  expect(within(dialogo).getByText('O estado atual é guardado antes de restaurar.')).toBeTruthy()
  await userEvent.click(within(dialogo).getByRole('button', { name: 'Restaurar' }))
  expect(m.restaurarVersao).toHaveBeenCalledWith('frota_x_1842.db')
  expect(onRestaurado).toHaveBeenCalledTimes(1)
})

test('só para consulta desativa as ações', async () => {
  render(<Backup estado={estado({ soConsulta: true })} onRestaurado={() => {}} />)
  const lista = await screen.findByRole('list', { name: 'Cópias disponíveis' })
  expect((screen.getByRole('button', { name: 'Fazer cópia agora' }) as HTMLButtonElement).disabled).toBe(true)
  for (const b of within(lista).getAllByRole('button', { name: 'Restaurar…' }))
    expect((b as HTMLButtonElement).disabled).toBe(true)
})
