import { useCallback, useEffect, useRef, useState } from 'react'
import { Aviso } from './ui/Aviso'
import { Barra } from './ui/Barra'
import { Cartao } from './ui/Cartao'
import { Desfazer, DesfazerProvider } from './ui/Desfazer'
import { useEstadoArmazenamento } from './ui/useEstadoArmazenamento'
import { Lancar } from './lancar/Lancar'
import { InicioOuBoasVindas } from './ecras/BoasVindas'
import { Carros } from './ecras/Carros'
import { FichaCarro } from './ecras/FichaCarro'
import { Inicio } from './ecras/Inicio'
import { Motoristas } from './ecras/Motoristas'
import { Relatorios } from './ecras/Relatorios'

export type Ecra =
  | { nome: 'inicio' }
  | { nome: 'carros' }
  | { nome: 'carro'; id: number }
  | { nome: 'motoristas'; /** Motorista a abrir (p.ex. a partir de um alerta). */ id?: number }
  | { nome: 'relatorios' }
  | { nome: 'planos' }
  | { nome: 'backup' }

const TITULOS: Record<Ecra['nome'], string> = {
  inicio: 'Início',
  carros: 'Carros',
  carro: 'Carro',
  motoristas: 'Motoristas',
  relatorios: 'Relatórios',
  planos: 'Planos de manutenção',
  backup: 'Backup',
}

export default function App() {
  return (
    <DesfazerProvider>
      <Concha />
    </DesfazerProvider>
  )
}

function Concha() {
  const [ecra, setEcra] = useState<Ecra>({ nome: 'inicio' })
  const [lancar, setLancar] = useState<{
    aberta: boolean
    origem: DOMRect | null
    /** Carro de onde foi aberta (fica pré-selecionado). */
    carroId: number | null
  }>({ aberta: false, origem: null, carroId: null })
  const [pesquisa, setPesquisa] = useState('')
  const gravacao = useEstadoArmazenamento()
  const refLancar = useRef<HTMLButtonElement>(null)
  const refPesquisa = useRef<HTMLInputElement>(null)
  const refPrincipal = useRef<HTMLElement>(null)

  const abrirLancar = useCallback((origem: DOMRect | null, carroId?: number) => {
    setLancar({ aberta: true, origem, carroId: carroId ?? null })
  }, [])
  const fecharLancar = useCallback(() => setLancar((l) => ({ ...l, aberta: false })), [])
  const [versao, setVersao] = useState(0)
  const recarregar = useCallback(() => setVersao((v) => v + 1), [])
  const [zonaToasts, setZonaToasts] = useState<HTMLDivElement | null>(null)

  // Atalhos globais: Ctrl+N (lançar), Ctrl+K (pesquisar). Esc é tratado pela folha.
  useEffect(() => {
    const tecla = (e: KeyboardEvent) => {
      if (!e.ctrlKey || e.altKey || e.shiftKey || e.metaKey) return
      const k = e.key.toLowerCase()
      if (k === 'n') {
        e.preventDefault()
        // Na ficha de um carro, esse carro fica pré-selecionado.
        if (!lancar.aberta)
          abrirLancar(
            refLancar.current?.getBoundingClientRect() ?? null,
            ecra.nome === 'carro' ? ecra.id : undefined,
          )
      } else if (k === 'k') {
        e.preventDefault()
        if (lancar.aberta) return
        refPesquisa.current?.focus()
        refPesquisa.current?.select()
      }
    }
    window.addEventListener('keydown', tecla)
    return () => window.removeEventListener('keydown', tecla)
  }, [abrirLancar, lancar.aberta, ecra])

  const navegar = useCallback((e: Ecra) => {
    setEcra(e)
    refPrincipal.current?.scrollTo({ top: 0 })
  }, [])

  const titulo = TITULOS[ecra.nome]
  const [ano] = useState(() => new Date().getFullYear())

  return (
    <div className="app">
      <Barra
        ecra={ecra}
        onNavegar={navegar}
        onLancar={abrirLancar}
        refLancar={refLancar}
        gravacao={gravacao}
      />

      <main className="principal" ref={refPrincipal}>
        <header className="topo">
          <div className="topo-titulos">
            <h1 className="topo-titulo">{titulo}</h1>
            
          </div>
          <label className="busca">
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <circle cx="11" cy="11" r="7" />
              <path d="M20 20l-3.5-3.5" />
            </svg>
            <input
              ref={refPesquisa}
              type="search"
              placeholder="Procurar matrícula"
              aria-label="Procurar matrícula"
              aria-keyshortcuts="Control+K"
              value={pesquisa}
              onChange={(e) => {
                setPesquisa(e.target.value)
                if (e.target.value && ecra.nome !== 'carros') navegar({ nome: 'carros' })
              }}
              spellCheck={false}
              autoComplete="off"
            />
            <kbd aria-hidden="true">Ctrl K</kbd>
          </label>
        </header>

        <Aviso {...gravacao} />

        {/* `versao` muda após cada lançamento: o ecrã visível volta a pedir os dados
            (sem remontar: separador, filtros, scroll e foco mantêm-se). */}
        <div className="conteudo">
          {ecra.nome === 'carros' && (
            <Carros
              ano={ano}
              versao={versao}
              pesquisa={pesquisa}
              onAbrir={(id) => navegar({ nome: 'carro', id })}
            />
          )}
          {ecra.nome === 'carro' && (
            <FichaCarro
              key={ecra.id}
              id={ecra.id}
              ano={ano}
              versao={versao}
              onNavegar={navegar}
              onLancar={abrirLancar}
            />
          )}
          {ecra.nome === 'motoristas' && (
            <Motoristas key={ecra.id ?? 'lista'} ano={ano} versao={versao} inicial={ecra.id} />
          )}
          {ecra.nome === 'inicio' && (
            <InicioOuBoasVindas ano={ano} versao={versao} onNavegar={navegar}>
              <Inicio ano={ano} versao={versao} onNavegar={navegar} />
            </InicioOuBoasVindas>
          )}
          {ecra.nome === 'relatorios' && <Relatorios versao={versao} onNavegar={navegar} />}
          {(ecra.nome === 'planos' || ecra.nome === 'backup') && <EcraVazio titulo={titulo} />}
        </div>
      </main>

      <Lancar
        aberta={lancar.aberta}
        origem={lancar.origem}
        carroId={lancar.carroId}
        onFechar={fecharLancar}
        onLancado={recarregar}
        zonaToasts={zonaToasts}
      />

      {/* Toasts empilhados: "Lançado ✓" (portal da folha Lançar) e "Desfazer". */}
      <div className="zona-toasts" ref={setZonaToasts}>
        <Desfazer />
      </div>
    </div>
  )
}

/** Ecrã ainda por construir (as próximas etapas preenchem cada um). */
function EcraVazio({ titulo }: { titulo: string }) {
  return (
    <Cartao>
      <div className="vazio">
        <strong>{titulo}</strong>
        Este ecrã ainda está em construção.
      </div>
    </Cartao>
  )
}
