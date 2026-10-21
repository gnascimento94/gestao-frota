// Lista de versões (cópias em dados\versoes) com restauro confirmado na própria página.
// Partilhada pelo ecrã Backup e pelo ecrã de arranque com os dados danificados.

import { useEffect, useId, useRef, useState } from 'react'
import { restaurarVersao } from '../api/comandos'
import type { Versao } from '../api/tipos'
import { Botao } from '../ui/Botao'
import { formatarDataHora } from '../util/formato'
import { paraErroApi } from './comum'
import { contagensDaVersao, eDanificada, tipoDaVersao } from './textosVersoes'

export function ListaVersoes({
  versoes,
  rotuloBotao,
  textoConfirmacao,
  desativado,
  onRestaurada,
}: {
  versoes: Versao[]
  /** "Restaurar esta" (arranque) ou "Restaurar…" (Backup). */
  rotuloBotao: string
  /** Frase depois de "Restaurar a cópia de …?". */
  textoConfirmacao: string
  /** Modo só de consulta: não deixa restaurar. */
  desativado?: boolean
  onRestaurada(v: Versao): void
}) {
  const [escolhida, setEscolhida] = useState<string | null>(null)
  const [aRestaurar, setARestaurar] = useState(false)
  const [erro, setErro] = useState<string | null>(null)
  const botoes = useRef(new Map<string, HTMLButtonElement>())

  function cancelar() {
    const nome = escolhida
    setEscolhida(null)
    if (nome) botoes.current.get(nome)?.focus()
  }

  async function restaurar(v: Versao) {
    setARestaurar(true)
    setErro(null)
    try {
      await restaurarVersao(v.nome)
      setEscolhida(null)
      onRestaurada(v)
    } catch (e) {
      setErro(paraErroApi(e).mensagem)
    } finally {
      setARestaurar(false)
    }
  }

  if (versoes.length === 0) return <p className="vazio-lista">Ainda não há cópias.</p>
  const maisRecente = versoes.find((v) => !eDanificada(v.nome))?.nome

  return (
    <>
      {erro && (
        <p className="form-erro versoes-erro" role="alert">
          {erro}
        </p>
      )}
      <ul className="lista" aria-label="Cópias disponíveis">
        {versoes.map((v) => (
          <li key={v.nome} className="item-lista">
            <div className="item-corpo">
              <span className="item-texto">
                <span className="item-titulo">{formatarDataHora(v.data)}</span>
                <span className="item-detalhe">
                  {tipoDaVersao(v.nome)} · {contagensDaVersao(v)}
                </span>
              </span>
              <span className="item-fim">
                {v.nome === maisRecente && <span className="pilula pilula-ok">Mais recente</span>}
                {eDanificada(v.nome) ? (
                  <span className="pilula pilula-neutra">Danificado</span>
                ) : (
                  <Botao
                    ref={(b) => {
                      if (b) botoes.current.set(v.nome, b)
                      else botoes.current.delete(v.nome)
                    }}
                    tamanho="pequeno"
                    disabled={desativado || aRestaurar}
                    aria-expanded={escolhida === v.nome}
                    onClick={() => {
                      setErro(null)
                      setEscolhida(v.nome)
                    }}
                  >
                    {rotuloBotao}
                  </Botao>
                )}
              </span>
            </div>
            {escolhida === v.nome && (
              <Confirmacao
                versao={v}
                texto={textoConfirmacao}
                aRestaurar={aRestaurar}
                onCancelar={cancelar}
                onConfirmar={() => void restaurar(v)}
              />
            )}
          </li>
        ))}
      </ul>
    </>
  )
}

function Confirmacao({
  versao,
  texto,
  aRestaurar,
  onCancelar,
  onConfirmar,
}: {
  versao: Versao
  texto: string
  aRestaurar: boolean
  onCancelar(): void
  onConfirmar(): void
}) {
  const id = useId()
  const refCancelar = useRef<HTMLButtonElement>(null)
  useEffect(() => refCancelar.current?.focus(), [])
  return (
    <div
      className="confirmacao"
      role="alertdialog"
      aria-labelledby={`${id}-t`}
      aria-describedby={`${id}-d`}
      onKeyDown={(e) => {
        if (e.key === 'Escape' && !aRestaurar) {
          e.stopPropagation()
          onCancelar()
        }
      }}
    >
      <p>
        <strong id={`${id}-t`}>Restaurar a cópia de {formatarDataHora(versao.data)}?</strong>{' '}
        <span id={`${id}-d`}>{texto}</span>
      </p>
      <div className="confirmacao-botoes">
        <Botao ref={refCancelar} tamanho="pequeno" onClick={onCancelar} disabled={aRestaurar}>
          Cancelar
        </Botao>
        <Botao tamanho="pequeno" variante="primario" onClick={onConfirmar} disabled={aRestaurar}>
          {aRestaurar ? 'A restaurar…' : 'Restaurar'}
        </Botao>
      </div>
    </div>
  )
}
