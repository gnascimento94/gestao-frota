-- Pneus: um lançamento pode ter vários pneus (normalmente trocam-se aos pares).
-- `posicao` passa a ser a lista das posições, separadas por vírgulas e pela ordem
-- FE, FD, TE, TD, suplente (p.ex. 'FE,FD'); quem a valida é o núcleo.
-- O antigo 'todos' passa a ser as quatro rodas.
CREATE TABLE pneus_novo (
    id INTEGER PRIMARY KEY,
    carro_id INTEGER NOT NULL REFERENCES carros(id) ON DELETE RESTRICT,
    data TEXT NOT NULL,
    km INTEGER CHECK (km IS NULL OR km >= 0),
    posicao TEXT NOT NULL CHECK (posicao <> ''),
    marca TEXT,
    modelo TEXT,
    motivo TEXT NOT NULL DEFAULT 'troca' CHECK (motivo IN ('troca', 'rodizio')),
    valor INTEGER NOT NULL DEFAULT 0 CHECK (valor >= 0),
    criado_em TEXT NOT NULL DEFAULT (datetime('now')),
    atualizado_em TEXT NOT NULL DEFAULT (datetime('now'))
);
INSERT INTO pneus_novo (id, carro_id, data, km, posicao, marca, modelo, motivo, valor,
                        criado_em, atualizado_em)
SELECT id, carro_id, data, km,
       CASE posicao WHEN 'todos' THEN 'FE,FD,TE,TD' ELSE posicao END,
       marca, modelo, motivo, valor, criado_em, atualizado_em
  FROM pneus;
DROP TABLE pneus;
ALTER TABLE pneus_novo RENAME TO pneus;
CREATE INDEX idx_pneus_carro_data ON pneus (carro_id, data);
