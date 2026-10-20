// Ecrã de arranque: antes da aplicação normal pergunta ao núcleo como correu o arranque
// (`arranque()`) e só mostra a aplicação quando os dados estão abertos (normal ou consulta).

import { useCallback, useEffect, useState, type ReactNode } from 'react'
import { abrirConsulta, arranque, listarVersoes } from '../api/comandos'
import type { Arranque as EstadoArranque } from '../api/tipos'
import { Botao } from '../ui/Botao'
import { paraErroApi, useCarregar } from './comum'
import { ListaVersoes } from './Versoes'

type Problema = Exclude<EstadoArranque, { estado: 'ok' }>

type Fase =
  | { tipo: 'aVerificar' }
  | { tipo: 'aberto' }
  | { tipo: 'problema'; problema: Problema }
  /** O próprio pedido `arranque()` falhou. */
  | { tipo: 'falhou'; mensagem: string }

const faseDe = (a: EstadoArranque): Fase =>
  a.estado === 'ok' ? { tipo: 'aberto' } : { tipo: 'problema', problema: a }

export function Arranque({ children }: { children: ReactNode }) {
  const [fase, setFase] = useState<Fase>({ tipo: 'aVerificar' })
  const [ocupado, setOcupado] = useState(false)
  const [erro, setErro] = useState<string | null>(null)

  useEffect(() => {
    let vivo = true
    arranque().then(
      (a) => vivo && setFase(faseDe(a)),
      (e) => vivo && setFase({ tipo: 'falhou', mensagem: paraErroApi(e).mensagem }),
    )
    return () => {
      vivo = false
    }
  }, [])

  const tentarDeNovo = useCallback(async () => {
    setOcupado(true)
    setErro(null)
    try {
      setFase(faseDe(await arranque()))
    } catch (e) {
      setFase({ tipo: 'falhou', mensagem: paraErroApi(e).mensagem })
    } finally {
      setOcupado(false)
    }
  }, [])

  const consultar = useCallback(async () => {
    setOcupado(true)
    setErro(null)
    try {
      await abrirConsulta()
      setFase({ tipo: 'aberto' })
    } catch (e) {
      setErro(paraErroApi(e).mensagem)
    } finally {
      setOcupado(false)
    }
  }, [])

  if (fase.tipo === 'aberto') return <>{children}</>
  if (fase.tipo === 'aVerificar') return <div className="arranque" aria-busy="true" />

  const botaoTentar = (
    <Botao variante="primario" onClick={() => void tentarDeNovo()} disabled={ocupado}>
      Tentar de novo
    </Botao>
  )
  const botaoConsulta = (principal: boolean) => (
    <Botao variante={principal ? 'primario' : 'secundario'} onClick={() => void consultar()} disabled={ocupado}>
      Abrir só para consulta
    </Botao>
  )

  if (fase.tipo === 'falhou') {
    return (
      <EcraArranque titulo="Não foi possível iniciar o sistema." erro={erro} botoes={botaoTentar}>
        <p>{fase.mensagem}</p>
      </EcraArranque>
    )
  }

  const p = fase.problema
  switch (p.estado) {
    case 'bloqueado': {
      const desconhecido = p.pc === 'desconhecido'
      return (
        <EcraArranque
          titulo={
            desconhecido
              ? 'O sistema parece estar aberto noutro PC.'
              : `O sistema está aberto no PC «${p.pc}». Feche-o lá primeiro.`
          }
          erro={erro}
          botoes={
            <>
              {botaoConsulta(false)}
              {botaoTentar}
            </>
          }
        >
          <p>
            {desconhecido
              ? 'Se não estiver, espere 2 minutos e tente de novo. '
              : 'Só um PC de cada vez pode fazer alterações. '}
            Entretanto, pode abrir os dados só para consulta.
          </p>
        </EcraArranque>
      )
    }
    case 'versaoMaisRecente':
      return (
        <EcraArranque
          titulo="Estes dados foram criados por uma versão mais recente do programa."
          erro={erro}
          botoes={botaoConsulta(true)}
        >
          <p>Atualize o programa neste PC para poder fazer alterações. Entretanto, pode ver os dados.</p>
        </EcraArranque>
      )
    case 'semPasta':
      return (
        <EcraArranque titulo="Não foi possível abrir a pasta de dados." erro={erro} botoes={botaoTentar}>
          <p>{p.mensagem}</p>
        </EcraArranque>
      )
    case 'corrompido':
      return <Corrompido onRestaurada={() => setFase({ tipo: 'aberto' })} />
  }
}

function Corrompido({ onRestaurada }: { onRestaurada(): void }) {
  const { dados, erro } = useCarregar(listarVersoes)
  return (
    <EcraArranque titulo="Os dados da frota estão danificados." largo>
      <p>
        Não é possível abrir o ficheiro dos dados. Escolha uma cópia para restaurar — normalmente a mais
        recente.
      </p>
      {erro && (
        <p className="form-erro" role="alert">
          {erro}
        </p>
      )}
      {dados && (
        <div className="arranque-versoes">
          <ListaVersoes
            versoes={dados}
            rotuloBotao="Restaurar esta"
            textoConfirmacao="Os dados danificados são substituídos por esta cópia."
            onRestaurada={onRestaurada}
          />
        </div>
      )}
    </EcraArranque>
  )
}

function EcraArranque({
  titulo,
  erro,
  botoes,
  largo,
  children,
}: {
  titulo: string
  erro?: string | null
  botoes?: ReactNode
  largo?: boolean
  children?: ReactNode
}) {
  return (
    <main className="arranque">
      <section className={largo ? 'arranque-cartao arranque-largo' : 'arranque-cartao'} aria-labelledby="arranque-titulo">
        <span className="marca-logo arranque-logo" role="img" aria-label="Frota" />
        <h1 id="arranque-titulo" className="arranque-titulo">
          {titulo}
        </h1>
        <div className="arranque-texto">{children}</div>
        {erro && (
          <p className="form-erro" role="alert">
            {erro}
          </p>
        )}
        {botoes && <div className="arranque-botoes">{botoes}</div>}
      </section>
    </main>
  )
}
