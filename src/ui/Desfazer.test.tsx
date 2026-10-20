import { act, cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import { Desfazer, DesfazerProvider, useDesfazer } from './Desfazer'

function Apagar({ acao, rotulo = 'Apagado' }: { acao: () => Promise<void>; rotulo?: string }) {
  const { agendar } = useDesfazer()
  return <button onClick={() => agendar(rotulo, acao)}>Apagar</button>
}

function montar(acao: () => Promise<void>, aoFecharJanela?: (f: () => Promise<void>) => () => void) {
  return render(
    <DesfazerProvider aoFecharJanela={aoFecharJanela}>
      <Apagar acao={acao} />
      <Desfazer />
    </DesfazerProvider>,
  )
}

beforeEach(() => {
  vi.useFakeTimers()
})

afterEach(() => {
  cleanup()
  vi.useRealTimers()
})

test('desfazer cancela a ação', async () => {
  const acao = vi.fn(() => Promise.resolve())
  montar(acao)
  fireEvent.click(screen.getByText('Apagar'))
  expect(screen.getByRole('status').textContent).toContain('Apagado')
  fireEvent.click(screen.getByRole('button', { name: 'Desfazer' }))
  await act(async () => {
    vi.advanceTimersByTime(9000)
  })
  expect(acao).not.toHaveBeenCalled()
  expect(screen.queryByRole('button', { name: 'Desfazer' })).toBeNull()
})

test('sem desfazer a ação corre aos 8 s', async () => {
  const acao = vi.fn(() => Promise.resolve())
  montar(acao)
  fireEvent.click(screen.getByText('Apagar'))
  await act(async () => {
    vi.advanceTimersByTime(7900)
  })
  expect(acao).not.toHaveBeenCalled()
  await act(async () => {
    vi.advanceTimersByTime(200)
  })
  expect(acao).toHaveBeenCalledTimes(1)
  // O toast pode ainda estar a animar a saída, mas já não oferece "Desfazer".
  expect(screen.queryByRole('button', { name: 'Desfazer' })).toBeNull()
  await act(async () => {
    vi.advanceTimersByTime(20000)
  })
  expect(acao).toHaveBeenCalledTimes(1)
})

test('beforeunload executa pendentes de imediato', async () => {
  const acao = vi.fn(() => Promise.resolve())
  montar(acao)
  fireEvent.click(screen.getByText('Apagar'))
  await act(async () => {
    window.dispatchEvent(new Event('beforeunload'))
  })
  expect(acao).toHaveBeenCalledTimes(1)
  await act(async () => {
    vi.advanceTimersByTime(9000)
  })
  expect(acao).toHaveBeenCalledTimes(1)
})

test('fecho da janela executa pendentes e espera por elas', async () => {
  const acao = vi.fn(() => Promise.resolve())
  let fechar: (() => Promise<void>) | null = null
  const desligar = vi.fn()
  montar(acao, (f) => {
    fechar = f
    return desligar
  })
  fireEvent.click(screen.getByText('Apagar'))
  fireEvent.click(screen.getByText('Apagar'))
  expect(fechar).not.toBeNull()
  await act(async () => {
    await fechar!()
  })
  expect(acao).toHaveBeenCalledTimes(2)
  cleanup()
  expect(desligar).toHaveBeenCalled()
})

test('uma ação que falha não impede as outras no fecho', async () => {
  const falha = vi.fn(() => Promise.reject(new Error('x')))
  const ok = vi.fn(() => Promise.resolve())
  let fechar: (() => Promise<void>) | null = null
  render(
    <DesfazerProvider aoFecharJanela={(f) => ((fechar = f), () => {})}>
      <Apagar acao={falha} rotulo="A" />
      <Apagar acao={ok} rotulo="B" />
      <Desfazer />
    </DesfazerProvider>,
  )
  const [a, b] = screen.getAllByText('Apagar')
  fireEvent.click(a)
  fireEvent.click(b)
  expect(screen.getByRole('status').textContent).toContain('B')
  await act(async () => {
    await fechar!()
  })
  expect(falha).toHaveBeenCalledTimes(1)
  expect(ok).toHaveBeenCalledTimes(1)
})
