import { useCallback, useState, type FormEvent } from 'react'
import {
  atribuicoesListar,
  marcarReembolsado,
  motoristaApagar,
  motoristaGuardar,
  motoristasListar,
  multasListar,
  multasMotorista,
  relatorio,
} from '../api/comandos'
import type { Motorista, MotoristaInput } from '../api/tipos'
import { Botao } from '../ui/Botao'
import { Campo } from '../ui/Campo'
import { Cartao } from '../ui/Cartao'
import { CabecalhoFolha, Folha } from '../ui/Folha'
import { ItemLista, Lista, Matricula, Pilula, Valor } from '../ui/Lista'
import { formatarData, formatarEuros } from '../util/formato'
import { AcaoData } from './AcaoData'
import { errosDe, paraErroApi, useCarregar, type ErrosCampos } from './comum'

function FormMotorista({
  motorista,
  onGuardado,
  onCancelar,
}: {
  motorista?: Motorista
  onGuardado(id: number): void
  onCancelar(): void
}) {
  const [nome, setNome] = useState(motorista?.nome ?? '')
  const [telefone, setTelefone] = useState(motorista?.telefone ?? '')
  const [cartaNumero, setCartaNumero] = useState(motorista?.cartaNumero ?? '')
  const [cartaValidade, setCartaValidade] = useState(motorista?.cartaValidade ?? '')
  const [ativo, setAtivo] = useState(motorista?.ativo ?? true)
  const [notas, setNotas] = useState(motorista?.notas ?? '')
  const [erros, setErros] = useState<ErrosCampos>({})

  async function submeter(e: FormEvent) {
    e.preventDefault()
    const input: MotoristaInput = {
      id: motorista?.id ?? null,
      nome: nome.trim(),
      telefone: telefone.trim() || null,
      cartaNumero: cartaNumero.trim() || null,
      cartaValidade: cartaValidade || null,
      ativo,
      notas: notas.trim() || null,
    }
    setErros({})
    try {
      onGuardado(await motoristaGuardar(input))
    } catch (err) {
      setErros(errosDe(err))
    }
  }

  return (
    <form className="form" onSubmit={submeter} noValidate>
      <div className="form-campos">
        {erros.geral && (
          <p className="form-erro" role="alert">
            {erros.geral}
          </p>
        )}
        <Campo rotulo="Nome" value={nome} onChange={(e) => setNome(e.target.value)} data-foco-inicial />
        <Campo rotulo="Telefone" value={telefone} onChange={(e) => setTelefone(e.target.value)} />
        <div className="form-par">
          <Campo rotulo="N.º da carta" value={cartaNumero} onChange={(e) => setCartaNumero(e.target.value)} />
          <Campo
            rotulo="Validade da carta"
            type="date"
            value={cartaValidade}
            onChange={(e) => setCartaValidade(e.target.value)}
            erro={erros.data}
          />
        </div>
        <Campo rotulo="Notas" value={notas} onChange={(e) => setNotas(e.target.value)} />
        {motorista && (
          <label className="interruptor">
            <input type="checkbox" checked={ativo} onChange={(e) => setAtivo(e.target.checked)} />
            Ativo
          </label>
        )}
      </div>
      <div className="form-rodape">
        <Botao onClick={onCancelar}>Cancelar</Botao>
        <Botao type="submit" variante="primario">
          Guardar
        </Botao>
      </div>
    </form>
  )
}

