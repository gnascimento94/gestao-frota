import { cleanup, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import * as api from '../api/comandos'
import type { EstadoArmazenamento } from '../api/tipos'
import { Aviso, AvisoArranqueDispensavel } from './Aviso'

vi.mock('../api/comandos')
const m = vi.mocked(api)

afterEach(cleanup)
beforeEach(() => vi.resetAllMocks())

const TEXTO = 'Os dados foram alterados noutro PC. As alterações feitas neste PC foram guardadas em Backup.'

test('fechar o aviso esconde-o logo e chama dispensarAviso', async () => {
  m.dispensarAviso.mockResolvedValue(undefined)
  render(<AvisoArranqueDispensavel texto={TEXTO} />)
  expect(screen.getByRole('alert').textContent).toContain(TEXTO)
  await userEvent.click(screen.getByRole('button', { name: 'Fechar aviso' }))
  expect(m.dispensarAviso).toHaveBeenCalledTimes(1)
  expect(screen.queryByText(TEXTO)).toBeNull()
})

test('se dispensarAviso falhar, o aviso volta com o erro à vista', async () => {
  m.dispensarAviso.mockRejectedValue({ codigo: 'io', mensagem: 'Sem ligação ao núcleo' })
  render(<AvisoArranqueDispensavel texto={TEXTO} />)
  await userEvent.click(screen.getByRole('button', { name: 'Fechar aviso' }))
  expect(await screen.findByText('Não foi possível fechar o aviso: Sem ligação ao núcleo')).toBeTruthy()
  expect(screen.getByText(TEXTO, { exact: false })).toBeTruthy()
})

const estado = (extra: Partial<EstadoArmazenamento> = {}): EstadoArmazenamento => ({
  pasta: 'C:\\Frota',
  noGoogleDrive: true,
  ultimaGravacao: '2026-10-21T10:00:00',
  erro: null,
  notaGravacao: null,
  avisoArranque: null,
  soConsulta: false,
  versaoApp: '0.1.1',
  ...extra,
})

const NOTA =
  'Os dados foram guardados na pasta, mas não foi possível apagar o marcador de alterações por gravar neste computador (em uso).'

test('a nota de gravação é informativa e não aparece como "Não foi possível guardar"', () => {
  render(<Aviso estado={estado({ notaGravacao: NOTA })} falhou={false} />)
  const banner = screen.getByRole('status')
  expect(banner.className).toContain('aviso-info')
  expect(banner.textContent).toContain('Dados guardados.')
  expect(banner.textContent).toContain(NOTA)
  expect(screen.queryByText(/Não foi possível guardar/)).toBeNull()
  expect(screen.queryByRole('alert')).toBeNull()
})

test('um erro de gravação continua a ser erro (e tem prioridade sobre a nota)', () => {
  render(<Aviso estado={estado({ erro: 'Acesso negado', notaGravacao: NOTA })} falhou={false} />)
  const banner = screen.getByRole('alert')
  expect(banner.textContent).toContain('Não foi possível guardar na pasta da frota.')
  expect(banner.textContent).not.toContain(NOTA)
})

test('sem erro nem nota não mostra nada', () => {
  const { container } = render(<Aviso estado={estado()} falhou={false} />)
  expect(container.textContent).toBe('')
})

test('um aviso novo depois de fechado volta a aparecer', async () => {
  m.dispensarAviso.mockResolvedValue(undefined)
  const { rerender } = render(<AvisoArranqueDispensavel texto={TEXTO} />)
  await userEvent.click(screen.getByRole('button', { name: 'Fechar aviso' }))
  rerender(<AvisoArranqueDispensavel texto="Outro aviso" />)
  expect(screen.getByText('Outro aviso')).toBeTruthy()
})