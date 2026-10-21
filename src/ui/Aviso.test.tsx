import { cleanup, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import * as api from '../api/comandos'
import { AvisoArranqueDispensavel } from './Aviso'

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

test('um aviso novo depois de fechado volta a aparecer', async () => {
  m.dispensarAviso.mockResolvedValue(undefined)
  const { rerender } = render(<AvisoArranqueDispensavel texto={TEXTO} />)
  await userEvent.click(screen.getByRole('button', { name: 'Fechar aviso' }))
  rerender(<AvisoArranqueDispensavel texto="Outro aviso" />)
  expect(screen.getByText('Outro aviso')).toBeTruthy()
})