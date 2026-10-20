// Deteta se o material Mica do Windows 11 pode estar por baixo da janela.
// Sem Mica (Windows 10, ou fora do Tauri), marca <html data-sem-mica> e a barra lateral fica sólida.

interface UADataHE {
  platform?: string
  getHighEntropyValues?(dicas: string[]): Promise<{ platform?: string; platformVersion?: string }>
}

/** `true` = há Mica, `false` = não há, `null` = não se sabe (assume-se que há). */
export async function temMica(
  win: Window = window,
  nav: Navigator & { userAgentData?: UADataHE } = navigator,
): Promise<boolean | null> {
  if (!('__TAURI_INTERNALS__' in win)) return false
  const ua = nav.userAgentData
  if (!ua?.getHighEntropyValues) return null
  try {
    const { platform, platformVersion } = await ua.getHighEntropyValues(['platformVersion'])
    if (platform && platform !== 'Windows') return false
    const maior = Number((platformVersion ?? '').split('.')[0])
    if (!Number.isFinite(maior) || platformVersion === undefined) return null
    // UA-CH no Windows: platformVersion ≥ 13 = Windows 11; 1–10 = Windows 10.
    return maior >= 13
  } catch {
    return null
  }
}

export async function marcarSemMica(doc: Document = document) {
  if ((await temMica()) === false) doc.documentElement.setAttribute('data-sem-mica', '')
}
