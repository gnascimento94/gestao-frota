// Lista de versões (cópias em dados\versoes) com restauro confirmado na própria página.
// Partilhada pelo ecrã Backup (que também deixa nomear e apagar) e pelo ecrã de arranque
// com os dados danificados.

import { useEffect, useId, useRef, useState, type ReactNode } from 'react'
import { apagarVersao, nomearVersao, restaurarVersao } from '../api/comandos'
import type { Versao } from '../api/tipos'
import { Botao, type VarianteBotao } from '../ui/Botao'
import { Campo } from '../ui/Campo'
import { formatarDataHora } from '../util/formato'
import { paraErroApi } from './comum'
import { contagensDaVersao, ePostoDeLado, tipoDaVersao } from './textosVersoes'

type Acao = 'restaurar' | 'nomear' | 'apagar'

const MAX_ROTULO = 60

export function ListaVersoes({
  versoes,
  rotuloBotao,
  textoConfirmacao,
  desativado,
  onRestaurada,
  onAlterada,
}: {
  versoes: Versao[]
  /** "Restaurar esta" (arranque) ou "Restaurar…" (Backup). */
  rotuloBotao: string
  /** Frase depois de "Restaurar a cópia de …?". */
  textoConfirmacao: string
  /** Modo só de consulta: não deixa restaurar, nomear nem apagar. */
  desativado?: boolean
  onRestaurada(v: Versao): void
  /** Com esta função a lista também deixa nomear e apagar cópias (ecrã Backup). */
  onAlterada?(): void
}) {
  const [escolha, setEscolha] = useState<{ nome: string; acao: Acao } | null>(null)
  const [ocupado, setOcupado] = useState(false)
  const [erro, setErro] = useState<string | null>(null)
  const botoes = useRef(new Map<string, HTMLButtonElement>())

  function cancelar() {
    const e = escolha
    setEscolha(null)
    if (e) botoes.current.get(`${e.acao}:${e.nome}`)?.focus()
  }

  async function executar(f: () => Promise<void>, depois: () => void) {
    setOcupado(true)
    setErro(null)
    try {
      await f()
      setEscolha(null)
      depois()
    } catch (e) {
      setErro(paraErroApi(e).mensagem)
    } finally {
      setOcupado(false)
    }
  }

  if (versoes.length === 0) return <p className="vazio-lista">Ainda não há cópias.</p>
  const maisRecente = versoes.find((v) => v.restauravel && !ePostoDeLado(v.nome))?.nome
  const gerir = !!onAlterada

  function botao(v: Versao, acao: Acao, texto: string, variante?: VarianteBotao) {
    const chave = `${acao}:${v.nome}`
    return (
      <Botao
        ref={(b) => {
          if (b) botoes.current.set(chave, b)
          else botoes.current.delete(chave)
        }}
        tamanho="pequeno"
        variante={variante}
        disabled={desativado || ocupado}
        aria-expanded={escolha?.nome === v.nome && escolha.acao === acao}
        onClick={() => {
          setErro(null)
          setEscolha({ nome: v.nome, acao })
        }}
      >
        {texto}
      </Botao>
    )
  }

  return (
    <>
      {erro && (
        <p className="form-erro versoes-erro" role="alert">
          {erro}
        </p>
      )}
      <ul className="lista" aria-label="Cópias disponíveis">
        {versoes.map((v) => {
          const quando = formatarDataHora(v.data)
          const acao = escolha?.nome === v.nome ? escolha.acao : null
          return (
            <li key={v.nome} className="item-lista">
              <div className="item-corpo">
                <span className="item-texto">
                  <span className="item-titulo">{v.rotulo || quando}</span>
                  <span className="item-detalhe">
                    {v.rotulo && `${quando} · `}
                    {tipoDaVersao(v.nome)} · {contagensDaVersao(v)}
                  </span>
                </span>
                <span className="item-fim">
                  {v.nome === maisRecente && <span className="pilula pilula-ok">Mais recente</span>}
                  {gerir && botao(v, 'nomear', v.rotulo ? 'Renomear' : 'Nomear', 'simples')}
                  {gerir && botao(v, 'apagar', 'Apagar…', 'simples')}
                  {!v.restauravel ? (
                    <span className="pilula pilula-neutra">Não restaurável</span>
                  ) : (
                    botao(v, 'restaurar', rotuloBotao)
                  )}
                </span>
              </div>
              {acao === 'restaurar' && (
                <Confirmacao
                  titulo={`Restaurar a cópia de ${quando}?`}
                  texto={textoConfirmacao}
                  confirmar="Restaurar"
                  aConfirmar="A restaurar…"
                  ocupado={ocupado}
                  onCancelar={cancelar}
                  onConfirmar={() => void executar(() => restaurarVersao(v.nome), () => onRestaurada(v))}
                />
              )}
              {acao === 'apagar' && (
                <Confirmacao
                  titulo={`Apagar a cópia ${v.rotulo ? `«${v.rotulo}»` : `de ${quando}`}?`}
                  texto="Não é possível desfazer."
                  confirmar="Apagar"
                  aConfirmar="A apagar…"
                  variante="perigo"
                  ocupado={ocupado}
                  onCancelar={cancelar}
                  onConfirmar={() => void executar(() => apagarVersao(v.nome), () => onAlterada?.())}
                />
              )}
              {acao === 'nomear' && (
                <Nomear
                  inicial={v.rotulo ?? ''}
                  ocupado={ocupado}
                  onCancelar={cancelar}
                  onGuardar={(rotulo) =>
                    void executar(
                      () => nomearVersao(v.nome, rotulo),
                      () => {
                        botoes.current.get(`nomear:${v.nome}`)?.focus()
                        onAlterada?.()
                      },
                    )
                  }
                />
              )}
            </li>
          )
        })}
      </ul>
    </>
  )
}

