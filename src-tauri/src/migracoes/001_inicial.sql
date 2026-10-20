-- Esquema inicial (spec §5). Valores em cêntimos; datas AAAA-MM-DD.

CREATE TABLE carros (
    id INTEGER PRIMARY KEY,
    matricula TEXT NOT NULL,
    marca TEXT,
    modelo TEXT,
    ano INTEGER,
    cor TEXT,
    km_inicial INTEGER NOT NULL DEFAULT 0 CHECK (km_inicial >= 0),
    data_aquisicao TEXT,
    estado TEXT NOT NULL DEFAULT 'ativo' CHECK (estado IN ('ativo', 'inativo', 'vendido')),
    data_saida TEXT,
    notas TEXT,
    criado_em TEXT NOT NULL DEFAULT (datetime('now')),
    atualizado_em TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (matricula)
);

CREATE TABLE motoristas (
    id INTEGER PRIMARY KEY,
    nome TEXT NOT NULL,
    telefone TEXT,
    carta_numero TEXT,
    carta_validade TEXT,
    ativo INTEGER NOT NULL DEFAULT 1 CHECK (ativo IN (0, 1)),
    notas TEXT,
    criado_em TEXT NOT NULL DEFAULT (datetime('now')),
    atualizado_em TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE atribuicoes (
    id INTEGER PRIMARY KEY,
    carro_id INTEGER NOT NULL REFERENCES carros(id) ON DELETE RESTRICT,
    motorista_id INTEGER NOT NULL REFERENCES motoristas(id) ON DELETE RESTRICT,
    de TEXT NOT NULL,
    ate TEXT,
    criado_em TEXT NOT NULL DEFAULT (datetime('now')),
    atualizado_em TEXT NOT NULL DEFAULT (datetime('now')),
    CHECK (ate IS NULL OR ate >= de)
);
CREATE INDEX idx_atribuicoes_carro ON atribuicoes (carro_id, de);
CREATE INDEX idx_atribuicoes_motorista ON atribuicoes (motorista_id);

CREATE TABLE leituras_km (
    id INTEGER PRIMARY KEY,
    carro_id INTEGER NOT NULL REFERENCES carros(id) ON DELETE RESTRICT,
    data TEXT NOT NULL,
    km INTEGER NOT NULL CHECK (km >= 0),
    origem TEXT NOT NULL DEFAULT 'manual' CHECK (origem IN ('manual', 'manutencao', 'pneu')),
    origem_id INTEGER,
    criado_em TEXT NOT NULL DEFAULT (datetime('now')),
    atualizado_em TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_leituras_km_carro_data ON leituras_km (carro_id, data);

CREATE TABLE tipos_manutencao (
    id INTEGER PRIMARY KEY,
    nome TEXT NOT NULL,
    ativo INTEGER NOT NULL DEFAULT 1 CHECK (ativo IN (0, 1)),
    criado_em TEXT NOT NULL DEFAULT (datetime('now')),
    atualizado_em TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (nome)
);

CREATE TABLE manutencoes (
    id INTEGER PRIMARY KEY,
    carro_id INTEGER NOT NULL REFERENCES carros(id) ON DELETE RESTRICT,
    data TEXT NOT NULL,
    km INTEGER CHECK (km IS NULL OR km >= 0),
    tipo_id INTEGER NOT NULL REFERENCES tipos_manutencao(id) ON DELETE RESTRICT,
    executada_por TEXT NOT NULL DEFAULT 'propria' CHECK (executada_por IN ('propria', 'oficina')),
    oficina TEXT,
    valor INTEGER NOT NULL CHECK (valor >= 0),
    valor_pecas INTEGER CHECK (valor_pecas IS NULL OR valor_pecas >= 0),
    valor_mao_obra INTEGER CHECK (valor_mao_obra IS NULL OR valor_mao_obra >= 0),
    descricao TEXT,
    criado_em TEXT NOT NULL DEFAULT (datetime('now')),
    atualizado_em TEXT NOT NULL DEFAULT (datetime('now')),
    CHECK (executada_por <> 'oficina' OR (oficina IS NOT NULL AND trim(oficina) <> '')),
    CHECK (valor_pecas IS NULL OR valor_mao_obra IS NULL OR valor = valor_pecas + valor_mao_obra),
    CHECK (executada_por <> 'propria' OR valor_mao_obra IS NULL OR valor_mao_obra = 0)
);
CREATE INDEX idx_manutencoes_carro_data ON manutencoes (carro_id, data);
CREATE INDEX idx_manutencoes_tipo ON manutencoes (tipo_id);

CREATE TABLE multas (
    id INTEGER PRIMARY KEY,
    carro_id INTEGER NOT NULL REFERENCES carros(id) ON DELETE RESTRICT,
    motorista_id INTEGER REFERENCES motoristas(id) ON DELETE RESTRICT,
    data_infracao TEXT NOT NULL,
    data_notificacao TEXT,
    condutor_identificado_em TEXT,
    infracao TEXT NOT NULL DEFAULT 'outra' CHECK (infracao IN
        ('velocidade', 'estacionamento', 'telemovel', 'semaforo', 'cinto', 'documentos', 'portagem', 'outra')),
    pontos INTEGER NOT NULL DEFAULT 0 CHECK (pontos BETWEEN 0 AND 12),
    valor INTEGER NOT NULL CHECK (valor >= 0),
    vencimento TEXT,
    pago_em TEXT,
    responsavel TEXT NOT NULL DEFAULT 'empresa' CHECK (responsavel IN ('empresa', 'colaborador', 'coparticipado')),
    valor_empresa INTEGER NOT NULL DEFAULT 0,
    empresa_adiantou INTEGER NOT NULL DEFAULT 0 CHECK (empresa_adiantou IN (0, 1)),
    reembolsado_em TEXT,
    descricao TEXT,
    criado_em TEXT NOT NULL DEFAULT (datetime('now')),
    atualizado_em TEXT NOT NULL DEFAULT (datetime('now')),
    CHECK (valor_empresa BETWEEN 0 AND valor)
);
CREATE INDEX idx_multas_carro_data ON multas (carro_id, data_infracao);
CREATE INDEX idx_multas_motorista ON multas (motorista_id);

CREATE TABLE pneus (
    id INTEGER PRIMARY KEY,
    carro_id INTEGER NOT NULL REFERENCES carros(id) ON DELETE RESTRICT,
    data TEXT NOT NULL,
    km INTEGER CHECK (km IS NULL OR km >= 0),
    posicao TEXT NOT NULL CHECK (posicao IN ('FE', 'FD', 'TE', 'TD', 'suplente', 'todos')),
    marca TEXT,
    modelo TEXT,
    motivo TEXT NOT NULL DEFAULT 'troca' CHECK (motivo IN ('troca', 'rodizio')),
    valor INTEGER NOT NULL DEFAULT 0 CHECK (valor >= 0),
    criado_em TEXT NOT NULL DEFAULT (datetime('now')),
    atualizado_em TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_pneus_carro_data ON pneus (carro_id, data);

CREATE TABLE documentos (
    id INTEGER PRIMARY KEY,
    carro_id INTEGER NOT NULL REFERENCES carros(id) ON DELETE RESTRICT,
    tipo TEXT NOT NULL CHECK (tipo IN ('IUC', 'inspecao', 'seguro', 'outro')),
    descricao TEXT,
    valor INTEGER NOT NULL DEFAULT 0 CHECK (valor >= 0),
    vencimento TEXT NOT NULL,
    pago_em TEXT,
    anual INTEGER NOT NULL DEFAULT 0 CHECK (anual IN (0, 1)),
    gerado_de_id INTEGER REFERENCES documentos(id) ON DELETE RESTRICT,
    criado_em TEXT NOT NULL DEFAULT (datetime('now')),
    atualizado_em TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_documentos_carro_vencimento ON documentos (carro_id, vencimento);

CREATE TABLE planos (
    id INTEGER PRIMARY KEY,
    tipo_id INTEGER NOT NULL REFERENCES tipos_manutencao(id) ON DELETE RESTRICT,
    cada_km INTEGER CHECK (cada_km IS NULL OR cada_km > 0),
    cada_meses INTEGER CHECK (cada_meses IS NULL OR cada_meses > 0),
    todos_carros INTEGER NOT NULL DEFAULT 1 CHECK (todos_carros IN (0, 1)),
    criado_em TEXT NOT NULL DEFAULT (datetime('now')),
    atualizado_em TEXT NOT NULL DEFAULT (datetime('now')),
    CHECK (cada_km IS NOT NULL OR cada_meses IS NOT NULL)
);

CREATE TABLE planos_carros (
    id INTEGER PRIMARY KEY,
    plano_id INTEGER NOT NULL REFERENCES planos(id) ON DELETE RESTRICT,
    carro_id INTEGER NOT NULL REFERENCES carros(id) ON DELETE RESTRICT,
    criado_em TEXT NOT NULL DEFAULT (datetime('now')),
    atualizado_em TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (plano_id, carro_id)
);

CREATE TABLE config (
    chave TEXT PRIMARY KEY,
    valor TEXT,
    criado_em TEXT NOT NULL DEFAULT (datetime('now')),
    atualizado_em TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO tipos_manutencao (nome) VALUES
    ('Mudança de óleo'),
    ('Filtros'),
    ('Travões'),
    ('Pneus/alinhamento'),
    ('Revisão geral'),
    ('Embraiagem'),
    ('Bateria'),
    ('Correia de distribuição'),
    ('Ar condicionado'),
    ('Chapa e pintura'),
    ('Outro');

INSERT INTO config (chave, valor) VALUES ('limiar_km', '1000'), ('limiar_dias', '30');
