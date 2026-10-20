import { useState } from 'react'
import { fazerCopia, listarVersoes } from '../api/comandos'
import type { EstadoArmazenamento, Versao } from '../api/tipos'
import { Botao } from '../ui/Botao'
import { Cartao } from '../ui/Cartao'
import { formatarDataHora, formatarQuando } from '../util/formato'
import { paraErroApi, useCarregar } from './comum'
import { ListaVersoes } from './Versoes'

const MSG_FORA_DO_DRIVE = 'Esta pasta não está no Google Drive — os dados não têm cópia na nuvem.'

/**
 * Backup: estado das cópias, pasta dos dados, "Fazer cópia agora" e restauro de versões.
 * `estado` vem da App (consultado a cada 5 s); `onRestaurado` faz todos os ecrãs recarregar.
 */
export function Backup({
  estado,
  versao = 0,
  onRestaurado,
}: {
  estado: EstadoArmazenamento | null
  versao?: number
  onRestaurado(): void
}) {
  const { dados: versoes, erro: erroVersoes, recarregar } = useCarregar(listarVersoes, versao)
  const [aCopiar, setACopiar] = useState(false)
  const [erroCopia, setErroCopia] = useState<string | null>(null)
  const [restaurada, setRestaurada] = useState<string | null>(null)
  const soConsulta = estado?.soConsulta ?? false

  async function copiar() {
    setACopiar(true)
    setErroCopia(null)
    setRestaurada(null)
    try {
      await fazerCopia()
      recarregar()
    } catch (e) {
      setErroCopia(paraErroApi(e).mensagem)
    } finally {
      setACopiar(false)
    }
  }

  function aoRestaurar(v: Versao) {
    setRestaurada(`Cópia de ${formatarDataHora(v.data)} restaurada ✓`)
    recarregar()
    onRestaurado()
  }

  const ultima = versoes?.[0]

  return (
    <div className="backup">
      <Cartao
        titulo="Cópias"
        acao={
          <Botao variante="primario" onClick={() => void copiar()} disabled={soConsulta || aCopiar}>
            {aCopiar ? 'A copiar…' : 'Fazer cópia agora'}
          </Botao>
        }
      >
        <div className="cartao-corpo backup-estado">
          {estado?.erro ? (
            <p className="form-erro" role="alert">
              Não foi possível guardar na pasta: {estado.erro}
            </p>
          ) : ultima ? (
            <p className="backup-ultima">
              Última cópia: {formatarQuando(ultima.data)} <span className="backup-ok">✓</span>
            </p>
          ) : (
            versoes && <p className="backup-ultima">Ainda não há cópias.</p>
          )}
          {erroCopia && (
            <p className="form-erro" role="alert">
              {erroCopia}
            </p>
          )}
          {restaurada && (
            <p className="backup-restaurada" role="status">
              {restaurada}
            </p>
          )}
          {soConsulta && (
            <p className="texto-discreto">Aberto só para consulta — não é possível fazer cópias nem restaurar.</p>
          )}
          <p className="texto-discreto">
            O sistema guarda uma cópia por dia, ao fechar, e outra antes de cada atualização do programa.
          </p>
        </div>
      </Cartao>

      <Cartao titulo="Pasta dos dados">
        <div className="cartao-corpo">
          <code className="backup-pasta">{estado?.pasta ?? '—'}</code>
          {estado && !estado.noGoogleDrive && (
            <p className="backup-aviso" role="note">
              {MSG_FORA_DO_DRIVE}
            </p>
          )}
        </div>
      </Cartao>

      <Cartao titulo="Versões" semMargem contagem={versoes?.length}>
        {erroVersoes && (
          <p className="form-erro versoes-erro" role="alert">
            {erroVersoes}
          </p>
        )}
        {versoes && (
          <ListaVersoes
            versoes={versoes}
            rotuloBotao="Restaurar…"
            textoConfirmacao="O estado atual é guardado antes de restaurar."
            desativado={soConsulta}
            onRestaurada={aoRestaurar}
          />
        )}
      </Cartao>

      {estado && <p className="backup-versao texto-discreto">Frota · versão {estado.versaoApp}</p>}
    </div>
  )
}
