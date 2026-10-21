import { act, cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import { Desfazer, DesfazerProvider } from './Desfazer'
import { useDesfazer } from './useDesfazer'

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
  expect(screen.getByRole('status').textContent).toContain('Apagados (2)')
  await act(async () => {
    await fechar!()
  })
  expect(falha).toHaveBeenCalledTimes(1)
  expect(ok).toHaveBeenCalledTimes(1)
})

test('o fecho espera pelas ações que já estavam a correr', async () => {
  let terminar: () => void = () => {}
  const acao = vi.fn(() => new Promise<void>((r) => (terminar = r)))
  let fechar: (() => Promise<void>) | null = null
  montar(acao, (f) => ((fechar = f), () => {}))
  fireEvent.click(screen.getByText('Apagar'))
  await act(async () => {
    vi.advanceTimersByTime(8100) // o temporizador iniciou a ação; ainda não terminou
  })
  expect(acao).toHaveBeenCalledTimes(1)
  let fechou = false
  const p = fechar!().then(() => {
    fechou = true
  })
  await act(async () => {
    vi.advanceTimersByTime(1000)
  })
  expect(fechou).toBe(false)
  await act(async () => {
    terminar()
    await p
  })
  expect(fechou).toBe(true)
  expect(acao).toHaveBeenCalledTimes(1)
})

test('o fecho não espera mais de 4 s por uma ação que não termina', async () => {
  const acao = vi.fn(() => new Promise<void>(() => {}))
  let fechar: (() => Promise<void>) | null = null
  montar(acao, (f) => ((fechar = f), () => {}))
  fireEvent.click(screen.getByText('Apagar'))
  let fechou = false
  void fechar!().then(() => {
    fechou = true
  })
  await act(async () => {
    vi.advanceTimersByTime(3900)
  })
  expect(acao).toHaveBeenCalledTimes(1)
  expect(fechou).toBe(false)
  await act(async () => {
    vi.advanceTimersByTime(200)
  })
  expect(fechou).toBe(true)
})

test('várias pendentes: "Apagados (N)" e desfazer cancela todas', async () => {
  const a = vi.fn(() => Promise.resolve())
  const b = vi.fn(() => Promise.resolve())
  render(
    <DesfazerProvider aoFecharJanela={() => () => {}}>
      <Apagar acao={a} />
      <Apagar acao={b} />
      <Desfazer />
    </DesfazerProvider>,
  )
  const [x, y] = screen.getAllByText('Apagar')
  fireEvent.click(x)
  expect(screen.getByRole('status').textContent).toContain('Apagado')
  fireEvent.click(y)
  expect(screen.getByRole('status').textContent).toContain('Apagados (2)')
  fireEvent.click(screen.getByRole('button', { name: 'Desfazer' }))
  await act(async () => {
    vi.advanceTimersByTime(9000)
  })
  expect(a).not.toHaveBeenCalled()
  expect(b).not.toHaveBeenCalled()
})

test('Ctrl+Z desfaz enquanto o toast está visível', async () => {
  const acao = vi.fn(() => Promise.resolve())
  montar(acao)
  fireEvent.click(screen.getByText('Apagar'))
  fireEvent.keyDown(window, { key: 'z', ctrlKey: true })
  await act(async () => {
    vi.advanceTimersByTime(9000)
  })
  expect(acao).not.toHaveBeenCalled()
})

test('Ctrl+Z num campo de texto não desfaz o apagamento', async () => {
  const acao = vi.fn(() => Promise.resolve())
  render(
    <DesfazerProvider aoFecharJanela={() => () => {}}>
      <Apagar acao={acao} />
      <input aria-label="texto" />
      <Desfazer />
    </DesfazerProvider>,
  )
  fireEvent.click(screen.getByText('Apagar'))
  fireEvent.keyDown(screen.getByLabelText('texto'), { key: 'z', ctrlKey: true })
  await act(async () => {
    vi.advanceTimersByTime(9000)
  })
  expect(acao).toHaveBeenCalledTimes(1)
})

test('uma ação que falha depois dos 8 s mostra a mensagem do núcleo e pede para recarregar', async () => {
  const acao = vi.fn(() => Promise.reject({ codigo: 'bd', mensagem: 'Registo em uso.' }))
  const aoFalhar = vi.fn()
  render(
    <DesfazerProvider aoFecharJanela={() => () => {}} aoFalhar={aoFalhar}>
      <Apagar acao={acao} rotulo="Apagado: X" />
      <Desfazer />
    </DesfazerProvider>,
  )
  fireEvent.click(screen.getByText('Apagar'))
  await act(async () => {
    vi.advanceTimersByTime(8100)
  })
  expect(acao).toHaveBeenCalledTimes(1)
  expect(screen.getByRole('alert').textContent).toContain('Registo em uso.')
  expect(aoFalhar).toHaveBeenCalledTimes(1)
  fireEvent.click(screen.getByRole('button', { name: 'Fechar' }))
  expect(screen.queryByRole('alert')).toBeNull()
})