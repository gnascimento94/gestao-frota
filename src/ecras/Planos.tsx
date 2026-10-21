import { useCallback, useMemo, useState, type FormEvent } from 'react'
import {
  carrosListar,
  limiaresGuardar,
  limiaresObter,
  planoApagar,
  planoGuardar,
  planosListar,
  tipoAtivo,
  tipoGuardar,
  tiposListar,
} from '../api/comandos'
import type { Carro, Plano, PlanoInput, TipoManutencao } from '../api/tipos'
import { lerKm, MSG_KM, rotuloCarro } from '../lancar/logica'
import { Segmentado } from '../lancar/partes'
import { Botao } from '../ui/Botao'
import { Campo, CampoSelecao } from '../ui/Campo'
import { Cartao } from '../ui/Cartao'
import { CabecalhoFolha, Folha } from '../ui/Folha'
import { ItemLista, Lista } from '../ui/Lista'
import { useDesfazer, useEscondidas } from '../ui/useDesfazer'
import { errosDe, useCarregar, type ErrosCampos } from './comum'
import { SUGESTAO, textoAlvo, textoIntervalo, textoPlano } from './textosPlanos'

const MSG_MESES = 'Indique os meses como número inteiro.'

const chavePlano = (id: number) => `plano:${id}`

interface Dados {
  planos: Plano[]
  tipos: TipoManutencao[]
  carros: Carro[]
}

interface Inicial {
  tipoId: number | null
  cadaKm: number | null
  cadaMeses: number | null
}

type EstadoFolha = { aberta: boolean; origem: DOMRect | null; plano: Plano | null; inicial: Inicial | null }

export function Planos({ ano, versao = 0, soConsulta = false }: { ano: number; versao?: number; soConsulta?: boolean }) {
  const buscar = useCallback(async (): Promise<Dados> => {
    const [planos, tipos, carros] = await Promise.all([planosListar(), tiposListar(), carrosListar(false, ano)])
    return { planos, tipos, carros }
  }, [ano])
  const { dados, erro, recarregar } = useCarregar(buscar, versao)
  const { agendar } = useDesfazer()
  // Os apagamentos pendentes vivem no provider do "Desfazer": navegar e voltar não os faz
  // reaparecer, e um plano apagado só deixa de ser escondido quando a lista já não o tem.
  const presentes = useMemo(
    () => (dados ? new Set(dados.planos.map((p) => chavePlano(p.id))) : null),
    [dados],
  )
  const escondidas = useEscondidas(presentes)
  const [folha, setFolha] = useState<EstadoFolha>({ aberta: false, origem: null, plano: null, inicial: null })
  const fechar = () => setFolha((f) => ({ ...f, aberta: false }))

  const matriculas = new Map((dados?.carros ?? []).map((c) => [c.id, c.matricula]))
  const visiveis = (dados?.planos ?? []).filter((p) => !escondidas.has(chavePlano(p.id)))

  function apagar(p: Plano) {
    // Falha: o provider mostra a mensagem do núcleo e a App recarrega.
    agendar(
      `Apagado: ${p.tipoNome}`,
      async () => {
        await planoApagar(p.id)
        recarregar()
      },
      undefined,
      chavePlano(p.id),
    )
  }

  function criarSugestao(origem: DOMRect) {
    const tipo = dados?.tipos.find((t) => t.ativo && t.nome === SUGESTAO.tipoNome)
    setFolha({
      aberta: true,
      origem,
      plano: null,
      inicial: { tipoId: tipo?.id ?? null, cadaKm: SUGESTAO.cadaKm, cadaMeses: SUGESTAO.cadaMeses },
    })
  }

  const titulo = folha.plano ? 'Editar plano' : 'Novo plano'

  return (
    <>
      <div className="barra-ferramentas">
        <p className="texto-discreto">
          Regras de manutenção periódica. O que chegar primeiro — km ou meses — faz aparecer o alerta.
        </p>
        <span className="espaco" />
        <Botao
          variante="primario"
          disabled={soConsulta || !dados}
          onClick={(e) =>
            setFolha({ aberta: true, origem: e.currentTarget.getBoundingClientRect(), plano: null, inicial: null })
          }
        >
          Novo plano
        </Botao>
      </div>

      {erro && (
        <p className="form-erro" role="alert">
          {erro}
        </p>
      )}

      <div className="planos-grelha">
        <Cartao titulo="Planos" semMargem contagem={dados ? visiveis.length : undefined}>
          {dados && visiveis.length === 0 ? (
            <div className="planos-vazio">
              <p>Ainda não há planos de manutenção.</p>
              <p className="texto-discreto">
                Sugestão: {SUGESTAO.tipoNome} {textoIntervalo(SUGESTAO.cadaKm, SUGESTAO.cadaMeses)}
              </p>
              <Botao
                variante="primario"
                disabled={soConsulta}
                onClick={(e) => criarSugestao(e.currentTarget.getBoundingClientRect())}
              >
                Criar
              </Botao>
            </div>
          ) : (
            <Lista rotulo="Planos">
              {visiveis.map((p) => (
                <ItemLista
                  key={p.id}
                  titulo={
                    <>
                      {p.tipoNome}
                      <span className="plano-regra">
                        {' '}
                        · {textoIntervalo(p.cadaKm, p.cadaMeses)} · {textoAlvo(p, matriculas)}
                      </span>
                    </>
                  }
                  fim={
                    <span className="item-fim">
                      <Botao
                        tamanho="pequeno"
                        disabled={soConsulta}
                        aria-label={`Editar ${textoPlano(p, matriculas)}`}
                        onClick={(e) =>
                          setFolha({
                            aberta: true,
                            origem: e.currentTarget.getBoundingClientRect(),
                            plano: p,
                            inicial: null,
                          })
                        }
                      >
                        Editar
                      </Botao>
                      <Botao
                        tamanho="pequeno"
                        variante="perigo"
                        disabled={soConsulta}
                        aria-label={`Apagar ${textoPlano(p, matriculas)}`}
                        onClick={() => apagar(p)}
                      >
                        Apagar
                      </Botao>
                    </span>
                  }
                />
              ))}
            </Lista>
          )}
        </Cartao>

        <div className="planos-lateral">
          <CartaoLimiares versao={versao} soConsulta={soConsulta} />
          {dados && <CartaoTipos tipos={dados.tipos} soConsulta={soConsulta} onAlterado={recarregar} />}
        </div>
      </div>

      <Folha aberta={folha.aberta} origem={folha.origem} onFechar={fechar} rotulo={titulo}>
        <CabecalhoFolha titulo={titulo} onFechar={fechar} />
        {dados && (
          <FormPlano
            key={folha.plano?.id ?? 'novo'}
            plano={folha.plano}
            inicial={folha.inicial}
            tipos={dados.tipos}
            carros={dados.carros}
            onGuardado={() => {
              fechar()
              recarregar()
            }}
            onCancelar={fechar}
          />
        )}
      </Folha>
    </>
  )
}