function Confirmacao({
  titulo,
  texto,
  confirmar,
  aConfirmar,
  variante = 'primario',
  ocupado,
  onCancelar,
  onConfirmar,
}: {
  titulo: ReactNode
  texto: string
  confirmar: string
  aConfirmar: string
  variante?: VarianteBotao
  ocupado: boolean
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
        if (e.key === 'Escape' && !ocupado) {
          e.stopPropagation()
          onCancelar()
        }
      }}
    >
      <p>
        <strong id={`${id}-t`}>{titulo}</strong> <span id={`${id}-d`}>{texto}</span>
      </p>
      <div className="confirmacao-botoes">
        <Botao ref={refCancelar} tamanho="pequeno" onClick={onCancelar} disabled={ocupado}>
          Cancelar
        </Botao>
        <Botao tamanho="pequeno" variante={variante} onClick={onConfirmar} disabled={ocupado}>
          {ocupado ? aConfirmar : confirmar}
        </Botao>
      </div>
    </div>
  )
}

/** Nome da cópia, editado na própria linha. Vazio tira o nome. */
function Nomear({
  inicial,
  ocupado,
  onCancelar,
  onGuardar,
}: {
  inicial: string
  ocupado: boolean
  onCancelar(): void
  onGuardar(rotulo: string): void
}) {
  const [texto, setTexto] = useState(inicial)
  const ref = useRef<HTMLInputElement>(null)
  useEffect(() => {
    ref.current?.focus()
    ref.current?.select()
  }, [])
  return (
    <form
      className="confirmacao versao-nomear"
      onSubmit={(e) => {
        e.preventDefault()
        if (!ocupado) onGuardar(texto.trim())
      }}
      onKeyDown={(e) => {
        if (e.key === 'Escape' && !ocupado) {
          e.stopPropagation()
          onCancelar()
        }
      }}
    >
      <Campo
        ref={ref}
        rotulo="Nome da cópia"
        value={texto}
        maxLength={MAX_ROTULO}
        placeholder="Ex.: Antes de apagar os carros vendidos"
        ajuda="Uma cópia com nome nunca é apagada automaticamente. Deixe vazio para tirar o nome."
        onChange={(e) => setTexto(e.target.value)}
        disabled={ocupado}
        classeCampo="versao-nomear-campo"
      />
      <div className="confirmacao-botoes">
        <Botao tamanho="pequeno" onClick={onCancelar} disabled={ocupado}>
          Cancelar
        </Botao>
        <Botao type="submit" tamanho="pequeno" variante="primario" disabled={ocupado}>
          {ocupado ? 'A guardar…' : 'Guardar'}
        </Botao>
      </div>
    </form>
  )
}
