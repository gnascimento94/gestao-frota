import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, test } from 'vitest'
import type { Categoria } from '../api/tipos'
import { escala } from './escala'
import { Grafico } from './Grafico'

afterEach(cleanup)

const cores: Record<Categoria, string> = {
  manutencao: 'var(--cat-manutencao)',
  pneus: 'var(--cat-pneus)',
  iuc: 'var(--cat-iuc)',
  inspecao: 'var(--cat-inspecao)',
  seguro: 'var(--cat-seguro)',
  multa: 'var(--cat-multa)',
  outro: 'var(--cat-outro)',
}

describe('Grafico', () => {
  test('gráfico desenha um grupo por mês incluindo meses a zero', () => {
    const { container } = render(
      <Grafico
        meses={[
          { mes: '2026-01', valores: { manutencao: 10000 } },
          { mes: '2026-02', valores: {} },
          { mes: '2026-03', valores: { manutencao: 2000, pneus: 5000 } },
        ]}
        cores={cores}
      />,
    )
    const grupos = container.querySelectorAll('g[data-mes]')
    expect(grupos).toHaveLength(3)
    expect([...grupos].map((g) => g.getAttribute('data-mes'))).toEqual(['2026-01', '2026-02', '2026-03'])
    // o mês sem gasto existe, mas sem segmentos
    expect(grupos[1].querySelectorAll('[data-categoria]')).toHaveLength(0)
    expect(grupos[2].querySelectorAll('[data-categoria]')).toHaveLength(2)
    // legenda com as categorias presentes
    expect(screen.getByText('Manutenção')).toBeTruthy()
    expect(screen.getByText('Pneus')).toBeTruthy()
  })

  test('escala arredonda o máximo 1 234,56 € para 1 500 €', () => {
    const e = escala(123456)
    expect(e.maximo).toBe(150000)
    expect(e.marcas).toEqual([0, 50000, 100000, 150000])
    // passos de 1, 2 ou 5 × 10ⁿ
    expect(escala(90000).marcas).toEqual([0, 50000, 100000, 150000])
    expect(escala(50000).maximo).toBe(60000)
    expect(escala(0).maximo).toBeGreaterThan(0)
    render(<Grafico meses={[{ mes: '2026-01', valores: { manutencao: 123456 } }]} cores={cores} />)
    expect(screen.getByText(/^1\s500 €$/)).toBeTruthy()
  })

  test('tooltip ao passar mostra o mês e os valores', () => {
    const { container } = render(
      <Grafico meses={[{ mes: '2026-03', valores: { manutencao: 2000, pneus: 5000 } }]} cores={cores} />,
    )
    fireEvent.mouseEnter(container.querySelector('g[data-mes="2026-03"]')!)
    const dica = screen.getByRole('tooltip')
    expect(dica.textContent).toContain('março 2026')
    expect(dica.textContent).toContain('50,00 €')
    expect(dica.textContent).toContain('70,00 €')
  })
})
