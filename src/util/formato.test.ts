import { expect, test } from 'vitest'
import { formatarData, formatarEuros, formatarHora, formatarKm, parseEuros } from './formato'

test('parseEuros', () => {
  expect(parseEuros('1.234,56')).toBe(123456)
  expect(parseEuros('45,5')).toBe(4550)
  expect(parseEuros('45')).toBe(4500)
  expect(parseEuros('45.5')).toBe(4550)
  expect(parseEuros('45 €')).toBe(4500)
  for (const s of ['', '-3', 'abc', '1,234']) expect(parseEuros(s)).toBeNull()
})

test('parseEuros — casos extra', () => {
  expect(parseEuros('1234,56')).toBe(123456)
  expect(parseEuros('0')).toBe(0)
  expect(parseEuros('120,00 €')).toBe(12000)
  expect(parseEuros('  7,05  ')).toBe(705)
  expect(parseEuros('1 234,56')).toBe(123456)
  expect(parseEuros('1 234,56 €')).toBe(123456)
  expect(parseEuros('1.234.567')).toBe(123456700)
  expect(parseEuros('1.234.567,8')).toBe(123456780)
  expect(parseEuros('45.50')).toBe(4550)
  expect(parseEuros('90071992547409,91')).toBe(Number.MAX_SAFE_INTEGER)
  expect(parseEuros('90071992547409,92')).toBeNull() // fora dos inteiros seguros
  expect(parseEuros('9'.repeat(30))).toBeNull()
  for (const s of [
    '   ',
    '€',
    '1.234', // ambíguo: 3 casas decimais
    '1,2,3',
    '12.34,56', // grupo de milhares inválido
    '1.23,45',
    '45,555',
    '45.555',
    ',5',
    '5,',
    '1e3',
    '- 3',
    '3-',
  ])
    expect(parseEuros(s), s).toBeNull()
})

test('formatarEuros', () => {
  expect(formatarEuros(123456)).toBe('1 234,56 €')
  expect(formatarEuros(0)).toBe('0,00 €')
})

test('formatarEuros — casos extra', () => {
  expect(formatarEuros(5)).toBe('0,05 €')
  expect(formatarEuros(4550)).toBe('45,50 €')
  expect(formatarEuros(123456789)).toBe('1 234 567,89 €')
  expect(formatarEuros(-12000)).toBe('-120,00 €')
})

test('formatarKm', () => {
  expect(formatarKm(70000)).toBe('70 000 km')
  expect(formatarKm(0)).toBe('0 km')
  expect(formatarKm(999)).toBe('999 km')
  expect(formatarKm(1234567)).toBe('1 234 567 km')
})

test('formatarData', () => {
  expect(formatarData('2026-10-20')).toBe('20 out 2026')
  expect(formatarData('2026-01-05')).toBe('5 jan 2026')
  expect(formatarData('2026-12-31T18:42:00')).toBe('31 dez 2026')
  const meses = ['jan', 'fev', 'mar', 'abr', 'mai', 'jun', 'jul', 'ago', 'set', 'out', 'nov', 'dez']
  meses.forEach((m, i) => expect(formatarData(`2026-${String(i + 1).padStart(2, '0')}-01`)).toBe(`1 ${m} 2026`))
  expect(formatarData('lixo')).toBe('lixo')
})

test('formatarHora', () => {
  expect(formatarHora('2026-10-20T18:42:07')).toBe('18:42')
  expect(formatarHora('2026-10-20 09:05')).toBe('09:05')
  expect(formatarHora('2026-10-20')).toBeNull()
})