/** `versao` muda após cada lançamento (folha Lançar): os dados voltam a ser pedidos. */
export function Motoristas({
  ano,
  versao = 0,
  inicial,
}: {
  ano: number
  versao?: number
  /** Motorista aberto à entrada (p.ex. vindo de um alerta da carta no Início). */
  inicial?: number
}) {
  const [mostrarTodos, setMostrarTodos] = useState(false)
  const [selecionado, setSelecionado] = useState<number | null>(inicial ?? null)
  const [folha, setFolha] = useState<{ aberta: boolean; origem: DOMRect | null; editar: boolean }>({
    aberta: false,
    origem: null,
    editar: false,
  })
  const buscar = useCallback(() => motoristasListar(!mostrarTodos), [mostrarTodos])
  const { dados, erro, recarregar } = useCarregar(buscar, versao)
  // O motorista pedido à entrada pode estar inativo: nesse caso passa-se a mostrar todos
  // (uma só vez; ajuste durante o render, sem efeito).
  const [procurarInativo, setProcurarInativo] = useState(inicial !== undefined)
  if (procurarInativo && dados) {
    setProcurarInativo(false)
    if (!mostrarTodos && !dados.some((m) => m.id === inicial)) setMostrarTodos(true)
  }
  const atual = dados?.find((m) => m.id === selecionado) ?? null
  const fechar = () => setFolha((f) => ({ ...f, aberta: false }))

  return (
    <>
      <div className="barra-ferramentas">
        <label className="interruptor">
          <input type="checkbox" checked={mostrarTodos} onChange={(e) => setMostrarTodos(e.target.checked)} />
          Mostrar inativos
        </label>
        <span className="espaco" />
        <Botao
          variante="primario"
          onClick={(e) =>
            setFolha({ aberta: true, origem: e.currentTarget.getBoundingClientRect(), editar: false })
          }
        >
          Novo motorista
        </Botao>
      </div>

      {erro && (
        <p className="form-erro" role="alert">
          {erro}
        </p>
      )}

      <div className="duas-colunas">
        <Cartao semMargem>
          {dados && dados.length === 0 ? (
            <p className="vazio-lista">Ainda não há motoristas registados.</p>
          ) : (
            <Lista rotulo="Motoristas">
              {(dados ?? []).map((m) => {
                const carta = m.alertaCarta
                return (
                  <ItemLista
                    key={m.id}
                    titulo={m.nome}
                    detalhe={m.telefone ?? undefined}
                    onClick={() => setSelecionado(m.id)}
                    selecionado={m.id === selecionado}
                    fim={
                      <span className="item-fim">
                        {!m.ativo && <span className="pilula pilula-neutra">Inativo</span>}
                        {carta && (
                          <Pilula severidade={carta}>{carta === 'vencido' ? 'Carta vencida' : 'Carta a vencer'}</Pilula>
                        )}
                      </span>
                    }
                  />
                )
              })}
            </Lista>
          )}
        </Cartao>

        {atual && (
          <FichaMotorista
            key={atual.id}
            motorista={atual}
            ano={ano}
            versao={versao}
            aoAlterar={recarregar}
            aoApagado={() => {
              setSelecionado(null)
              recarregar()
            }}
            aoEditar={(origem) => setFolha({ aberta: true, origem, editar: true })}
          />
        )}
      </div>

      <Folha
        aberta={folha.aberta}
        origem={folha.origem}
        onFechar={fechar}
        rotulo={folha.editar ? 'Editar motorista' : 'Novo motorista'}
      >
        <CabecalhoFolha titulo={folha.editar ? 'Editar motorista' : 'Novo motorista'} onFechar={fechar} />
        <FormMotorista
          motorista={folha.editar ? (atual ?? undefined) : undefined}
          onGuardado={(id) => {
            fechar()
            setSelecionado(id)
            recarregar()
          }}
          onCancelar={fechar}
        />
      </Folha>
    </>
  )
}

