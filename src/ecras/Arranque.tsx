// Ecrã de arranque: antes da aplicação normal pergunta ao núcleo como correu o arranque
// (`arranque()`) e só mostra a aplicação quando os dados estão abertos (normal ou consulta).

import { useCallback, useEffect, useState, type ReactNode } from 'react'
import { abrirConsulta, arranque, listarVersoes } from '../api/comandos'
import type { Arranque as EstadoArranque, MotivoCorrompido } from '../api/tipos'
import { Botao } from '../ui/Botao'
import { paraErroApi, useCarregar } from './comum'
import { restauraveis } from './textosVersoes'
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
  const [tentativas, setTentativas] = useState(0)

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
    setTentativas((n) => n + 1)
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
      return (
        <Corrompido
          motivo={p.motivo}
          mensagem={p.mensagem}
          tentativas={tentativas}
          ocupado={ocupado}
          onTentar={() => void tentarDeNovo()}
          onRestaurada={() => setFase({ tipo: 'aberto' })}
        />
      )
  }
}

const AJUDA_DRIVE =
  'Também pode recuperar uma versão anterior no Google Drive: em drive.google.com, abra a pasta Frota › dados, clique com o botão direito em frota.db e escolha «Informações do ficheiro › Gerir versões» (histórico de versões do ficheiro frota.db).'

/**
 * Dados que não abrem. "vazio" (o Google Drive ainda não descarregou o ficheiro): a ação
 * principal é esperar e "Tentar de novo"; restaurar fica disponível mas em segundo plano.
 * "danificado": escolher uma cópia para restaurar (o ficheiro danificado é guardado à parte
 * pelo núcleo). Nunca fica sem saída: sem lista de cópias há "Tentar de novo" e a ajuda do Drive.
 */
function Corrompido({
  motivo,
  mensagem,
  tentativas,
  ocupado,
  onTentar,
  onRestaurada,
}: {
  motivo: MotivoCorrompido
  mensagem: string
  /** Quantas vezes "Tentar de novo" já foi carregado (só no caso "vazio"). */
  tentativas: number
  ocupado: boolean
  onTentar(): void
  onRestaurada(): void
}) {
  const { dados, erro, recarregar } = useCarregar(listarVersoes)
  const vazio = motivo === 'vazio'
  const [verCopias, setVerCopias] = useState(!vazio)
  const copias = dados ? restauraveis(dados) : null
  const semCopias = erro !== null || (copias !== null && copias.length === 0)

  const botaoLista = (principal: boolean) => (
    <Botao variante={principal ? 'primario' : 'secundario'} onClick={recarregar}>
      {principal ? 'Tentar de novo' : 'Ler a lista outra vez'}
    </Botao>
  )

  const botoes = vazio ? (
    <>
      {!verCopias && (
        <Botao variante="secundario" onClick={() => setVerCopias(true)}>
          Restaurar uma cópia…
        </Botao>
      )}
      <Botao variante="primario" onClick={onTentar} disabled={ocupado}>
        Tentar de novo
      </Botao>
    </>
  ) : semCopias ? (
    botaoLista(true)
  ) : undefined

  return (
    <EcraArranque
      titulo={vazio ? 'O ficheiro dos dados ainda não está disponível neste PC.' : 'Os dados da frota estão danificados.'}
      largo={verCopias}
      botoes={botoes}
    >
      <p>{mensagem}</p>
      {vazio && tentativas > 0 && (
        <p className="texto-discreto" role="status">
          Ainda não está disponível. Espere que a sincronização termine e tente de novo.
        </p>
      )}
      {verCopias && (
        <>
          {vazio ? (
            <p>
              Só restaure uma cópia se o ficheiro não voltar a ficar disponível: a cópia pode ser mais antiga do
              que os dados atuais.
            </p>
          ) : (
            <p>
              Escolha uma cópia para restaurar — normalmente a mais recente. O ficheiro danificado é guardado à
              parte, em Backup.
            </p>
          )}
          {erro && (
            <p className="form-erro" role="alert">
              Não foi possível ler a lista de cópias: {erro}
            </p>
          )}
          {semCopias ? (
            <>
              {!erro && <p>Não há cópias guardadas nesta pasta.</p>}
              <p className="texto-discreto">{AJUDA_DRIVE}</p>
              {vazio && <div className="arranque-botoes">{botaoLista(false)}</div>}
            </>
          ) : (
            dados && (
              <div className="arranque-versoes">
                <ListaVersoes
                  versoes={dados}
                  rotuloBotao="Restaurar esta"
                  textoConfirmacao="O ficheiro atual é guardado à parte (em Backup) e substituído por esta cópia."
                  onRestaurada={onRestaurada}
                />
              </div>
            )
          )}
        </>
      )}
    </EcraArranque>
  )
}

/** Cartão centrado com o logo (ecrãs de arranque e de atualização). */
export function EcraArranque({
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