function FormPlano({
  plano,
  inicial,
  tipos,
  carros,
  onGuardado,
  onCancelar,
}: {
  plano: Plano | null
  inicial: Inicial | null
  tipos: TipoManutencao[]
  carros: Carro[]
  onGuardado(): void
  onCancelar(): void
}) {
  const ativos = tipos.filter((t) => t.ativo || t.id === plano?.tipoId)
  const base = plano ?? inicial
  const [tipoId, setTipoId] = useState(String(base?.tipoId ?? ativos[0]?.id ?? ''))
  const [km, setKm] = useState(base?.cadaKm != null ? String(base.cadaKm) : '')
  const [meses, setMeses] = useState(base?.cadaMeses != null ? String(base.cadaMeses) : '')
  const [alvo, setAlvo] = useState<'todos' | 'escolher'>(plano && !plano.todosCarros ? 'escolher' : 'todos')
  const [escolhidos, setEscolhidos] = useState<ReadonlySet<number>>(new Set(plano?.carros ?? []))
  const [erros, setErros] = useState<ErrosCampos>({})
  const [aGuardar, setAGuardar] = useState(false)

  // Carros que podem ser escolhidos: os ativos e os que o plano já tinha.
  const opcoes = carros.filter((c) => c.estado === 'ativo' || escolhidos.has(c.id))

  async function submeter(e: FormEvent) {
    e.preventDefault()
    const cadaKm = km.trim() ? lerKm(km) : null
    const cadaMeses = meses.trim() ? lerKm(meses) : null
    const locais: ErrosCampos = {}
    if (km.trim() && cadaKm === null) locais.km = MSG_KM
    if (meses.trim() && cadaMeses === null) locais.meses = MSG_MESES
    if (!tipoId) locais.tipo = 'Escolha o tipo de manutenção.'
    if (Object.keys(locais).length > 0) {
      setErros(locais)
      return
    }
    const input: PlanoInput = {
      id: plano?.id ?? null,
      tipoId: Number(tipoId),
      cadaKm,
      cadaMeses,
      todosCarros: alvo === 'todos',
      carros: alvo === 'todos' ? [] : [...escolhidos],
    }
    setErros({})
    setAGuardar(true)
    try {
      await planoGuardar(input)
      onGuardado()
    } catch (err) {
      setErros(errosDe(err))
    } finally {
      setAGuardar(false)
    }
  }

  const erroIntervalo = erros.intervalo ?? erros.valor

  return (
    <form className="form" onSubmit={submeter} noValidate>
      <div className="form-campos">
        {erros.geral && (
          <p className="form-erro" role="alert">
            {erros.geral}
          </p>
        )}
        <CampoSelecao
          rotulo="Tipo de manutenção"
          value={tipoId}
          onChange={(e) => setTipoId(e.target.value)}
          erro={erros.tipo}
          data-foco-inicial
        >
          {ativos.map((t) => (
            <option key={t.id} value={t.id}>
              {t.ativo ? t.nome : `${t.nome} (inativo)`}
            </option>
          ))}
        </CampoSelecao>
        <div className="form-par">
          <Campo
            rotulo="A cada (km)"
            inputMode="numeric"
            placeholder="15 000"
            value={km}
            onChange={(e) => setKm(e.target.value)}
            erro={erros.km}
          />
          <Campo
            rotulo="A cada (meses)"
            inputMode="numeric"
            placeholder="12"
            value={meses}
            onChange={(e) => setMeses(e.target.value)}
            erro={erros.meses}
          />
        </div>
        {erroIntervalo ? (
          <p className="campo-erro" role="alert">
            {erroIntervalo}
          </p>
        ) : (
          <p className="campo-ajuda">Preencha os km, os meses ou ambos — conta o que chegar primeiro.</p>
        )}
        <Segmentado
          rotulo="Aplica a"
          opcoes={[
            ['todos', 'Todos os carros'],
            ['escolher', 'Escolher carros'],
          ]}
          valor={alvo}
          onMudar={setAlvo}
        />
        {alvo === 'escolher' && (
          <fieldset className="plano-carros" aria-invalid={erros.carros ? true : undefined}>
            <legend className="campo-rotulo">Carros</legend>
            {opcoes.length === 0 && <p className="texto-discreto">Não há carros ativos.</p>}
            {opcoes.map((c) => (
              <label key={c.id} className="interruptor">
                <input
                  type="checkbox"
                  checked={escolhidos.has(c.id)}
                  onChange={(e) =>
                    setEscolhidos((s) => {
                      const n = new Set(s)
                      if (e.target.checked) n.add(c.id)
                      else n.delete(c.id)
                      return n
                    })
                  }
                />
                {rotuloCarro(c)}
              </label>
            ))}
            {erros.carros && (
              <p className="campo-erro" role="alert">
                {erros.carros}
              </p>
            )}
          </fieldset>
        )}
      </div>
      <div className="form-rodape">
        <Botao onClick={onCancelar}>Cancelar</Botao>
        <Botao type="submit" variante="primario" disabled={aGuardar}>
          Guardar
        </Botao>
      </div>
    </form>
  )
}

