# Gestão de Frota — Especificação (etapa 1)

Data: 2026-10-20 · Estado: para revisão

## 1. Objetivo

Sistema local, para um único utilizador (o dono de uma empresa de entregas em Portugal, 10–50
carros próprios), para registar carros, KM, manutenções, pneus, documentos/taxas e motoristas,
receber alertas de vencimentos por KM e por data, e ver métricas de gasto (frota por ano, carro
por período, por motorista).

**Critérios de sucesso**

- O utilizador abre o sistema com um duplo clique num `.exe`, sem instalar nem configurar nada.
- O histórico nunca se perde: nem por avaria do PC, nem por atualização do programa.
- Ao abrir, vê de imediato o que está vencido ou a vencer.
- Responde em segundos a "quanto gastei com a frota este ano?" e "quanto gastou o carro X entre
  março e junho?".

**Fora do âmbito desta etapa:** abastecimentos (etapa 2, integração com a app de frota da Prio),
exportação (Excel/PDF), anexos de faturas, login, vários utilizadores, app para motoristas.

## 2. Distribuição, dados e backup

### 2.1 Pasta no Google Drive

```
G:\O meu disco\Frota\        (pasta marcada "Disponível offline" no Google Drive)
  Frota.exe                  programa (único ficheiro, ~10 MB)
  dados\frota.db             banco SQLite — fonte da verdade
  dados\versoes\             cópias datadas
  dados\frota.lock           indica que o sistema está aberto (e em que PC)
```

O Google Drive para computador sincroniza a pasta; o backup na nuvem é automático. O Drive
mantém também histórico de versões dos ficheiros (segunda rede de segurança).

O `.exe` usa a pasta onde está como raiz (modo portátil). Se for executado fora de uma pasta do
Google Drive, funciona na mesma e mostra um aviso discreto em Backup: "Esta pasta não está no
Google Drive — os dados não têm cópia na nuvem."

### 2.2 Gravação segura (o Drive nunca vê um ficheiro a meio)

1. Ao abrir: copia `dados\frota.db` para uma cópia de trabalho local
   (`%LOCALAPPDATA%\Frota\trabalho.db`) e corre `PRAGMA integrity_check`.
2. O sistema lê e escreve só na cópia de trabalho.
3. Após cada alteração (agrupadas, ~2 s depois da última) e ao fechar: `VACUUM INTO
   dados\frota.db.tmp` → flush → substituição atómica de `frota.db` (`MoveFileEx` com
   `REPLACE_EXISTING`).
4. Se a gravação na pasta falhar (Drive indisponível, disco cheio), os dados continuam seguros na
   cópia de trabalho; o sistema mostra aviso e tenta de novo a cada minuto. Enquanto houver
   gravação pendente existe o marcador `%LOCALAPPDATA%\Frota\pendente`; ao abrir, se o marcador
   existir, a cópia de trabalho é que vale e é gravada primeiro na pasta.
