use crate::erro::{Erro, Res};
use rusqlite::Connection;

pub const MIGRACOES: &[&str] = &[
    include_str!("001_inicial.sql"),
    include_str!("002_pneus_posicoes.sql"),
];

pub fn versao_esquema(conn: &Connection) -> Res<i64> {
    Ok(conn.query_row("PRAGMA user_version", [], |r| r.get(0))?)
}

pub fn migrar(conn: &mut Connection) -> Res<()> {
    migrar_com(conn, MIGRACOES)
}

pub fn migrar_com(conn: &mut Connection, migracoes: &[&str]) -> Res<()> {
    let atual = versao_esquema(conn)?;
    if atual < 0 {
        // `skip(atual as usize)` com um negativo saltaria todas as migrações.
        return Err(Erro {
            campo: None,
            codigo: "corrompido".into(),
            mensagem: "O ficheiro de dados tem uma versão de esquema inválida. Nada foi alterado."
                .into(),
        });
    }
    if atual > migracoes.len() as i64 {
        return Err(Erro { campo: None,
            codigo: "versao_mais_recente".into(),
            mensagem: "Estes dados foram criados por uma versão mais recente do programa. Atualize o programa antes de os abrir.".into(),
        });
    }
    for (i, sql) in migracoes.iter().enumerate().skip(atual as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.execute_batch(&format!("PRAGMA user_version = {}", i + 1))?;
        tx.commit()?;
    }
    Ok(())
}

#[cfg(test)]
mod testes {
    use super::*;

    fn mem() -> Connection {
        Connection::open_in_memory().unwrap()
    }

    fn contar(c: &Connection, sql: &str) -> i64 {
        c.query_row(sql, [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn banco_vazio_fica_na_versao_atual() {
        let mut c = mem();
        migrar(&mut c).unwrap();
        assert_eq!(versao_esquema(&c).unwrap(), MIGRACOES.len() as i64);
        assert_eq!(contar(&c, "SELECT COUNT(*) FROM tipos_manutencao"), 11);
        let v: String = c
            .query_row(
                "SELECT valor FROM config WHERE chave='limiar_km'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(v, "1000");
        let d: String = c
            .query_row(
                "SELECT valor FROM config WHERE chave='limiar_dias'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(d, "30");
    }

    #[test]
    fn migrar_duas_vezes_nao_altera_nada() {
        let mut c = mem();
        migrar(&mut c).unwrap();
        migrar(&mut c).unwrap();
        assert_eq!(versao_esquema(&c).unwrap(), MIGRACOES.len() as i64);
        assert_eq!(contar(&c, "SELECT COUNT(*) FROM tipos_manutencao"), 11);
    }

    #[test]
    fn migracao_que_falha_faz_rollback() {
        let mut c = mem();
        let r = migrar_com(&mut c, &[MIGRACOES[0], "CREATE TABLE x(a); SELEC lixo;"]);
        assert!(r.is_err());
        assert_eq!(versao_esquema(&c).unwrap(), 1);
        assert_eq!(
            contar(&c, "SELECT COUNT(*) FROM sqlite_master WHERE name='x'"),
            0
        );
    }

    #[test]
    fn pneus_antigos_passam_a_lista_de_posicoes() {
        let mut c = mem();
        migrar_com(&mut c, &MIGRACOES[..1]).unwrap();
        c.execute_batch(
            "INSERT INTO carros(matricula) VALUES ('AA-00-AA');
             INSERT INTO pneus(id,carro_id,data,posicao,valor) VALUES (7,1,'2026-01-01','todos',100);
             INSERT INTO pneus(id,carro_id,data,posicao,valor) VALUES (8,1,'2026-02-01','FD',50);",
        )
        .unwrap();
        migrar(&mut c).unwrap();
        let v: Vec<(i64, String, i64)> = c
            .prepare("SELECT id, posicao, valor FROM pneus ORDER BY id")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(v, [(7, "FE,FD,TE,TD".into(), 100), (8, "FD".into(), 50)]);
        assert!(
            c.execute(
                "INSERT INTO pneus(carro_id,data,posicao) VALUES (1,'2026-03-01','')",
                []
            )
            .is_err()
        );
        assert_eq!(
            contar(
                &c,
                "SELECT COUNT(*) FROM sqlite_master WHERE name='idx_pneus_carro_data'"
            ),
            1
        );
    }

    #[test]
    fn user_version_negativo_e_recusado() {
        let mut c = mem();
        c.execute_batch("PRAGMA user_version=-1").unwrap();
        let e = migrar(&mut c).unwrap_err();
        assert_eq!(e.codigo, "corrompido");
        assert_eq!(versao_esquema(&c).unwrap(), -1);
    }

    #[test]
    fn banco_mais_recente_e_recusado() {
        let mut c = mem();
        c.execute_batch("PRAGMA user_version=99").unwrap();
        let e = migrar(&mut c).unwrap_err();
        assert_eq!(e.codigo, "versao_mais_recente");
    }

    #[test]
    fn restricoes_principais() {
        let mut c = mem();
        migrar(&mut c).unwrap();
        c.execute_batch("PRAGMA foreign_keys=ON").unwrap();
        c.execute("INSERT INTO carros(matricula) VALUES ('AA-00-AA')", [])
            .unwrap();
        assert!(
            c.execute("INSERT INTO carros(matricula) VALUES ('AA-00-AA')", [])
                .is_err()
        );
        assert!(
            c.execute(
                "INSERT INTO carros(matricula, estado) VALUES ('BB-00-BB','x')",
                []
            )
            .is_err()
        );
        // oficina obrigatória
        assert!(c
            .execute(
                "INSERT INTO manutencoes(carro_id,data,tipo_id,executada_por,valor) VALUES (1,'2026-01-01',1,'oficina',100)",
                []
            )
            .is_err());
        c.execute(
            "INSERT INTO manutencoes(carro_id,data,tipo_id,executada_por,oficina,valor) VALUES (1,'2026-01-01',1,'oficina','Auto',100)",
            [],
        )
        .unwrap();
        // multas
        assert!(c
            .execute("INSERT INTO multas(carro_id,data_infracao,infracao,pontos,valor,responsavel,valor_empresa) VALUES (1,'2026-01-01','velocidade',13,100,'empresa',0)", [])
            .is_err());
        assert!(c
            .execute("INSERT INTO multas(carro_id,data_infracao,infracao,pontos,valor,responsavel,valor_empresa) VALUES (1,'2026-01-01','velocidade',2,100,'empresa',101)", [])
            .is_err());
        // documentos: multa não é tipo
        assert!(c
            .execute("INSERT INTO documentos(carro_id,tipo,valor,vencimento) VALUES (1,'multa',1,'2026-01-01')", [])
            .is_err());
        // plano sem km nem meses
        assert!(
            c.execute("INSERT INTO planos(tipo_id) VALUES (1)", [])
                .is_err()
        );
        // RESTRICT
        assert!(c.execute("DELETE FROM carros WHERE id=1", []).is_err());
        assert_eq!(
            contar(
                &c,
                "SELECT COUNT(*) FROM sqlite_master WHERE name='abastecimentos'"
            ),
            0
        );
    }
}