function CartaoLimiares({ versao, soConsulta }: { versao: number; soConsulta: boolean }) {
  const { dados } = useCarregar(limiaresObter, versao)
  // Valores escritos pelo utilizador; `null` = ainda não mexeu (mostra os guardados).
  const [km, setKm] = useState<string | null>(null)
  const [dias, setDias] = useState<string | null>(null)
  const [erros, setErros] = useState<ErrosCampos>({})
  const [guardado, setGuardado] = useState(false)
  const textoKm = km ?? (dados ? String(dados.km) : '')
  const textoDias = dias ?? (dados ? String(dados.dias) : '')

  async function guardar(e: FormEvent) {
    e.preventDefault()
    setGuardado(false)
    const nKm = lerKm(textoKm)
    const nDias = lerKm(textoDias)
    const locais: ErrosCampos = {}
    if (nKm === null) locais.km = MSG_KM
    if (nDias === null) locais.dias = 'Indique os dias como número inteiro.'
    if (nKm === null || nDias === null) {
      setErros(locais)
      return
    }
    setErros({})
    try {
      await limiaresGuardar({ km: nKm, dias: nDias })
      setGuardado(true)
    } catch (err) {
      setErros(errosDe(err))
    }
  }

  const erroGeral = erros.geral ?? erros.valor
  return (
    <Cartao titulo="Quando avisar">
      <form className="cartao-corpo" onSubmit={guardar} noValidate>
        <p className="texto-discreto">Um plano passa a «próximo» quando faltam menos do que isto.</p>
        <div className="form-par">
          <Campo
            rotulo="Quilómetros"
            inputMode="numeric"
            value={textoKm}
            onChange={(e) => {
              setKm(e.target.value)
              setGuardado(false)
            }}
            erro={erros.km}
            disabled={!dados}
          />
          <Campo
            rotulo="Dias"
            inputMode="numeric"
            value={textoDias}
            onChange={(e) => {
              setDias(e.target.value)
              setGuardado(false)
            }}
            erro={erros.dias}
            disabled={!dados}
          />
        </div>
        {erroGeral && (
          <p className="form-erro" role="alert">
            {erroGeral}
          </p>
        )}
        <div className="aviso-linha">
          <Botao type="submit" tamanho="pequeno" disabled={soConsulta || !dados}>
            Guardar
          </Botao>
          {guardado && (
            <span className="texto-discreto" role="status">
              Guardado ✓
            </span>
          )}
        </div>
      </form>
    </Cartao>
  )
}

