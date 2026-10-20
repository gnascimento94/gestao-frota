// Escala do eixo Y do gráfico de barras (só apresentação: os valores vêm do núcleo).

export const N_MARCAS = 4

/**
 * 4 marcas (0 incluído) com passo 1, 2 ou 5 × 10ⁿ cêntimos — o menor que cobre o máximo.
 * `123456` (1 234,56 €) → passo 50 000 → máximo 150 000 (1 500 €). Nunca menos de 1 € por passo.
 */
export function escala(maximoCentimos: number): { maximo: number; marcas: number[] } {
  const bruto = Math.max(maximoCentimos, 1) / (N_MARCAS - 1)
  let potencia = 10 ** Math.floor(Math.log10(bruto))
  let passo = 0
  while (!passo) {
    passo = [1, 2, 5].map((f) => f * potencia).find((p) => p >= bruto) ?? 0
    potencia *= 10
  }
  passo = Math.max(passo, 100)
  const marcas = Array.from({ length: N_MARCAS }, (_, i) => i * passo)
  return { maximo: marcas[N_MARCAS - 1], marcas }
}