5. Se `integrity_check` falhar ao abrir: não toca em nada e propõe restaurar a versão mais
   recente válida de `versoes\`.

### 2.3 Versões

- Uma cópia por dia, no primeiro fecho do dia: `versoes\frota_AAAA-MM-DD.db`. Guardam-se as
  últimas 30 diárias.
- Antes de cada atualização do programa: `versoes\frota_antes-da-vX.Y.Z.db` (guardam-se as 5
  mais recentes).
- Ecrã Backup: "Última cópia: hoje, 18:42 ✓", botões "Fazer cópia agora" e "Restaurar…" (lista
  de versões com data e n.º de registos; restaurar pede confirmação e faz antes uma cópia do
  estado atual).

### 2.4 Bloqueio entre computadores

`frota.lock` contém nome do PC e hora, renovada a cada 30 s. Ao abrir, se existir um lock de
outro PC renovado há menos de 2 min: "O sistema está aberto no PC «X». Feche-o lá primeiro."
(com opção "Abrir só para consulta"). Lock mais antigo do que 2 min é considerado abandonado.

## 3. Atualização automática (GitHub)

- Código num repositório **público** no GitHub. `dados\` está no `.gitignore`; os dados do
  utilizador nunca vão para o Git.
- Ao criar uma tag `vX.Y.Z`, um workflow do GitHub Actions compila o `Frota.exe` para Windows,
  assina-o (chave de assinatura própria, guardada nos *secrets* do repositório; a chave pública
  vai embutida no programa) e publica numa *Release* com um `latest.json`
  (versão, notas, URL, assinatura, SHA-256).
- Ao abrir, o sistema consulta `latest.json` (timeout 5 s). Sem internet ou erro: abre
  normalmente, sem aviso.
- Se houver versão mais recente: ecrã "A atualizar para a versão X.Y.Z…" →
  1. descarrega para `Frota.new.exe`;
  2. verifica assinatura e SHA-256 (falha → apaga e abre a versão atual);
  3. grava os dados e faz a cópia `frota_antes-da-vX.Y.Z.db`;
  4. renomeia `Frota.exe` → `Frota.old.exe` (o Windows permite renomear um executável em uso),
     `Frota.new.exe` → `Frota.exe`, reinicia;
  5. a nova versão apaga `Frota.old.exe` ao arrancar.
- **Migrações do banco:** numeradas, guardadas em `PRAGMA user_version`; cada uma corre dentro de
  uma transação (falha → rollback, banco intacto, mensagem de erro). Só acrescentam (tabelas,
  colunas, índices); nunca apagam dados.
- Se o banco tiver `user_version` maior do que o programa conhece (programa antigo), o sistema
  abre só para consulta e avisa.

## 4. Funcionalidades e ecrãs

Barra lateral com o logo no topo: Início · Carros · Motoristas · Relatórios · Planos de
manutenção · Backup. Botão "+ Lançar" sempre visível (Ctrl+N). Pesquisa por matrícula (Ctrl+K).

### 4.1 Início

- Cartões: Gasto este mês · Gasto no ano · Por pagar · Carros ativos.
- **Próximos vencimentos**, ordenados por urgência (vencidos a vermelho, próximos a laranja):
  manutenções por plano, documentos por pagar, cartas de condução.
- Últimos lançamentos (10).

### 4.2 Carros

- Lista: matrícula, marca/modelo, KM atual, motorista atual, gasto no ano; pesquisa; filtro por
  estado (por omissão só ativos).
- Ficha: dados do carro, métricas (gasto total, gasto no ano, gasto por categoria) e separadores
  Histórico (linha do tempo com tudo) · Manutenções · Pneus · Documentos · Motoristas · KM.

### 4.3 Lançar (painel que desliza a partir do botão "+")

Tipos: **Manutenção** · **Pneus** · **Documento/taxa** · **Atualizar KM**. Escolhe-se primeiro o
carro (pesquisa por matrícula); campos conforme §5. Lançamentos com KM criam uma leitura de KM.

### 4.4 Motoristas

Cadastro e ficha com carros atribuídos (histórico de/até) e gasto atribuído no período.
Atribuir/trocar motorista de um carro: fecha a atribuição atual (até = data da troca) e abre a
nova.

### 4.5 Relatórios

- Período: este mês · mês anterior · este ano · ano anterior · personalizado (de/até).
- Filtros: carro(s), motorista.
- Total no período e por categoria (manutenção, pneus, IUC, inspeção, seguro, multas, outros).
- Gráfico mensal de gasto (barras empilhadas por categoria).
- Ranking de carros por gasto no período.
- Gasto por motorista.
- Tipos de manutenção mais caros.

### 4.6 Planos de manutenção

Regras "tipo X a cada N km e/ou a cada M meses", aplicadas a todos os carros ou a carros
escolhidos. Configuração dos limiares "próximo" (por omissão 1.000 km / 30 dias).

### 4.7 Backup

Ver §2.3. Mostra também a pasta atual e a versão do programa.

### 4.8 Primeira execução

Ecrã de boas-vindas → registar o primeiro carro. Tipos de manutenção pré-carregados (editáveis):
Mudança de óleo, Filtros, Travões, Pneus/alinhamento, Revisão geral, Embraiagem, Bateria,
Correia de distribuição, Ar condicionado, Chapa e pintura, Outro. Sem planos pré-definidos (o
utilizador cria os seus; o ecrã sugere "Mudança de óleo a cada 15.000 km ou 12 meses").

## 5. Modelo de dados (SQLite)

Valores em cêntimos (inteiro). Datas `AAAA-MM-DD`. Todas as tabelas têm `id`, `criado_em`,
`atualizado_em`. Apagar é definitivo, mas o ecrã oferece "Desfazer" durante ~8 s.

| Tabela | Campos |
|---|---|
| `carros` | matricula (única, normalizada `AA-00-AA`), marca, modelo, ano, cor, km_inicial, data_aquisicao, estado (`ativo`/`inativo`/`vendido`), data_saida, notas |
| `motoristas` | nome, telefone, carta_numero, carta_validade, ativo, notas |
| `atribuicoes` | carro_id, motorista_id, de, ate (nulo = atual). Sem sobreposição por carro |
| `leituras_km` | carro_id, data, km, origem (`manual`/`manutencao`/`pneu`), origem_id |
| `tipos_manutencao` | nome (único), ativo |
| `manutencoes` | carro_id, data, km, tipo_id, oficina, valor, descricao |
| `pneus` | carro_id, data, km, posicao (`FE`/`FD`/`TE`/`TD`/`suplente`/`todos`), marca, modelo, motivo (`troca`/`rodizio`), valor |
| `documentos` | carro_id, tipo (`IUC`/`inspecao`/`seguro`/`multa`/`outro`), descricao, valor, vencimento, pago_em (nulo = por pagar), data_ocorrencia (só multas), anual (bool), gerado_de_id |
| `planos` | tipo_id, cada_km (nulo), cada_meses (nulo; pelo menos um preenchido), todos_carros (bool) |
| `planos_carros` | plano_id, carro_id (quando não é para todos) |
| `config` | chave, valor (limiares de alerta, etc.) |
| `abastecimentos` | *(etapa 2 — reservada; não criada nesta etapa)* |

## 6. Regras de cálculo

1. **KM atual** = maior `km` em `leituras_km` do carro (ou `km_inicial` se não houver). Ao lançar
   KM menor do que a leitura anterior mais próxima por data: aviso + confirmação explícita.
2. **Gasto num período [de, ate]** = Σ `manutencoes.valor` com `data` no período + Σ
   `pneus.valor` com `data` no período + Σ `documentos.valor` com `pago_em` no período.
   **Por pagar** = Σ `documentos.valor` com `pago_em` nulo (não entra no gasto).
3. **Gasto por motorista**: cada despesa é atribuída ao motorista com atribuição ativa no carro na
   data de referência da despesa. Sem atribuição → "Sem motorista". Data de referência:
   manutenções e pneus → `data`; multas → `data_ocorrencia` (data da infração; obrigatória para
   multas); restantes documentos → `pago_em` (por pagar não contam). Numa troca de motorista no
   próprio dia, conta o motorista cuja atribuição começa nesse dia.
4. **Alertas de manutenção** (por plano × carro ativo):
   - Base = última `manutencoes` desse tipo no carro (km, data); se não houver, `km_inicial` e
     `data_aquisicao` (se não houver data de aquisição, a data de registo do carro).
   - Próximo KM = base.km + cada_km; próxima data = base.data + cada_meses.
   - **Vencido** se KM atual ≥ próximo KM **ou** hoje ≥ próxima data.
   - **Próximo** se faltar < limiar_km **ou** < limiar_dias.
   - Lançar uma manutenção desse tipo muda a base, o que "zera" o alerta.
5. **Documentos**: alerta "próximo" quando por pagar e o vencimento é daqui a ≤ 30 dias; "vencido"
   quando por pagar e o vencimento já passou. Ao marcar como pago um documento `anual`, cria-se o
   documento do ano seguinte (mesmo tipo, valor sugerido igual, vencimento +1 ano, por pagar,
   `gerado_de_id`), editável.
6. **Carta de condução**: "próximo" a ≤ 60 dias de `carta_validade`; "vencido" após. Só motoristas
   ativos.
7. **Carros inativos/vendidos**: fora dos alertas e das listas por omissão; despesas continuam nos
   relatórios dos períodos em que existiram.

## 7. Arquitetura

- **Tauri 2** (Rust) → `Frota.exe` portátil; WebView2 do Windows.
- **Núcleo em Rust**, em módulos independentes e testados:
  - `armazenamento` — cópia de trabalho, gravação atómica, versões, lock, restauro.
  - `migracoes` — esquema e migrações numeradas.
  - `repositorio` — CRUD por tabela (rusqlite).
  - `regras` — KM atual, gastos, atribuição a motorista, alertas, documentos anuais (funções
    puras sobre dados lidos; o centro dos testes).
  - `atualizacao` — consulta de `latest.json`, verificação de assinatura, troca do `.exe`.
- **Interface**: React + TypeScript (Vite), animações com Motion; comunica com o núcleo por
  comandos Tauri (`invoke`). A interface não calcula regras de negócio; só apresenta.
- **Gráficos**: componente SVG próprio (barras empilhadas e ranking), sem biblioteca pesada.

## 8. Visual (estilo Apple, adaptado ao Windows 11)

- Barra lateral com material Mica do Windows 11 (fallback: cor sólida suave); conteúdo em
  superfície clara/limpa. Cabeçalhos fixos translúcidos com desfoque e fade na borda, sem linhas
  divisórias rígidas.
- Fonte do sistema (Segoe UI Variable, `system-ui`); títulos e números grandes com tracking
  negativo (~-0.02em) e line-height apertado; algarismos tabulares nos valores.
- Cor de destaque tirada do logo; vermelho/laranja só para alertas. Tema claro/escuro segue o
  Windows.
- Movimento: molas criticamente amortecidas (sem ressalto), resposta ~0,3–0,4 s. O painel Lançar
  nasce e regressa ao botão "+". Feedback no clique (pressed), nada bloqueia o rato,
  animações interrompíveis.
- `prefers-reduced-motion` → fades curtos; `prefers-reduced-transparency` → superfícies sólidas;
  `prefers-contrast` → bordas definidas.
- Validação enquanto se escreve; "Desfazer" após apagar; confirmação só para ações irreversíveis
  (restaurar backup).
- Mensagens de erro em linguagem simples, a dizer o que aconteceu, se os dados estão seguros e o
  que o sistema vai fazer.
- Janela mínima 1100×700; atalhos Ctrl+N (lançar), Ctrl+K (pesquisar), Esc (fechar painel).

## 9. Testes

- Rust (`cargo test`): todas as regras de §6 com casos limite (sem leituras, KM a descer, troca
  de motorista no dia da despesa, plano só por km / só por meses, documento anual pago, carro
  vendido a meio do período); gravação atómica (falha simulada a meio não corrompe `frota.db`);
  migrações (banco antigo → atual sem perda; falha → rollback); lock (ativo vs abandonado);
  verificação de assinatura da atualização (válida, inválida, SHA errado).
- Interface: testes de componentes dos formulários (validação) com Vitest.
- Verificação manual antes de cada release: abrir a partir de uma pasta do Drive, lançar,
  fechar, confirmar `frota.db` e versão diária; atualizar de vX para vX+1 com dados reais de
  teste.

## 10. Dependências externas e pré-requisitos

- No PC do utilizador: Windows 10/11 com WebView2 (já incluído no Windows 11) e Google Drive
  para computador com a pasta Frota "Disponível offline". Passo a passo de 1 minuto incluído.
- No desenvolvimento: Rust, Node.js, conta GitHub (repositório público), chave de assinatura de
  atualizações.
- **Logo**: é preciso exportar `Logo.cdr` para SVG (ou PNG transparente ≥ 1024 px); usado na
  barra lateral, no ecrã de boas-vindas e no ícone do `.exe`.

## 11. Etapa 2 (registo, fora do âmbito)

Abastecimentos importados da app de frota da Prio (tabela `abastecimentos`: carro, data, km,
litros, valor, posto, origem), métricas de consumo (L/100 km) e custo por km.
