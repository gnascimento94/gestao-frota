use super::erro;
use crate::erro::Res;
use crate::regras::alertas::Limiares;
use rusqlite::{Connection, OptionalExtension, params};

const KM_OMISSAO: i64 = 1_000;
const DIAS_OMISSAO: i64 = 30;

fn ler(conn: &Connection, chave: &str, omissao: i64) -> Res<i64> {
    let v: Option<Option<String>> = conn
        .query_row("SELECT valor FROM config WHERE chave = ?1", [chave], |r| {
            r.get(0)
        })
        .optional()?;
    Ok(v.flatten()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(omissao))
}

fn gravar(conn: &Connection, chave: &str, valor: i64) -> Res<()> {
    conn.execute(
        "INSERT INTO config (chave, valor) VALUES (?1, ?2)
         ON CONFLICT (chave) DO UPDATE SET valor = excluded.valor, atualizado_em = datetime('now')",
        params![chave, valor.to_string()],
    )?;
    Ok(())
}

pub fn obter_limiares(conn: &Connection) -> Res<Limiares> {
    Ok(Limiares {
        km: ler(conn, "limiar_km", KM_OMISSAO)?,
        dias: ler(conn, "limiar_dias", DIAS_OMISSAO)?,
    })
}

pub fn guardar_limiares(conn: &Connection, l: &Limiares) -> Res<()> {
    if l.km < 0 || l.dias < 0 {
        return Err(erro(
            "valor_invalido",
            "Os limiares não podem ser negativos.",
        ));
    }
    let tx = conn.unchecked_transaction()?;
    gravar(&tx, "limiar_km", l.km)?;
    gravar(&tx, "limiar_dias", l.dias)?;
    tx.commit()?;
    Ok(())
}
