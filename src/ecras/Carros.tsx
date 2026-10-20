import { useCallback, useMemo, useState, type FormEvent } from 'react'
import { carroGuardar, carrosListar } from '../api/comandos'
import type { Carro, CarroInput, EstadoCarro } from '../api/tipos'
import { Botao } from '../ui/Botao'
import { Campo, CampoSelecao } from '../ui/Campo'
import { Cartao } from '../ui/Cartao'
import { CabecalhoFolha, Folha } from '../ui/Folha'
import { Matricula, Pilula } from '../ui/Lista'
import { formatarEuros, formatarKm } from '../util/formato'
import { ROTULO_ESTADO, errosDe, normalizar, useCarregar, type ErrosCampos } from './comum'
import '../estilo/ecras.css'

interface FormCarroProps {
  /** Se existir, edita este carro. */
  carro?: Carro
  onGuardado(id: number): void
  onCancelar?(): void
}

/** Formulário de carro (novo ou edição). Os erros do núcleo aparecem junto ao campo respetivo. */
export function FormCarro({ carro, onGuardado, onCancelar }: FormCarroProps) {
  const [matricula, setMatricula] = useState(carro?.matricula ?? '')
  const [marca, setMarca] = useState(carro?.marca ?? '')
  const [modelo, setModelo] = useState(carro?.modelo ?? '')
  const [ano, setAno] = useState(carro?.ano != null ? String(carro.ano) : '')
  const [cor, setCor] = useState(carro?.cor ?? '')
  const [kmInicial, setKmInicial] = useState(carro ? String(carro.kmInicial) : '')
  const [dataAquisicao, setDataAquisicao] = useState(carro?.dataAquisicao ?? '')
  const [estado, setEstado] = useState<EstadoCarro>(carro?.estado ?? 'ativo')
  const [dataSaida, setDataSaida] = useState(carro?.dataSaida ?? '')
  const [notas, setNotas] = useState(carro?.notas ?? '')
  const [erros, setErros] = useState<ErrosCampos>({})
  const [aGuardar, setAGuardar] = useState(false)

  async function submeter(e: FormEvent) {
    e.preventDefault()
    const km = kmInicial.trim() === '' ? 0 : Number(kmInicial.replace(/[\s.]/g, ''))
    if (!Number.isInteger(km) || km < 0) {
      setErros({ km: 'Indique os quilómetros como número inteiro.' })
      return
    }
    const anoNum = ano.trim() === '' ? null : Number(ano)
    if (anoNum !== null && !Number.isInteger(anoNum)) {
      setErros({ geral: 'O ano do carro tem de ser um número.' })
      return
    }
    const input: CarroInput = {
      id: carro?.id ?? null,
      matricula: matricula.trim(),
      marca: marca.trim() || null,
      modelo: modelo.trim() || null,
      ano: anoNum,
      cor: cor.trim() || null,
      kmInicial: km,
      dataAquisicao: dataAquisicao || null,
      estado,
      dataSaida: estado === 'ativo' ? null : dataSaida || null,
      notas: notas.trim() || null,
    }
    setAGuardar(true)
    setErros({})
    try {
      onGuardado(await carroGuardar(input))
    } catch (err) {
      setErros(errosDe(err))
    } finally {
      setAGuardar(false)
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
        <Campo
          rotulo="Matrícula"
          value={matricula}
          onChange={(e) => setMatricula(e.target.value)}
          erro={erros.matricula}
          autoComplete="off"
          spellCheck={false}
          data-foco-inicial
        />
        <div className="form-par">
          <Campo rotulo="Marca" value={marca} onChange={(e) => setMarca(e.target.value)} />
          <Campo rotulo="Modelo" value={modelo} onChange={(e) => setModelo(e.target.value)} />
        </div>
        <div className="form-par">
          <Campo
            rotulo="Ano"
            inputMode="numeric"
            value={ano}
            onChange={(e) => setAno(e.target.value)}
          />
          <Campo rotulo="Cor" value={cor} onChange={(e) => setCor(e.target.value)} />
        </div>
        <div className="form-par">
          <Campo
            rotulo="KM iniciais"
            inputMode="numeric"
            value={kmInicial}
            onChange={(e) => setKmInicial(e.target.value)}
            erro={erros.km}
          />
          <Campo
            rotulo="Data de aquisição"
            type="date"
            value={dataAquisicao}
            onChange={(e) => setDataAquisicao(e.target.value)}
            erro={erros.data}
          />
        </div>
        {carro && (
          <div className="form-par">
            <CampoSelecao
              rotulo="Estado"
              value={estado}
              onChange={(e) => setEstado(e.target.value as EstadoCarro)}
            >
              <option value="ativo">Ativo</option>
              <option value="inativo">Inativo</option>
              <option value="vendido">Vendido</option>
            </CampoSelecao>
            {estado !== 'ativo' && (
              <Campo
                rotulo="Data de saída"
                type="date"
                value={dataSaida}
                onChange={(e) => setDataSaida(e.target.value)}
              />
            )}
          </div>
        )}
        <Campo rotulo="Notas" value={notas} onChange={(e) => setNotas(e.target.value)} />
      </div>
      <div className="form-rodape">
        {onCancelar && (
          <Botao variante="secundario" onClick={onCancelar}>
            Cancelar
          </Botao>
        )}
        <Botao type="submit" variante="primario" disabled={aGuardar}>
          Guardar
        </Botao>
      </div>
    </form>
  )
}

interface CarrosProps {
  ano: number
  /** Texto da pesquisa global (matrícula, marca ou modelo). */
  pesquisa: string
  onAbrir(id: number): void
}

export function Carros({ ano, pesquisa, onAbrir }: CarrosProps) {
  const [mostrarTodos, setMostrarTodos] = useState(false)
  const [folha, setFolha] = useState<{ aberta: boolean; origem: DOMRect | null }>({
    aberta: false,
    origem: null,
  })
  const buscar = useCallback(() => carrosListar(!mostrarTodos, ano), [mostrarTodos, ano])
  const { dados, erro, recarregar } = useCarregar(buscar)

  const visiveis = useMemo(() => {
    const q = normalizar(pesquisa)
    if (!dados) return []
    if (!q) return dados
    return dados.filter(
      (c) =>
        normalizar(c.matricula).includes(q) ||
        normalizar(`${c.marca ?? ''}${c.modelo ?? ''}`).includes(q) ||
        normalizar(`${c.modelo ?? ''}${c.marca ?? ''}`).includes(q),
    )
  }, [dados, pesquisa])

  const fechar = () => setFolha((f) => ({ ...f, aberta: false }))

  return (
    <>
      <div className="barra-ferramentas">
        <label className="interruptor">
          <input
            type="checkbox"
            checked={mostrarTodos}
            onChange={(e) => setMostrarTodos(e.target.checked)}
          />
          Mostrar inativos/vendidos
        </label>
        <span className="espaco" />
        <Botao
          variante="primario"
          onClick={(e) =>
            setFolha({ aberta: true, origem: e.currentTarget.getBoundingClientRect() })
          }
        >
          Novo carro
        </Botao>
      </div>

      {erro && (
        <p className="form-erro" role="alert">
          {erro}
        </p>
      )}

      <Cartao semMargem>
        {dados && visiveis.length === 0 ? (
          <p className="vazio-lista">
            {pesquisa ? 'Nenhum carro corresponde à pesquisa.' : 'Ainda não há carros registados.'}
          </p>
        ) : (
          <div className="tabela-wrap">
            <table className="tabela">
              <thead>
                <tr>
                  <th>Matrícula</th>
                  <th>Marca / modelo</th>
                  <th className="num">KM</th>
                  <th>Motorista</th>
                  <th className="num">Gasto em {ano}</th>
                </tr>
              </thead>
              <tbody>
                {visiveis.map((c) => (
                  <tr key={c.id}>
                    <td>
                      <button type="button" onClick={() => onAbrir(c.id)}>
                        <Matricula>{c.matricula}</Matricula>
                        {c.estado !== 'ativo' && (
                          <>
                            {' '}
                            <Pilula severidade="proximo">{ROTULO_ESTADO[c.estado]}</Pilula>
                          </>
                        )}
                      </button>
                    </td>
                    <td>
                      <button type="button" onClick={() => onAbrir(c.id)}>
                        {[c.marca, c.modelo].filter(Boolean).join(' ') || '—'}
                      </button>
                    </td>
                    <td className="num">
                      <button type="button" onClick={() => onAbrir(c.id)}>
                        {formatarKm(c.kmAtual)}
                      </button>
                    </td>
                    <td>
                      <button type="button" onClick={() => onAbrir(c.id)}>
                        {c.motoristaAtual ? c.motoristaAtual[1] : <span className="esmaecido">—</span>}
                      </button>
                    </td>
                    <td className="num">
                      <button type="button" onClick={() => onAbrir(c.id)}>
                        {formatarEuros(c.gastoAno)}
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </Cartao>

      <Folha aberta={folha.aberta} origem={folha.origem} onFechar={fechar} rotulo="Novo carro">
        <CabecalhoFolha titulo="Novo carro" onFechar={fechar} />
        <FormCarro
          onGuardado={(id) => {
            fechar()
            recarregar()
            onAbrir(id)
          }}
          onCancelar={fechar}
        />
      </Folha>
    </>
  )
}
