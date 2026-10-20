import { expect, test } from 'vitest'
import { temMica } from './mica'

const tauri = { __TAURI_INTERNALS__: {} } as unknown as Window
const nav = (platform: string, platformVersion?: string) =>
  ({
    userAgentData: { getHighEntropyValues: async () => ({ platform, platformVersion }) },
  }) as unknown as Navigator

test('fora do Tauri não há Mica', async () => {
  expect(await temMica({} as Window, nav('Windows', '15.0.0'))).toBe(false)
})

test('Windows 11 tem Mica; Windows 10 não', async () => {
  expect(await temMica(tauri, nav('Windows', '15.0.0'))).toBe(true)
  expect(await temMica(tauri, nav('Windows', '13.0.0'))).toBe(true)
  expect(await temMica(tauri, nav('Windows', '10.0.0'))).toBe(false)
})

test('sem informação, não se sabe', async () => {
  expect(await temMica(tauri, {} as Navigator)).toBeNull()
  expect(await temMica(tauri, nav('Windows'))).toBeNull()
})