function CartaoTipos({
  tipos,
  soConsulta,
  onAlterado,
}: {
  tipos: TipoManutencao[]
  soConsulta: boolean
  onAlterado(): void
}) {
  const [aRenomear, setARenomear] = useState<number | null>(null)
  const [nome, setNome] = useState('')
  const [novo, setNovo] = useState('')
  const [erros, setErros] = useState<ErrosCampos>({})
  const [erroNovo, setErroNovo] = useState<ErrosCampos>({})

  async function renomear(e: FormEvent, t: TipoManutencao) {
    e.preventDefault()
    try {
      await tipoGuardar(t.id, nome)
      setARenomear(null)
      setErros({})
      onAlterado()
    } catch (err) {
      setErros(errosDe(err))
    }
  }

  async function alternar(t: TipoManutencao) {
    setErros({})
    try {
      await tipoAtivo(t.id, !t.ativo)
      onAlterado()
    } catch (err) {
      setErros(errosDe(err))
    }
  }

  async function adicionar(e: FormEvent) {
    e.preventDefault()
    try {
      await tipoGuardar(null, novo)
      setNovo('')
      setErroNovo({})
      onAlterado()
    } catch (err) {
      setErroNovo(errosDe(err))
    }
  }

  return (
    <Cartao titulo="Tipos de manutenção" semMargem contagem={tipos.length}>
      {erros.geral && aRenomear === null && (
        <p className="form-erro tipos-erro" role="alert">
          {erros.geral}
        </p>
      )}
      <ul className="lista" aria-label="Tipos de manutenção">
        {tipos.map((t) =>
          aRenomear === t.id ? (
            <li key={t.id} className="item-lista">
              <form className="item-corpo tipo-renomear" onSubmit={(e) => void renomear(e, t)} noValidate>
                <Campo
                  rotulo={`Novo nome para «${t.nome}»`}
                  value={nome}
                  onChange={(e) => setNome(e.target.value)}
                  erro={erros.nome ?? erros.geral}
                  autoFocus
                  onKeyDown={(e) => {
                    if (e.key === 'Escape') {
                      e.stopPropagation()
                      setARenomear(null)
                      setErros({})
                    }
                  }}
                />
                <span className="item-fim">
                  <Botao
                    tamanho="pequeno"
                    onClick={() => {
                      setARenomear(null)
                      setErros({})
                    }}
                  >
                    Cancelar
                  </Botao>
                  <Botao type="submit" tamanho="pequeno" variante="primario">
                    Guardar
                  </Botao>
                </span>
              </form>
            </li>
          ) : (
            <ItemLista
              key={t.id}
              titulo={<span className={t.ativo ? undefined : 'texto-discreto'}>{t.nome}</span>}
              fim={
                <span className="item-fim">
                  {!t.ativo && <span className="pilula pilula-neutra">Inativo</span>}
                  <Botao
                    tamanho="pequeno"
                    variante="simples"
                    disabled={soConsulta}
                    aria-label={`Renomear ${t.nome}`}
                    onClick={() => {
                      setErros({})
                      setNome(t.nome)
                      setARenomear(t.id)
                    }}
                  >
                    Renomear
                  </Botao>
                  <Botao
                    tamanho="pequeno"
                    variante="simples"
                    disabled={soConsulta}
                    aria-label={`${t.ativo ? 'Desativar' : 'Reativar'} ${t.nome}`}
                    onClick={() => void alternar(t)}
                  >
                    {t.ativo ? 'Desativar' : 'Reativar'}
                  </Botao>
                </span>
              }
            />
          ),
        )}
      </ul>
      <form className="tipo-novo" onSubmit={(e) => void adicionar(e)} noValidate>
        <Campo
          rotulo="Novo tipo"
          placeholder="Por exemplo, Escovas"
          value={novo}
          onChange={(e) => setNovo(e.target.value)}
          erro={erroNovo.nome ?? erroNovo.geral}
          disabled={soConsulta}
        />
        <Botao type="submit" disabled={soConsulta}>
          Adicionar
        </Botao>
      </form>
    </Cartao>
  )
}