function FichaMotorista({
  motorista,
  ano,
  versao,
  aoAlterar,
  aoApagado,
  aoEditar,
}: {
  motorista: Motorista
  ano: number
  versao: number
  aoAlterar(): void
  aoApagado(): void
  aoEditar(origem: DOMRect): void
}) {
  const [aviso, setAviso] = useState<string | null>(null)
  const [erroGeral, setErroGeral] = useState<string | null>(null)

  const buscar = useCallback(async () => {
    const [gasto, multas, atribuicoes, lista] = await Promise.all([
      relatorio({ de: `${ano}-01-01`, ate: `${ano}-12-31`, motoristaId: motorista.id }),
      multasMotorista(motorista.id),
      atribuicoesListar(null, motorista.id),
      multasListar(null, motorista.id),
    ])
    // O estado de cada multa vem do núcleo; a lista concorda com o total "por reembolsar".
    const porReembolsar = lista.filter((x) => x.estadoMulta === 'porReembolsar')
    return { gastoAno: gasto.resumo.total, multas, atribuicoes, porReembolsar }
  }, [ano, motorista.id])
  const { dados, erro, recarregar } = useCarregar(buscar, versao)

  async function apagar() {
    setAviso(null)
    setErroGeral(null)
    try {
      await motoristaApagar(motorista.id)
      aoApagado()
    } catch (e) {
      const { codigo, mensagem } = paraErroApi(e)
      if (codigo === 'tem_historico') setAviso(mensagem)
      else setErroGeral(mensagem)
    }
  }

  async function marcarInativo() {
    try {
      await motoristaGuardar({
        id: motorista.id,
        nome: motorista.nome,
        telefone: motorista.telefone,
        cartaNumero: motorista.cartaNumero,
        cartaValidade: motorista.cartaValidade,
        ativo: false,
        notas: motorista.notas,
      })
      setAviso(null)
      aoAlterar()
    } catch (e) {
      setErroGeral(paraErroApi(e).mensagem)
    }
  }

  const carta = motorista.alertaCarta
  const m = dados?.multas

  return (
    <div className="conteudo" style={{ padding: 0 }}>
      {erro && (
        <p className="form-erro" role="alert">
          {erro}
        </p>
      )}
      <Cartao
        titulo={motorista.nome}
        acao={
          <span className="item-fim">
            <Botao tamanho="pequeno" onClick={(e) => aoEditar(e.currentTarget.getBoundingClientRect())}>
              Editar
            </Botao>
            <Botao tamanho="pequeno" variante="perigo" onClick={apagar}>
              Apagar
            </Botao>
          </span>
        }
      >
        <div className="cartao-corpo" style={{ padding: 0 }}>
          {aviso && (
            <div className="form-erro aviso-linha" role="alert">
              <span>{aviso}</span>
              <Botao tamanho="pequeno" onClick={marcarInativo}>
                Marcar como inativo
              </Botao>
            </div>
          )}
          {erroGeral && (
            <p className="form-erro" role="alert">
              {erroGeral}
            </p>
          )}
          <div className="ficha-sub">
            {motorista.telefone ?? 'Sem telefone'}
            {!motorista.ativo && ' · Inativo'}
          </div>
          <div className="aviso-linha">
            <span>
              Carta {motorista.cartaNumero ?? '—'}
              {motorista.cartaValidade && ` · válida até ${formatarData(motorista.cartaValidade)}`}
            </span>
            {carta && <Pilula severidade={carta}>{carta === 'vencido' ? 'Vencida' : 'A vencer'}</Pilula>}
          </div>
          <div className="metrica">
            <div className="metrica-rotulo">Gasto atribuído em {ano}</div>
            <div className="metrica-valor">{dados ? formatarEuros(dados.gastoAno) : '—'}</div>
          </div>
        </div>
      </Cartao>

      <Cartao titulo="Carros atribuídos" semMargem contagem={dados?.atribuicoes.length}>
        {dados && dados.atribuicoes.length === 0 ? (
          <p className="vazio-lista">Sem carros atribuídos.</p>
        ) : (
          <Lista rotulo="Carros atribuídos">
            {(dados?.atribuicoes ?? []).map((a) => (
              <ItemLista
                key={a.id}
                inicio={<Matricula>{a.matricula}</Matricula>}
                titulo={`${formatarData(a.de)} — ${a.ate ? formatarData(a.ate) : 'atual'}`}
                fim={a.ate ? undefined : <span className="pilula pilula-ok">Atual</span>}
              />
            ))}
          </Lista>
        )}
      </Cartao>

      <Cartao titulo="Multas">
        {m && (
          <div className="metricas">
            <Metrica rotulo="N.º de multas" valor={String(m.n)} />
            <Metrica rotulo="Total" valor={formatarEuros(m.total)} />
            <Metrica rotulo="Pago pela empresa" valor={formatarEuros(m.empresa)} />
            <Metrica rotulo="Pago por ele" valor={formatarEuros(m.colaborador)} />
            <Metrica rotulo="Por reembolsar" valor={formatarEuros(m.porReembolsar)} />
            <Metrica rotulo="Pontos (3 anos)" valor={String(m.pontos3Anos)} />
          </div>
        )}
        {dados && dados.porReembolsar.length > 0 && (
          <Lista rotulo="Multas por reembolsar">
            {dados.porReembolsar.map((i) => (
              <ItemLista
                key={i.id}
                inicio={<Matricula>{i.matricula}</Matricula>}
                titulo={`Multa (${i.infracao})`}
                detalhe={`${formatarData(i.dataInfracao)} · a reembolsar ${formatarEuros(i.valorColaborador)}`}
                fim={
                  <span className="item-fim">
                    <Valor>{formatarEuros(i.valor)}</Valor>
                    <AcaoData
                      rotulo="Marcar reembolsado"
                      rotuloData="Reembolsado em"
                      onConfirmar={async (data) => {
                        await marcarReembolsado(i.id, data)
                        recarregar()
                        aoAlterar()
                      }}
                    />
                  </span>
                }
              />
            ))}
          </Lista>
        )}
      </Cartao>
    </div>
  )
}

function Metrica({ rotulo, valor }: { rotulo: string; valor: string }) {
  return (
    <div className="metrica">
      <div className="metrica-rotulo">{rotulo}</div>
      <div className="metrica-valor">{valor}</div>
    </div>
  )
}
