import { cleanup, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { StrictMode } from 'react'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import * as api from '../api/comandos'
import { VerificarAtualizacao } from './Atualizando'

vi.mock('../api/comandos')
const m = vi.mocked(api)

afterEach(cleanup)

beforeEach(() => {
  vi.resetAllMocks()
})

const App = () => <p>Aplicação aberta</p>

const montar = () =>
  render(
    <VerificarAtualizacao>
      <App />
    </VerificarAtualizacao>,
  )

test('sem atualização abre logo a aplicação', async () => {
  m.verificarAtualizacao.mockResolvedValue(null)
  montar()
  expect(await screen.findByText('Aplicação aberta')).toBeTruthy()
  expect(m.aplicarAtualizacao).not.toHaveBeenCalled()
})

test('erro ao verificar abre a aplicação sem aviso', async () => {
  m.verificarAtualizacao.mockRejectedValue({ codigo: 'io', mensagem: 'x' })
  montar()
  expect(await screen.findByText('Aplicação aberta')).toBeTruthy()
})

test('com atualização mostra o ecrã e aplica-a uma vez', async () => {
  m.verificarAtualizacao.mockResolvedValue({ versao: '1.3.0', notas: 'Melhorias' })
  m.aplicarAtualizacao.mockReturnValue(new Promise(() => {})) // reinicia: nunca volta
  // StrictMode corre os efeitos duas vezes: mesmo assim só um pedido
  render(
    <StrictMode>
      <VerificarAtualizacao>
        <App />
      </VerificarAtualizacao>
    </StrictMode>,
  )
  expect(await screen.findByText('A atualizar para a versão 1.3.0…')).toBeTruthy()
  expect(screen.getByRole('progressbar')).toBeTruthy()
  expect(screen.queryByText('Aplicação aberta')).toBeNull()
  await waitFor(() => expect(m.aplicarAtualizacao).toHaveBeenCalledTimes(1))
})

test('erro ao atualizar mostra a mensagem e permite continuar com a versão atual', async () => {
  m.verificarAtualizacao.mockResolvedValue({ versao: '1.3.0', notas: '' })
  m.aplicarAtualizacao.mockRejectedValue({
    codigo: 'atualizacao_invalida',
    mensagem: 'A atualização descarregada não passou a verificação de segurança. Continua a versão atual.',
  })
  montar()
  expect(
    await screen.findByText('A atualização descarregada não passou a verificação de segurança. Continua a versão atual.'),
  ).toBeTruthy()
  expect(screen.queryByText('Aplicação aberta')).toBeNull()
  await userEvent.click(screen.getByRole('button', { name: 'Continuar com a versão atual' }))
  expect(await screen.findByText('Aplicação aberta')).toBeTruthy()
  expect(m.aplicarAtualizacao).toHaveBeenCalledTimes(1)
})
