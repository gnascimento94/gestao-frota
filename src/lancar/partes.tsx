// Peças de interface partilhadas pelos formulários da folha Lançar.

import { useId, useState, type ReactNode } from 'react'
import type { Carro } from '../api/tipos'
import { normalizar } from '../ecras/comum'
import { Botao } from '../ui/Botao'
import { Campo } from '../ui/Campo'
import { formatarKm } from '../util/formato'
import { rotuloCarro } from './logica'

/** Controlo segmentado (botões com `aria-pressed`), com rótulo visível opcional. */
export function Segmentado<T extends string>({
  rotulo,
  rotuloVisivel = true,
  opcoes,
  valor,
  onMudar,
  classe,
}: {
  rotulo: string
  rotuloVisivel?: boolean
  opcoes: readonly (readonly [T, string])[]
  valor: NoInfer<T>
  onMudar(v: NoInfer<T>): void
  classe?: string
}) {
  const id = useId()
  const grupo = (
    <div
      className={['seg', classe].filter(Boolean).join(' ')}
      role="group"
      aria-label={rotuloVisivel ? undefined : rotulo}
      aria-labelledby={rotuloVisivel ? id : undefined}
    >
      {opcoes.map(([v, texto]) => (
        <button key={v} type="button" aria-pressed={v === valor} onClick={() => onMudar(v)}>
          {texto}
        </button>
      ))}
    </div>
  )
  if (!rotuloVisivel) return grupo
  return (
    <div className="campo">
      <span className="campo-rotulo" id={id}>
        {rotulo}
      </span>
      {grupo}
    </div>
  )
}

/** Interruptor (checkbox com papel de `switch`) com texto de apoio opcional. */
export function Interruptor({
  rotulo,
  detalhe,
  ligado,
  onMudar,
}: {
  rotulo: string
  detalhe?: ReactNode
  ligado: boolean
  onMudar(v: boolean): void
}) {
  const id = useId()
  return (
    <label className="interruptor-switch">
      <input
        type="checkbox"
        role="switch"
        checked={ligado}
        onChange={(e) => onMudar(e.target.checked)}
        aria-labelledby={`${id}-r`}
        aria-describedby={detalhe ? `${id}-d` : undefined}
      />
      <span>
        <span id={`${id}-r`}>{rotulo}</span>
        {detalhe && <small id={`${id}-d`}>{detalhe}</small>}
      </span>
    </label>
  )
}

/** Escolha do carro por pesquisa de matrícula ("aa12" encontra AA-12-BB). */
export function SeletorCarro({
  carros,
  carro,
  onEscolher,
  erro,
  focoInicial,
}: {
  carros: Carro[]
  carro: Carro | null
  onEscolher(id: number | null): void
  erro?: string
  focoInicial: boolean
}) {
  const idLista = useId()
  const [texto, setTexto] = useState(carro ? rotuloCarro(carro) : '')
  const [candidatos, setCandidatos] = useState(0)

  // Pré-selecionado: mostra o carro quando a lista chega (ajuste durante o render, sem efeito).
  const [visto, setVisto] = useState(carro?.id ?? null)
  if (carro && carro.id !== visto) {
    setVisto(carro.id)
    if (texto === '') setTexto(rotuloCarro(carro))
  }

  function mudar(v: string) {
    setTexto(v)
    const q = normalizar(v.split('·')[0])
    const exato = q ? carros.find((c) => normalizar(c.matricula) === q) : undefined
    const parecidos = q.length >= 2 ? carros.filter((c) => normalizar(c.matricula).includes(q)) : []
    const escolhido = exato ?? (parecidos.length === 1 ? parecidos[0] : null)
    setCandidatos(parecidos.length)
    onEscolher(escolhido?.id ?? null)
  }

  let ajuda: string | undefined
  let tom: 'ok' | 'alerta' | 'normal' = 'normal'
  if (carro) {
    ajuda = `KM atual: ${formatarKm(carro.kmAtual)}${carro.motoristaAtual ? ` · ${carro.motoristaAtual[1]}` : ''}`
    tom = 'ok'
  } else if (!/^\s*$/.test(texto)) {
    ajuda =
      candidatos > 1
        ? `${candidatos} carros correspondem — continue a escrever a matrícula.`
        : 'Nenhum carro com esta matrícula.'
    tom = 'alerta'
  }

  return (
    <>
      <Campo
        rotulo="Carro"
        value={texto}
        onChange={(e) => mudar(e.target.value)}
        placeholder="Matrícula, por exemplo AA-12-BB"
        list={idLista}
        autoComplete="off"
        spellCheck={false}
        erro={erro}
        ajuda={ajuda}
        tomAjuda={tom}
        data-foco-inicial={focoInicial || undefined}
      />
      <datalist id={idLista}>
        {carros.map((c) => (
          <option key={c.id} value={rotuloCarro(c)} />
        ))}
      </datalist>
    </>
  )
}

/** Campo de KM com o KM atual como ajuda e o botão "Confirmar" quando o núcleo pede confirmação. */
export function CampoKm({
  valor,
  onMudar,
  erro,
  carro,
  pedeConfirmacao,
  onConfirmar,
  aEnviar,
}: {
  valor: string
  onMudar(v: string): void
  erro?: string
  carro: Carro | null
  pedeConfirmacao: boolean
  onConfirmar(): void
  aEnviar: boolean
}) {
  return (
    <div className="campo-com-acao">
      <Campo
        rotulo="KM"
        inputMode="numeric"
        value={valor}
        onChange={(e) => onMudar(e.target.value)}
        erro={erro}
        ajuda={carro ? `Último registado: ${formatarKm(carro.kmAtual)}` : undefined}
        autoComplete="off"
      />
      {pedeConfirmacao && (
        <Botao tamanho="pequeno" variante="secundario" onClick={onConfirmar} disabled={aEnviar}>
          Confirmar
        </Botao>
      )}
    </div>
  )
}

/** Mensagem de erro geral (referência inválida, só consulta, …). */
export function ErroGeral({ texto }: { texto?: string | null }) {
  if (!texto) return null
  return (
    <p className="form-erro" role="alert">
      {texto}
    </p>
  )
}

export function Rodape({ onCancelar, aEnviar }: { onCancelar(): void; aEnviar: boolean }) {
  return (
    <div className="form-rodape">
      <Botao variante="secundario" onClick={onCancelar}>
        Cancelar
      </Botao>
      <Botao type="submit" variante="primario" disabled={aEnviar}>
        Lançar
      </Botao>
    </div>
  )
}
