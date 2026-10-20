# Gestão de Frota (etapa 1) — Plano de implementação

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `Frota.exe` portátil (Tauri) que regista carros, KM, manutenções, pneus, documentos e
motoristas num SQLite guardado com segurança numa pasta do Google Drive, mostra alertas e
relatórios de gasto, e atualiza-se sozinho a partir das Releases do GitHub.

**Architecture:** Núcleo Rust (armazenamento, migrações, repositório, regras puras, atualização)
exposto por comandos Tauri; interface React+TypeScript que só apresenta e recolhe dados. O banco
vivo é uma cópia de trabalho em `%LOCALAPPDATA%\Frota`; a pasta do Drive só recebe ficheiros
completos por substituição atómica.

**Tech Stack:** Rust stable, Tauri 2, rusqlite (feature `bundled`), chrono, serde, ureq,
sha2, minisign-verify, semver, tempfile (dev) · Node 24, Vite, React 18, TypeScript, `motion`,
Vitest + @testing-library/react.

**Spec:** `docs/superpowers/specs/2026-10-20-gestao-frota-design.md` (ler antes de qualquer tarefa).

## Global Constraints

- Português de Portugal em todo o texto visível (matrícula, carta de condução, IUC, Inspeção, travões, €).
- Valores monetários: inteiros em cêntimos (`i64` / `number` inteiro) do banco à interface; formatação `1 234,56 €` só no ecrã. Nunca `f64` para dinheiro.
- Datas `AAAA-MM-DD` (`chrono::NaiveDate`; `string` na interface).
- Regras de negócio só em Rust (`src-tauri/src/regras`); a interface não calcula gastos nem alertas.
- Limiares por omissão: manutenção "próximo" < 1000 km ou < 30 dias; documentos ≤ 30 dias; carta ≤ 60 dias.
- Versões: 30 diárias, 5 "antes-da"; lock renovado a cada 30 s, abandonado após 120 s; gravação ~2 s após a última alteração; consulta de atualização com timeout 5 s.
- Raiz de dados = pasta do `.exe` (`current_exe().parent()`); em desenvolvimento, variável `FROTA_RAIZ`.
- Nada de `dados/` no Git. Repositório GitHub público.
- Animações: molas sem ressalto (`bounce: 0`, `duration` 0.3–0.4); respeitar `prefers-reduced-motion`, `prefers-reduced-transparency`, `prefers-contrast`.
- Cor de destaque num único token CSS `--destaque` (claro `#0071E3`, escuro `#0A84FF`); logo como máscara (`#2B2B2B` claro / branco escuro).

## Review Focus

1. **Matrícula escrita de formas diferentes** (`aa12bb`, `AA 12 BB`, `aa-12-bb`) → é a mesma matrícula `AA-12-BB`; uma segunda viatura com a mesma matrícula é rejeitada com mensagem clara. Testes na Tarefa 4 e 7.
2. **Valores em euros com vírgula** (`1.234,56`, `45,5`, `45`, `45.5`) → cêntimos exatos (123456, 4550, 4500, 4550); texto inválido mostra erro no campo e não grava. Teste na Tarefa 11.
3. **Datas impossíveis num lançamento** (data no futuro numa manutenção/pneu/KM, ou anterior à aquisição do carro) → recusado com mensagem; o vencimento de um documento pode ser futuro. Testes na Tarefa 7.
4. **Pasta do Drive inacessível a meio da sessão** → nada se perde, aparece aviso, a gravação é retomada e o próximo arranque grava primeiro a cópia pendente. Testes nas Tarefas 2 e 8.
5. **Coparticipação incoerente** (parte da empresa ≥ valor da multa, ou 0) e **total da oficina que não bate com peças + mão de obra** → recusado com mensagem no campo; nunca se grava uma repartição que some mais do que a multa. Testes nas Tarefas 7 e 11.
6. **Apagar um carro ou motorista com histórico** → não é permitido; o ecrã propõe "Marcar como vendido/inativo". Apagar só é possível sem lançamentos associados. Teste na Tarefa 7.

---

## Estrutura de ficheiros

```
package.json, vite.config.ts, tsconfig.json, index.html, .gitignore
src/
  main.tsx, App.tsx                 navegação por estado (sem router)
  api/tipos.ts                      tipos espelho dos structs Rust (serde camelCase)
  api/comandos.ts                   wrappers tipados de invoke()
  util/formato.ts                   parseEuros, formatarEuros, formatarData, formatarKm
  estilo/tokens.css, base.css
  assets/logo.png
  ui/  Botao.tsx Campo.tsx Cartao.tsx Folha.tsx Lista.tsx Desfazer.tsx Grafico.tsx Barra.tsx Aviso.tsx
  ecras/ Inicio.tsx Carros.tsx FichaCarro.tsx Motoristas.tsx Relatorios.tsx Planos.tsx Backup.tsx
         BoasVindas.tsx Arranque.tsx Atualizando.tsx
  lancar/ Lancar.tsx FormManutencao.tsx FormPneu.tsx FormDocumento.tsx FormKm.tsx
src-tauri/
  Cargo.toml, tauri.conf.json, build.rs, icons/
  src/main.rs, lib.rs, erro.rs, comandos.rs, estado.rs
  src/migracoes/mod.rs, src/migracoes/001_inicial.sql
  src/armazenamento/mod.rs gravacao.rs versoes.rs lock.rs
  src/regras/mod.rs km.rs matricula.rs gastos.rs alertas.rs documentos.rs
  src/repositorio/mod.rs carros.rs motoristas.rs lancamentos.rs planos.rs config.rs consultas.rs
  src/atualizacao/mod.rs
.github/workflows/release.yml
docs/GUIA-INSTALACAO.md
```

---

### Task 0: Ferramentas e esqueleto Tauri

**Files:**
- Create: projeto Vite React-TS na raiz; `src-tauri/` via `tauri init`; `src-tauri/src/erro.rs`; `.gitignore` (acrescentar)
- Test: `src-tauri/src/lib.rs` (teste fumo), `src/util/formato.test.ts` (placeholder removido na Tarefa 11)

**Interfaces:**
- Produces: `pub struct Erro { pub codigo: String, pub mensagem: String }` (serde `Serialize`, `From<rusqlite::Error>`, `From<std::io::Error>`), `pub type Res<T> = Result<T, Erro>` em `erro.rs`; scripts npm `dev`, `build`, `test`, `tauri`.

- [ ] **Step 1: Instalar ferramentas**

Run: `winget install --id Rustlang.Rustup -e` e `winget install --id Microsoft.VisualStudio.2022.BuildTools -e --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"`; abrir novo terminal; `rustup default stable`.
Expected: `cargo --version` e `rustc --version` respondem.

- [ ] **Step 2: Esqueleto**

`npm create vite@latest . -- --template react-ts` (na raiz, sem apagar `docs/`, `Logo.png`, `.gitignore`), `npm i @tauri-apps/api motion`, `npm i -D @tauri-apps/cli vitest @testing-library/react @testing-library/user-event jsdom`, `npx tauri init` (app name `Frota`, identifier `pt.frota.app`, dev URL `http://localhost:5173`, dist `../dist`). Em `tauri.conf.json`: `"bundle": {"active": false}`, janela `title "Frota"`, `minWidth 1100`, `minHeight 700`, `transparent: true`, `windowEffects: {"effects": ["mica"]}`. Em `Cargo.toml` adicionar as crates do Tech Stack e `[profile.release] opt-level="s", lto=true, strip=true`. Copiar `Logo.png` para `src/assets/logo.png`.

- [ ] **Step 3: Verificar**

Run: `cargo test --manifest-path src-tauri/Cargo.toml` → PASS (0 testes ok); `npx vitest run` → PASS; `npm run tauri dev` → abre janela "Frota" com fundo Mica.

- [ ] **Step 4: Commit** — `git add -A; git commit -m "chore: esqueleto Tauri + React"`

---

### Task 1: Migrações e esquema

**Files:**
- Create: `src-tauri/src/migracoes/mod.rs`, `src-tauri/src/migracoes/001_inicial.sql`
- Test: `src-tauri/src/migracoes/mod.rs` (`#[cfg(test)]`)

**Interfaces:**
- Consumes: `Erro`, `Res`.
- Produces: `pub const MIGRACOES: &[&str]` (via `include_str!`), `pub fn migrar(conn: &mut Connection) -> Res<()>`, `pub fn migrar_com(conn: &mut Connection, migracoes: &[&str]) -> Res<()>`, `pub fn versao_esquema(conn: &Connection) -> Res<i64>`. Erro com `codigo = "versao_mais_recente"` quando `user_version > migracoes.len()`.

`001_inicial.sql`: tabelas e colunas exatamente como na spec §5 (incluindo `multas` e os campos de `executada_por`/peças/mão de obra em `manutencoes`; sem `abastecimentos`; CHECKs: `valor_empresa BETWEEN 0 AND valor`, `pontos BETWEEN 0 AND 12`, oficina não vazia quando `executada_por='oficina'`), `criado_em`/`atualizado_em` `TEXT DEFAULT (datetime('now'))`, `UNIQUE(matricula)`, `UNIQUE(tipos_manutencao.nome)`, FKs com `ON DELETE RESTRICT`, CHECKs nos enums (`estado`, `origem`, `posicao`, `motivo`, `tipo`) e `CHECK(cada_km IS NOT NULL OR cada_meses IS NOT NULL)`; índices em `(carro_id, data)` das tabelas de lançamentos; inserir os 11 tipos de manutenção da spec §4.8 e `config` (`limiar_km`=1000, `limiar_dias`=30). `PRAGMA foreign_keys=ON` é ligado em cada abertura (Tarefa 2), não na migração.

- [ ] **Step 1: Testes que falham**

```rust
#[test] fn banco_vazio_fica_na_versao_atual() { /* migrar(mem) ; versao_esquema == MIGRACOES.len() ; 11 tipos ; config limiar_km == "1000" */ }
#[test] fn migrar_duas_vezes_nao_altera_nada() { /* segunda chamada Ok, mesma versão, 11 tipos */ }
#[test] fn migracao_que_falha_faz_rollback() { /* migrar_com(&[MIGRACOES[0], "CREATE TABLE x(a); SELEC lixo;"]) é Err; versao == 1; tabela x não existe */ }
#[test] fn banco_mais_recente_e_recusado() { /* PRAGMA user_version=99 ; migrar → Err codigo "versao_mais_recente" */ }
```

- [ ] **Step 2:** `cargo test migracoes` → FAIL (não compila).
- [ ] **Step 3:** Implementar: cada migração pendente numa `transaction()`, `execute_batch` + `PRAGMA user_version = n` dentro da transação.
- [ ] **Step 4:** `cargo test migracoes` → PASS.
- [ ] **Step 5: Commit** — `feat: esquema e migrações`

---

### Task 2: Gravação atómica e cópia de trabalho

**Files:**
- Create: `src-tauri/src/armazenamento/mod.rs`, `gravacao.rs`
- Test: em `gravacao.rs`

**Interfaces:**
- Consumes: `migrar`.
- Produces (em `gravacao.rs`):
  - `pub struct Caminhos { pub dados: PathBuf /* <raiz>\dados */, pub banco: PathBuf /* dados\frota.db */, pub trabalho: PathBuf /* <local>\trabalho.db */, pub pendente: PathBuf /* <local>\pendente */ }` com `Caminhos::new(raiz: &Path, local: &Path)`.
  - `pub fn preparar_trabalho(c: &Caminhos) -> Res<Connection>`: se `pendente` existe → usa `trabalho` tal como está e chama `gravar` primeiro (falha de gravação não impede abrir); senão, se `banco` existe → copia para `trabalho`; senão cria vazio. Corre `PRAGMA integrity_check` (≠ "ok" → `Erro{codigo:"corrompido"}`), `PRAGMA foreign_keys=ON`, `journal_mode=DELETE`, `migrar`.
  - `pub fn gravar(conn: &Connection, c: &Caminhos) -> Res<()>`: cria `pendente`; `VACUUM INTO '<banco>.tmp'` (apagar `.tmp` antigo antes); `std::fs::rename(tmp, banco)` (MoveFileEx REPLACE_EXISTING); remove `pendente`. Em qualquer erro: `pendente` fica, `banco` intacto, devolve `Erro{codigo:"gravacao_falhou"}`.

- [ ] **Step 1: Testes que falham** (tempdir)

```rust
#[test] fn primeira_abertura_cria_banco_vazio_migrado()
#[test] fn gravar_substitui_banco_com_os_dados_novos() // inserir carro, gravar, abrir banco com rusqlite → 1 carro; pendente não existe
#[test] fn falha_ao_gravar_mantem_banco_antigo_e_marca_pendente() // gravar com 1 carro; inserir 2.º carro; gravar com um Caminhos cujo `banco` aponta para <tmp>\inexistente\frota.db → Err "gravacao_falhou"; pendente existe; frota.db original intacto com 1 carro
#[test] fn arranque_com_pendente_usa_trabalho_e_grava_primeiro() // trabalho tem 2 carros, banco tem 1, pendente existe → preparar_trabalho → banco tem 2, pendente removido
#[test] fn banco_corrompido_e_detetado() // escrever bytes lixo em frota.db → preparar_trabalho Err codigo "corrompido"
```

- [ ] **Step 2:** `cargo test gravacao` → FAIL.
- [ ] **Step 3:** Implementar conforme Interfaces.
- [ ] **Step 4:** `cargo test gravacao` → PASS.
- [ ] **Step 5: Commit** — `feat: cópia de trabalho e gravação atómica`

---

### Task 3: Versões e bloqueio

**Files:**
- Create: `src-tauri/src/armazenamento/versoes.rs`, `lock.rs`
- Test: nos próprios ficheiros

**Interfaces:**
- Consumes: `Caminhos`, `gravar`.
- Produces:
  - `pub struct Versao { pub nome: String, pub data: String, pub tamanho: u64, pub carros: i64, pub lancamentos: i64 }`
  - `pub fn copia_diaria(c: &Caminhos, hoje: NaiveDate) -> Res<bool>` — copia `banco` para `versoes\frota_AAAA-MM-DD.db` se ainda não existe; poda para 30 (`frota_2*`). Devolve se criou.
  - `pub fn copia_antes_de(c: &Caminhos, versao: &str) -> Res<()>` — `frota_antes-da-v{versao}.db`; poda para 5 (`frota_antes-da-*`, por data de modificação).
  - `pub fn copia_manual(c: &Caminhos, agora: NaiveDateTime) -> Res<()>` — `frota_AAAA-MM-DD_HHMM.db` (conta para as 30 diárias).
  - `pub fn listar(c: &Caminhos) -> Res<Vec<Versao>>` (mais recente primeiro; `carros`/`lancamentos` lidos abrindo a cópia só-leitura).
  - `pub fn restaurar(conn: &mut Connection, c: &Caminhos, nome: &str, agora: NaiveDateTime) -> Res<()>` — `copia_manual` do estado atual, depois `conn` passa a ter o conteúdo da versão (`backup` API do rusqlite da versão para a ligação), `integrity_check`, `migrar`, `gravar`.
  - `pub struct Lock`; `pub fn adquirir(caminho: &Path, pc: &str, agora: DateTime<Utc>) -> Result<Lock, Erro>` (`codigo:"bloqueado"`, `mensagem` = nome do outro PC) ; `Lock::renovar(&self, agora)`; `Lock::libertar(self)`. Formato do ficheiro: `pc\nRFC3339`. Mesmo PC → adquire sempre.

- [ ] **Step 1: Testes que falham**

```rust
#[test] fn copia_diaria_so_uma_por_dia()            // 2 chamadas no mesmo dia → true, false
#[test] fn guarda_so_30_diarias()                   // 32 dias seguidos → 30 ficheiros, os 2 mais antigos apagados
#[test] fn guarda_so_5_antes_da()                   // 6 versões → 5
#[test] fn restaurar_faz_copia_do_atual_e_repoe()   // estado A(1 carro) copia; estado B(2 carros); restaurar A → 1 carro; existe cópia manual com 2 carros
#[test] fn lock_de_outro_pc_recente_bloqueia()      // outro "PC-X" há 60 s → Err "bloqueado" mensagem "PC-X"
#[test] fn lock_abandonado_e_assumido()             // outro PC há 121 s → Ok
#[test] fn lock_do_mesmo_pc_e_assumido()
```

- [ ] **Step 2:** `cargo test versoes lock` → FAIL. **Step 3:** implementar. **Step 4:** PASS.
- [ ] **Step 5: Commit** — `feat: versões diárias e bloqueio entre PCs`

---

### Task 4: Regras — matrícula e KM

**Files:**
- Create: `src-tauri/src/regras/mod.rs`, `matricula.rs`, `km.rs`
- Test: nos próprios ficheiros

**Interfaces:**
- Produces:
  - `pub fn normalizar_matricula(s: &str) -> Res<String>` — remove espaços/hífens/pontos, maiúsculas; exige 6 caracteres em 3 pares, cada par só letras ou só dígitos, com pelo menos um par de letras e pelo menos um par de dígitos (cobre `AA-00-00`, `00-00-AA`, `00-AA-00`, `AA-00-AA`) → `XX-XX-XX`. Inválida → `Erro{codigo:"matricula_invalida", mensagem:"Matrícula inválida. Exemplo: AA-12-BB"}`.
  - `pub struct Leitura { pub data: NaiveDate, pub km: i64 }`
  - `pub fn km_atual(km_inicial: i64, leituras: &[Leitura]) -> i64` — máximo entre `km_inicial` e todas as leituras.
  - `pub fn km_abaixo_do_anterior(leituras: &[Leitura], data: NaiveDate, km: i64) -> Option<i64>` — maior km entre leituras com `data <= data`; devolve-o se `km <` esse valor.

- [ ] **Step 1: Testes que falham**

```rust
#[test] fn matriculas_equivalentes() { for s in ["aa12bb","AA 12 BB","aa-12-bb"," AA.12.BB "] { assert_eq!(normalizar_matricula(s).unwrap(), "AA-12-BB"); } }
#[test] fn formatos_antigos() { assert_eq!(normalizar_matricula("12-34-ab").unwrap(),"12-34-AB"); assert_eq!(normalizar_matricula("12ab34").unwrap(),"12-AB-34"); }
#[test] fn matricula_invalida() { for s in ["","ABC","AB-12-B","AB-CD-EF","12-34-56"] { assert_eq!(normalizar_matricula(s).unwrap_err().codigo,"matricula_invalida"); } }
#[test] fn km_atual_sem_leituras_e_o_inicial() { assert_eq!(km_atual(50_000,&[]),50_000); }
#[test] fn km_atual_e_o_maximo() // leituras 51_000, 53_000, 52_000 → 53_000
#[test] fn km_abaixo_compara_com_leitura_anterior_por_data() // leituras (01-03,60_000),(01-06,70_000); (01-04,65_000) → None; (01-07,69_000) → Some(70_000); (01-02, 1) → None
```

- [ ] **Step 2:** FAIL. **Step 3:** implementar. **Step 4:** `cargo test regras` → PASS.
- [ ] **Step 5: Commit** — `feat: regras de matrícula e KM`

---

### Task 5: Regras — gastos e motoristas

**Files:**
- Create: `src-tauri/src/regras/gastos.rs`
- Test: no próprio ficheiro

**Interfaces:**
- Produces:
  - `#[derive(Serialize, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)] pub enum Categoria { Manutencao, Pneus, Iuc, Inspecao, Seguro, Multa, Outro }`
  - `pub struct Despesa { pub carro_id: i64, pub categoria: Categoria, pub valor: i64, pub pago: bool, pub data_ref: Option<NaiveDate>, pub motorista_fixo: Option<Option<i64>> }` — resolvido pelo repositório (spec §6.2/§6.3): `data_ref` = `data` em manutenção/pneu, `pago_em` em documentos e multas; em multas `valor` = **`valor_empresa`** e `motorista_fixo = Some(multas.motorista_id)` (o condutor, não a atribuição); nos restantes `motorista_fixo = None` → usa `motorista_em(data_ref)`.
  - `pub struct Periodo { pub de: NaiveDate, pub ate: NaiveDate }` (inclusivo)
  - `pub struct Resumo { pub total: i64, pub por_categoria: BTreeMap<Categoria,i64>, pub por_mes: BTreeMap<String /*AAAA-MM*/, BTreeMap<Categoria,i64>>, pub por_carro: Vec<(i64,i64)> /* desc por valor */, pub por_pagar: i64 }`
  - `pub fn resumir(despesas: &[Despesa], p: &Periodo) -> Resumo` — conta só `pago && data_ref ∈ p`; `por_pagar` = soma de `!pago` (independente do período).
  - `pub struct Atribuicao { pub carro_id: i64, pub motorista_id: i64, pub de: NaiveDate, pub ate: Option<NaiveDate> }`
  - `pub fn motorista_em(atribs: &[Atribuicao], carro_id: i64, data: NaiveDate) -> Option<i64>` — atribuições com `de <= data && ate.map_or(true, |a| data <= a)`; se duas (troca no próprio dia), vence a de `de` mais recente.
  - `pub fn por_motorista(despesas: &[Despesa], atribs: &[Atribuicao], p: &Periodo) -> Vec<(Option<i64>, i64)>` — despesas pagas no período; motorista = `motorista_fixo` se `Some`, senão `motorista_em(carro, data_ref)`; desc por valor; `None` = "Sem motorista".

- [ ] **Step 1: Testes que falham**

```rust
#[test] fn soma_so_o_periodo_e_so_pagos()          // manut 10_000 em 2026-03-10, pneu 40_000 em 2026-12-01, IUC 15_000 por pagar → p=2026-01-01..2026-06-30: total 10_000; por_pagar 15_000
#[test] fn limites_do_periodo_sao_inclusivos()     // despesas em 'de' e em 'ate' contam
#[test] fn agrupa_por_mes_e_categoria()            // 2 manut em março, 1 seguro em abril → por_mes["2026-03"][Manutencao]==soma
#[test] fn ranking_de_carros_desc()
#[test] fn motorista_pela_data_da_despesa()        // A: 01-01..31-03, B: 01-04..None ; despesa 15-03 → A ; 10-04 → B
#[test] fn troca_no_proprio_dia_conta_o_novo()     // A até 01-04, B desde 01-04 ; despesa 01-04 → B
#[test] fn sem_atribuicao_e_sem_motorista()
#[test] fn multa_conta_para_o_condutor_e_so_parte_da_empresa() // multa valor_empresa 4_000 (de 12_000), motorista_fixo Some(A), pago_em 20-05 com B atribuído → por_motorista A=4_000; resumir total 4_000
```

- [ ] **Step 2:** FAIL. **Step 3:** implementar. **Step 4:** PASS.
- [ ] **Step 5: Commit** — `feat: regras de gastos e atribuição a motoristas`

---

### Task 6: Regras — alertas e documentos anuais

**Files:**
- Create: `src-tauri/src/regras/alertas.rs`, `documentos.rs`
- Test: nos próprios ficheiros

**Interfaces:**
- Consumes: `km_atual`, `Leitura`.
- Produces:
  - `#[derive(Serialize)] pub enum Nivel { Vencido, Proximo }` ; `#[derive(Serialize)] pub enum TipoAlerta { Manutencao, Documento, Carta }`
  - `#[derive(Serialize)] pub struct Alerta { pub nivel: Nivel, pub tipo: TipoAlerta, pub carro_id: Option<i64>, pub motorista_id: Option<i64>, pub ref_id: i64 /* plano, documento ou motorista */, pub titulo: String, pub falta_km: Option<i64>, pub falta_dias: Option<i64> }` — negativos quando vencido.
  - `pub struct CarroAlerta { pub id: i64, pub km_atual: i64, pub base_km: i64, pub base_data: NaiveDate /* data_aquisicao ou criado_em */ }`
  - `pub struct PlanoAlerta { pub id: i64, pub tipo_id: i64, pub tipo_nome: String, pub cada_km: Option<i64>, pub cada_meses: Option<u32>, pub carros: Option<Vec<i64>> /* None = todos */ }`
  - `pub struct UltimaManutencao { pub carro_id: i64, pub tipo_id: i64, pub data: NaiveDate, pub km: i64 }`
  - `pub struct Limiares { pub km: i64, pub dias: i64 }`
  - `pub fn alertas_manutencao(carros: &[CarroAlerta] /* só ativos */, planos: &[PlanoAlerta], ultimas: &[UltimaManutencao], lim: &Limiares, hoje: NaiveDate) -> Vec<Alerta>` — spec §6.4; `titulo` ex.: `"Mudança de óleo"`; meses com `checked_add_months` (31-01 + 1 mês = 28/29-02).
  - `pub struct DocAlerta { pub id: i64, pub carro_id: i64, pub tipo_nome: String, pub vencimento: NaiveDate }` (só por pagar) ; `pub fn alertas_documentos(docs: &[DocAlerta], hoje: NaiveDate) -> Vec<Alerta>` (≤ 30 dias → Proximo; passado → Vencido).
  - `pub struct CartaAlerta { pub motorista_id: i64, pub nome: String, pub validade: NaiveDate }` ; `pub fn alertas_cartas(c: &[CartaAlerta], hoje: NaiveDate) -> Vec<Alerta>` (≤ 60 dias).
  - `pub fn ordenar(alertas: &mut Vec<Alerta>)` — Vencido antes de Proximo; dentro do nível, menor `falta_dias` (None = +∞) e depois menor `falta_km`.
  - `TipoAlerta` ganha `IdentificarCondutor` e `Multa`. `pub fn feriados_pt(ano: i32) -> Vec<NaiveDate>` (1 jan, 25 abr, 1 mai, 10 jun, 15 ago, 5 out, 1 nov, 1 dez, 8 dez, 25 dez + Sexta-feira Santa, Páscoa, Corpo de Deus = Páscoa+60; Páscoa pelo algoritmo de Meeus/Butcher). `pub fn somar_dias_uteis(d: NaiveDate, n: u32) -> NaiveDate` (exclui sáb, dom, feriados). `pub fn dias_uteis_entre(a: NaiveDate, b: NaiveDate) -> i64`. `pub struct MultaAlerta { pub id: i64, pub carro_id: i64, pub data_notificacao: Option<NaiveDate>, pub condutor_identificado: bool, pub vencimento: Option<NaiveDate>, pub pago: bool }`; `pub fn alertas_multas(m: &[MultaAlerta], hoje: NaiveDate) -> Vec<Alerta>` — identificar condutor: prazo = `somar_dias_uteis(notificacao, 15)`, Proximo a ≤ 5 dias úteis, Vencido depois (`falta_dias` em dias de calendário, titulo `"Identificar condutor até 05 nov"`); pagamento: mesma regra dos documentos.
  - Em `documentos.rs`: `pub struct NovoDocumento { pub carro_id: i64, pub tipo: String, pub descricao: String, pub valor: i64, pub vencimento: NaiveDate, pub anual: bool, pub gerado_de_id: i64 }` ; `pub fn proximo_anual(id: i64, carro_id: i64, tipo: &str, descricao: &str, valor: i64, vencimento: NaiveDate) -> NovoDocumento` (vencimento + 12 meses; 29-02 → 28-02).

- [ ] **Step 1: Testes que falham**

```rust
#[test] fn plano_km_sem_historico_conta_do_km_inicial()   // base 50_000, cada 15_000, km_atual 64_500, lim 1000 → Proximo, falta_km 500
#[test] fn vencido_por_km()                               // km_atual 65_200 → Vencido, falta_km -200
#[test] fn vencido_por_data_mesmo_com_km_ok()             // cada_meses 12, base 2025-10-01, hoje 2026-10-20 → Vencido
#[test] fn ultima_manutencao_do_tipo_muda_a_base()        // manutenção em 64_000 → falta_km 14_000 → sem alerta
#[test] fn plano_so_para_carros_escolhidos()
#[test] fn sem_alerta_quando_longe()
#[test] fn fim_de_mes_soma_meses_sem_panico()             // base 2026-01-31 + 1 mês → 2026-02-28
#[test] fn documento_30_dias_proximo_e_passado_vencido()  // venc hoje+30 → Proximo ; hoje+31 → nada ; hoje-1 → Vencido
#[test] fn carta_60_dias()
#[test] fn ordenacao_vencidos_primeiro()
#[test] fn anual_gera_ano_seguinte()                      // IUC venc 2026-05-31 → 2027-05-31, gerado_de_id
#[test] fn anual_29_fevereiro()                           // 2028-02-29 → 2029-02-28
#[test] fn pascoa_e_feriados_moveis()                     // Páscoa 2026-04-05, Sexta-feira Santa 2026-04-03, Corpo de Deus 2026-06-04; Páscoa 2027-03-28
#[test] fn quinze_dias_uteis_saltam_fins_de_semana_e_feriados() // somar_dias_uteis(2026-11-27 (sex), 15) → 2026-12-21 (salta 1 e 8 dez)
#[test] fn identificar_condutor_proximo_e_vencido()      // notificação 2026-10-01; hoje 2026-10-20 → Proximo; condutor identificado → nada; hoje 2026-10-23 → Vencido
```

- [ ] **Step 2:** FAIL. **Step 3:** implementar. **Step 4:** PASS.
- [ ] **Step 5: Commit** — `feat: alertas e renovação de documentos anuais`

---

### Task 7: Repositório

**Files:**
- Create: `src-tauri/src/repositorio/mod.rs`, `carros.rs`, `motoristas.rs`, `lancamentos.rs`, `planos.rs`, `config.rs`, `consultas.rs`
- Test: `src-tauri/src/repositorio/testes.rs` (`#[cfg(test)] mod testes;` em `mod.rs`)

**Interfaces:**
- Consumes: regras (Tarefas 4–6), `migrar`.
- Produces — structs `serde(rename_all="camelCase")`, `Deserialize` para `*Input`, `Serialize` para saídas; todas as funções recebem `conn: &Connection` (ou `&mut` quando transação) e `hoje: NaiveDate` onde valida datas:
  - `carros.rs`: `Carro { id, matricula, marca, modelo, ano: Option<i32>, cor, km_inicial, km_atual, data_aquisicao: Option<String>, estado, data_saida: Option<String>, notas, motorista_atual: Option<(i64,String)>, gasto_ano: i64 }`; `CarroInput { id: Option<i64>, matricula, marca, modelo, ano, cor, km_inicial, data_aquisicao, estado, data_saida, notas }`; `listar_carros(conn, so_ativos: bool, ano: i32) -> Res<Vec<Carro>>`; `obter_carro(conn, id, ano) -> Res<Carro>`; `guardar_carro(conn, &CarroInput) -> Res<i64>` (normaliza matrícula; duplicada → `codigo:"matricula_duplicada"`, mensagem `"Já existe um carro com a matrícula AA-12-BB."`); `apagar_carro(conn, id) -> Res<()>` (com lançamentos/atribuições → `codigo:"tem_historico"`, mensagem `"Este carro tem histórico. Marque-o como vendido ou inativo."`).
  - `motoristas.rs`: `Motorista { id, nome, telefone, carta_numero, carta_validade: Option<String>, ativo, notas }`, `MotoristaInput`, `listar_motoristas(conn, so_ativos) `, `guardar_motorista`, `apagar_motorista` (mesma regra `tem_historico`); `atribuir(conn: &mut, carro_id, motorista_id: Option<i64>, data: NaiveDate) -> Res<()>` — fecha a atribuição aberta do carro com `ate = data` e, se `Some`, abre nova com `de = data`; um motorista só tem uma atribuição aberta (fecha a anterior noutro carro); `atribuicoes_de(conn, Filtro) -> Res<Vec<Atribuicao>>`.
  - `lancamentos.rs`: `ManutencaoInput { carro_id, data, km, tipo_id, executada_por, oficina: Option<String>, valor: i64, valor_pecas: Option<i64>, valor_mao_obra: Option<i64>, descricao, confirmado }` (validação: `oficina` obrigatória em `oficina` → `"falta_oficina"`; se peças/mão de obra preenchidas, `valor` tem de ser a soma → `"valor_nao_bate"`; em `propria`, mão de obra tem de ser 0/nula); `oficinas_usadas(conn) -> Res<Vec<String>>` (distintas, mais usadas primeiro); `PneuInput { carro_id, data, km, posicao, marca, modelo, motivo, valor }`, `DocumentoInput { carro_id, tipo, descricao, valor, vencimento, pago_em: Option, data_ocorrencia: Option, anual }`, `KmInput { carro_id, data, km, confirmado: bool }`; `lancar_manutencao(conn:&mut, &ManutencaoInput, hoje) -> Res<i64>`, `lancar_pneu`, `lancar_documento`, `atualizar_km` — cada um em transação; manutenção/pneu/KM criam `leituras_km` (origem e origem_id); validações: data > hoje → `"data_futura"`; data < data_aquisicao → `"data_antes_aquisicao"`; km abaixo e `!confirmado` → `Erro{codigo:"km_abaixo", mensagem:"O último KM registado era 70 000. Confirma 69 000?"}` (manutenção/pneu também levam `confirmado: bool`); valor < 0 → `"valor_invalido"`. `MultaInput { id: Option<i64>, carro_id, motorista_id: Option<i64>, data_infracao, data_notificacao: Option, condutor_identificado_em: Option, infracao, pontos, valor, vencimento: Option, pago_em: Option, responsavel, valor_empresa: Option<i64>, empresa_adiantou, reembolsado_em: Option, descricao }`; `guardar_multa(conn:&mut, &MultaInput, hoje) -> Res<i64>` — `valor_empresa` calculado para `empresa`/`colaborador`; em `coparticipado` obrigatório e `0 < valor_empresa < valor` → senão `"coparticipacao_invalida"` ("A parte da empresa tem de ser maior que 0 e menor que o valor da multa."); `data_infracao` futura → `"data_futura"`; `condutor_sugerido(conn, carro_id, data) -> Res<Option<i64>>` (via `motorista_em`); `marcar_reembolsado(conn, id, data)`; `identificar_condutor(conn, id, motorista_id, data)`. `ResumoMultasMotorista { n: i64, total: i64, empresa: i64, colaborador: i64, por_reembolsar: i64, pontos_3_anos: i64 }`; `multas_motorista(conn, motorista_id, hoje) -> Res<ResumoMultasMotorista>`. `pagar_documento(conn:&mut, id, pago_em) -> Res<Option<i64>>` (se `anual`, insere `proximo_anual` e devolve o id). `apagar_lancamento(conn:&mut, tipo: &str /* "manutencao"|"pneu"|"documento"|"km" */, id) -> Res<()>` (apaga também a leitura associada). `HistoricoItem { tipo, id, data, km: Option<i64>, titulo, valor: Option<i64>, detalhe }`; `historico(conn, carro_id: Option<i64>, limite: i64) -> Res<Vec<HistoricoItem>>` (desc por data).
  - `planos.rs`: `TipoManutencao { id, nome, ativo }`, `listar_tipos`, `guardar_tipo(conn, id: Option<i64>, nome) `; `Plano { id, tipo_id, tipo_nome, cada_km, cada_meses, todos_carros, carros: Vec<i64> }`, `PlanoInput`, `listar_planos`, `guardar_plano`, `apagar_plano`.
  - `config.rs`: `obter_limiares(conn) -> Res<Limiares>`, `guardar_limiares(conn, &Limiares)`.
  - `consultas.rs`: `despesas(conn, carro_ids: Option<&[i64]>) -> Res<Vec<Despesa>>`; `Painel { gasto_mes, gasto_ano, por_pagar, carros_ativos, alertas: Vec<Alerta>, recentes: Vec<HistoricoItem> }`, `painel(conn, hoje) -> Res<Painel>`; `FiltroRelatorio { de, ate, carro_ids: Option<Vec<i64>>, motorista_id: Option<i64> }`, `Relatorio { resumo: Resumo, por_motorista: Vec<(Option<i64>, String, i64)>, por_tipo_manutencao: Vec<(String,i64)>, manutencao: { interna: i64, oficina: i64, pecas: i64, mao_obra: i64, nao_discriminado: i64, por_oficina: Vec<(String,i64)> }, multas: { n: i64, total: i64, empresa: i64, colaboradores: i64, por_reembolsar: i64, por_infracao: Vec<(String,i64,i64) /* infração, n, total */>, por_motorista: Vec<(Option<i64>,String,i64 /*n*/,i64 /*pontos*/)> } }` (structs nomeados `RelManutencao`, `RelMultas`), `relatorio(conn, &FiltroRelatorio) -> Res<Relatorio>` (com `motorista_id`, só despesas cujo `motorista_em` é esse motorista); `alertas(conn, hoje) -> Res<Vec<Alerta>>` (monta os inputs da Tarefa 6 — manutenções, documentos, cartas e multas —, só carros ativos e motoristas ativos, chama `ordenar`).

- [ ] **Step 1: Testes que falham** (banco em memória migrado; `hoje = 2026-10-20`)

```rust
#[test] fn matricula_duplicada_em_formatos_diferentes() // guardar "aa12bb" e "AA-12-BB" → 2.º Err "matricula_duplicada"
#[test] fn nao_apaga_carro_com_historico()               // lançar manutenção; apagar_carro → Err "tem_historico"; sem histórico → Ok
#[test] fn manutencao_cria_leitura_e_atualiza_km()      // km 61_000 → obter_carro.km_atual == 61_000
#[test] fn km_abaixo_pede_confirmacao()                  // leitura 70_000; km 69_000 confirmado=false → Err "km_abaixo" mensagem contém "70 000"; confirmado=true → Ok
#[test] fn data_futura_recusada_mas_vencimento_futuro_ok()
#[test] fn data_antes_da_aquisicao_recusada()
#[test] fn coparticipacao_fora_dos_limites_recusada()   // valor 12_000: valor_empresa 0, 12_000 e None → Err "coparticipacao_invalida"; 4_000 → Ok
#[test] fn multa_colaborador_nao_conta_no_gasto_da_frota() // relatorio total exclui; multas.colaboradores inclui
#[test] fn adiantamento_fica_por_reembolsar_ate_marcar()  // empresa_adiantou, coparticipado 4_000/12_000 → por_reembolsar 8_000; marcar_reembolsado → 0
#[test] fn condutor_sugerido_pela_atribuicao()
#[test] fn pontos_so_dos_ultimos_3_anos()
#[test] fn oficina_obrigatoria_e_soma_pecas_mao_de_obra()  // oficina sem nome → "falta_oficina"; pecas 5_000 + mão 3_000 com valor 9_000 → "valor_nao_bate"
#[test] fn pagar_iuc_anual_cria_o_seguinte()
#[test] fn apagar_manutencao_remove_a_leitura()
#[test] fn atribuir_fecha_a_anterior()                   // A em carro1 desde 01-01; atribuir B 01-04 → A.ate == 01-04; B aberto
#[test] fn relatorio_ano_por_categoria_e_motorista()     // cenário pequeno: 2 carros, 2 motoristas, manut+pneu+IUC pago+multa → totais esperados exatos
#[test] fn painel_conta_alertas_de_carros_ativos_apenas() // carro vendido com plano vencido não aparece
```

- [ ] **Step 2:** FAIL. **Step 3:** implementar (SQL parametrizado; `km_atual` e `gasto_ano` via regras). **Step 4:** `cargo test repositorio` → PASS.
- [ ] **Step 5: Commit** — `feat: repositório e consultas`

---

### Task 8: Comandos Tauri, arranque, gravação automática e fecho

**Files:**
- Create: `src-tauri/src/estado.rs`, `src-tauri/src/comandos.rs`; Modify: `src-tauri/src/lib.rs`
- Create: `src/api/tipos.ts`, `src/api/comandos.ts`
- Test: `src-tauri/src/estado.rs`

**Interfaces:**
- Consumes: tudo das Tarefas 1–7.
- Produces:
  - `estado.rs`: `pub enum Arranque { Ok, Bloqueado { pc: String }, Corrompido, VersaoMaisRecente, SemPasta { mensagem: String } }` (serde tag `"estado"`); `pub struct Estado { conn: Mutex<Option<Connection>>, caminhos: Caminhos, lock: Mutex<Option<Lock>>, so_consulta: AtomicBool, ultima_gravacao: Mutex<Option<NaiveDateTime>>, erro_gravacao: Mutex<Option<String>>, sujo: AtomicBool }`; `Estado::arrancar(raiz, local, pc, agora) -> (Estado, Arranque)`; `Estado::abrir_consulta(&self)` (abre `banco` read-only, `so_consulta=true`); `Estado::alterado(&self)` (marca `sujo`); `Estado::tick(&self, agora)` (chamado por uma thread a cada 1 s: grava se `sujo` e passaram ≥ 2 s desde a última alteração; se `erro_gravacao`, tenta de novo a cada 60 s; renova o lock a cada 30 s); `Estado::fechar(&self, hoje)` (grava, `copia_diaria`, liberta lock); `pub fn mutar<T>(&self, f: impl FnOnce(&mut Connection) -> Res<T>) -> Res<T>` (recusa com `codigo:"so_consulta"` em modo consulta; chama `alterado`).
  - `comandos.rs` (`#[tauri::command]`, todos devolvem `Result<T, Erro>`): `arranque`, `abrir_consulta`, `estado_armazenamento -> EstadoArmazenamento { pasta, no_google_drive: bool, ultima_gravacao, erro: Option<String>, so_consulta, versao_app }`, `fazer_copia`, `listar_versoes`, `restaurar_versao(nome)`, `carros_listar(soAtivos, ano)`, `carro_obter(id, ano)`, `carro_guardar(input)`, `carro_apagar(id)`, `motoristas_listar(soAtivos)`, `motorista_guardar`, `motorista_apagar`, `atribuir(carroId, motoristaId, data)`, `lancar_manutencao`, `lancar_pneu`, `lancar_documento`, `guardar_multa`, `condutor_sugerido(carroId, data)`, `marcar_reembolsado(id, data)`, `identificar_condutor(id, motoristaId, data)`, `multas_motorista(motoristaId)`, `oficinas_usadas`, `atualizar_km`, `pagar_documento(id, pagoEm)`, `apagar_lancamento(tipo, id)`, `historico(carroId, limite)`, `tipos_listar`, `tipo_guardar`, `planos_listar`, `plano_guardar`, `plano_apagar`, `limiares_obter`, `limiares_guardar`, `painel`, `relatorio(filtro)`. `no_google_drive` = caminho contém `\Meu Disco\`, `\O meu disco\`, `\My Drive\` ou `\Google Drive\` (sem distinção de maiúsculas).
  - `lib.rs`: raiz = `FROTA_RAIZ` ou pasta do exe; local = `%LOCALAPPDATA%\Frota`; pc = `COMPUTERNAME`; `Estado::arrancar` no `setup`; thread de `tick`; no evento `CloseRequested` → `fechar` (síncrono) antes de sair.
  - `src/api/tipos.ts`: interfaces TS com os mesmos nomes e campos (camelCase) dos structs serializados; `src/api/comandos.ts`: uma função tipada por comando (`carrosListar(soAtivos: boolean, ano: number): Promise<Carro[]>`, …) e `type ErroApi = { codigo: string; mensagem: string }`.

- [ ] **Step 1: Testes que falham**

```rust
#[test] fn arranque_normal_e_gravacao_apos_alteracao() // arrancar → Ok; mutar(inserir carro); tick(t+1s) não grava; tick(t+3s) grava → banco tem 1 carro
#[test] fn arranque_bloqueado_por_outro_pc()
#[test] fn modo_consulta_recusa_mutacoes()             // so_consulta → mutar Err "so_consulta"
#[test] fn gravacao_falhada_fica_registada_e_recupera() // tornar pasta dados inacessível (renomear) → tick → erro_gravacao Some; repor pasta; tick(t+61s) → None e banco atualizado
#[test] fn fechar_grava_copia_diaria_e_liberta_lock()
```

- [ ] **Step 2:** FAIL. **Step 3:** implementar. **Step 4:** `cargo test` (tudo) → PASS; `npx tsc --noEmit` → sem erros.
- [ ] **Step 5: Commit** — `feat: comandos, arranque e gravação automática`

---

### Task 9: Base visual

**Files:**
- Create: `src/estilo/tokens.css`, `src/estilo/base.css`, `src/ui/*.tsx` (Botao, Campo, Cartao, Folha, Lista, Desfazer, Barra, Aviso), `src/App.tsx`, `src/util/formato.ts`
- Test: `src/util/formato.test.ts`, `src/ui/Desfazer.test.tsx`

**Interfaces:**
- Produces:
  - `formato.ts`: `parseEuros(texto: string): number | null` (cêntimos; aceita `1.234,56`, `1234,56`, `45,5`, `45`, `45.5`, `45 €`; rejeita vazio, negativo, letras, mais de 2 decimais), `formatarEuros(c: number): string` (`"1 234,56 €"`, espaço fino ` ` como separador de milhares), `formatarKm(n: number): string` (`"70 000 km"`), `formatarData(iso: string): string` (`"20 out 2026"`).
  - `tokens.css`: `--destaque`, `--vermelho #FF3B30/#FF453A`, `--laranja #FF9500/#FF9F0A`, `--texto`, `--texto-2`, `--fundo`, `--superficie`, `--lateral` (translúcido sobre Mica), raios 10/14/20, sombras por tamanho, escalas tipográficas (`--t-display: 2.5rem/1.05 -0.02em; --t-titulo 1.5rem/1.15 -0.015em; --t-corpo 0.9375rem/1.45 0; --t-legenda 0.8125rem/1.35 0.01em`), `font-family: "Segoe UI Variable Display", "Segoe UI Variable Text", system-ui`; `font-variant-numeric: tabular-nums` nos valores; temas por `prefers-color-scheme`; blocos `prefers-reduced-transparency` (superfícies sólidas) e `prefers-contrast: more` (bordas 1px `--texto-2`).
  - `App.tsx`: `type Ecra = {nome:"inicio"} | {nome:"carros"} | {nome:"carro", id:number} | {nome:"motoristas"} | {nome:"relatorios"} | {nome:"planos"} | {nome:"backup"}`; barra lateral (`Barra.tsx`) com logo (`mask-image: url(logo.png)` pintado com `--texto`), itens Início · Carros · Motoristas · Relatórios · Planos de manutenção · Backup, botão "+ Lançar"; atalhos Ctrl+N, Ctrl+K (foco na pesquisa de matrícula), Esc.
  - `Folha.tsx` (painel lateral do Lançar): `props { aberta: boolean; origem: DOMRect | null; onFechar(): void; children }` — anima de/para o retângulo de `origem` com `motion` (`type:"spring", bounce:0, duration:0.35`), materializa com blur+escala; com `prefers-reduced-motion` usa fade de 150 ms; nunca bloqueia cliques durante a animação.
  - `Desfazer.tsx`: `useDesfazer(): { agendar(rotulo: string, acao: () => Promise<void>): void }` + `<Desfazer/>` (toast "Apagado · Desfazer", 8 s; executa `acao` ao expirar; `window` `beforeunload` e evento de fecho executam pendentes de imediato).
  - `Botao.tsx`: estado pressed em `pointerdown` (`scale(0.97)`, 100 ms).
  - `Aviso.tsx`: banner de erro de gravação/so-consulta a partir de `estadoArmazenamento()` (consulta a cada 5 s).

- [ ] **Step 1: Testes que falham**

```ts
test("parseEuros", () => { expect(parseEuros("1.234,56")).toBe(123456); expect(parseEuros("45,5")).toBe(4550); expect(parseEuros("45")).toBe(4500); expect(parseEuros("45.5")).toBe(4550); expect(parseEuros("45 €")).toBe(4500); for (const s of ["", "-3", "abc", "1,234"]) expect(parseEuros(s)).toBeNull(); });
test("formatarEuros", () => { expect(formatarEuros(123456)).toBe("1 234,56 €"); expect(formatarEuros(0)).toBe("0,00 €"); });
test("desfazer cancela a ação", ...)      // fake timers: agendar, clicar "Desfazer", avançar 9 s → ação não chamada
test("sem desfazer a ação corre aos 8 s", ...)
```

- [ ] **Step 2:** `npx vitest run` → FAIL. **Step 3:** implementar. **Step 4:** PASS; `npm run tauri dev` → barra lateral translúcida com logo, navegação entre ecrãs vazios, tema escuro troca o logo para branco.
- [ ] **Step 5: Commit** — `feat: base visual e componentes`

---

### Task 10: Carros, ficha do carro, motoristas, boas-vindas

**Files:**
- Create: `src/ecras/Carros.tsx`, `FichaCarro.tsx`, `Motoristas.tsx`, `BoasVindas.tsx`
- Test: `src/ecras/Carros.test.tsx` (com `comandos` mockados via `vi.mock("../api/comandos")`)

**Interfaces:**
- Consumes: `comandos.ts`, componentes da Tarefa 9.
- Produces: ecrãs conforme spec §4.2, §4.4, §4.8.
  - Carros: lista (matrícula, marca/modelo, KM, motorista, gasto no ano), pesquisa (filtra por matrícula normalizada sem hífens e por modelo), alternar "Mostrar inativos/vendidos"; botão "Novo carro" abre formulário na `Folha`.
  - Ficha: cabeçalho com matrícula grande, métricas (gasto total, ano, por categoria), separadores Histórico · Manutenções · Pneus · Documentos (com botão "Marcar como pago") · Motoristas (atribuir/trocar) · KM; editar dados; "Apagar" → se `tem_historico`, mostra a mensagem e botão "Marcar como vendido".
  - Motoristas: lista, ficha com carros atribuídos e gasto no ano (via `relatorio` com `motoristaId`), validade da carta com alerta visual, bloco Multas (`multasMotorista`: n.º, total, pago pela empresa, pago por ele, **por reembolsar** com botão "Marcar reembolsado" por multa, pontos nos últimos 3 anos).
  - Ficha do carro: separador Multas (lista com estado: por identificar condutor / por pagar / por reembolsar / fechada).
  - BoasVindas: aparece quando `carrosListar(false, ano)` está vazio; texto "Vamos registar o primeiro carro" + formulário.
  - Erros de validação do núcleo (`ErroApi.mensagem`) aparecem junto ao campo respetivo (mapa `codigo → campo`: `matricula_*`→matrícula, `data_*`→data, `km_abaixo`→km com botão "Confirmar", `valor_invalido`→valor).

- [ ] **Step 1: Testes que falham**

```ts
test("pesquisa encontra 'aa12' em AA-12-BB")
test("matrícula duplicada mostra erro no campo e não fecha a folha")
test("apagar carro com histórico propõe marcar como vendido")
test("lista vazia mostra boas-vindas")
```

- [ ] **Step 2:** FAIL. **Step 3:** implementar. **Step 4:** `npx vitest run` → PASS; manual em `tauri dev`: criar 2 carros, 1 motorista, atribuir.
- [ ] **Step 5: Commit** — `feat: ecrãs de carros e motoristas`

---

### Task 11: Lançar

**Files:**
- Create: `src/lancar/Lancar.tsx`, `FormManutencao.tsx`, `FormPneu.tsx`, `FormDocumento.tsx`, `FormMulta.tsx`, `FormKm.tsx`
- Test: `src/lancar/Lancar.test.tsx`

**Interfaces:**
- Consumes: `Folha`, `parseEuros`, `comandos`.
- Produces: `<Lancar aberta origem carroId? onFechar onLancado/>`. Passo 1: escolher tipo (Manutenção · Pneus · Documento/taxa · Multa · Atualizar KM) e carro (pesquisa por matrícula; pré-selecionado se aberto a partir da ficha). Passo 2: formulário do tipo com campos da spec §5; data por omissão = hoje; KM sugerido = KM atual.
  - Manutenção: "Feita por" segmentado **Nós / Oficina** (omissão: Nós). Nós → campo "Peças e materiais (€)". Oficina → nome com sugestões (`oficinasUsadas`), "Peças (€)" + "Mão de obra (€)" com total calculado à vista, e interruptor "Só tenho o total" que troca para um único campo.
  - Documento: tipo IUC/Inspeção/Seguro/Outro, "Anual" ligado por omissão para IUC e Seguro.
  - Multa: data da infração (obrigatória) → condutor pré-preenchido com `condutorSugerido` (editável, ou "Por identificar"); infração (lista), pontos (0–12); valor; prazo de pagamento; data de notificação e "Condutor já identificado em"; **Quem paga** segmentado Empresa / Colaborador / Coparticipado — em Coparticipado, campo "Parte da empresa (€)" com linha ao vivo `"Empresa 40,00 € (33%) · Colaborador 80,00 €"`; interruptor "A empresa adiantou o pagamento" (visível quando a parte do colaborador > 0). Validação ao escrever (valor via `parseEuros`, KM inteiro ≥ 0); `km_abaixo` mostra a mensagem do núcleo e um botão "Confirmar" que reenvia com `confirmado: true`. Após sucesso: fecha, toast "Lançado ✓", chama `onLancado`.

- [ ] **Step 1: Testes que falham**

```ts
test("valor '1.234,56' envia 123456 cêntimos")
test("valor inválido mostra erro e não envia")
test("multa exige data da infração")
test("multa pré-preenche o condutor sugerido para a data da infração")
test("coparticipado 120 € com empresa 40 € mostra 33% e colaborador 80,00 €")
test("manutenção na oficina sem nome mostra erro; 'Nós' esconde mão de obra")
test("peças 50 + mão de obra 30 envia valor 8000")
test("km abaixo pede confirmação e reenvia com confirmado=true")
test("IUC vem marcado como anual")
```

- [ ] **Step 2:** FAIL. **Step 3:** implementar. **Step 4:** PASS; manual: Ctrl+N abre a folha a partir do botão "+", Esc fecha para o botão.
- [ ] **Step 5: Commit** — `feat: lançamentos`

---

### Task 12: Início e Relatórios

**Files:**
- Create: `src/ecras/Inicio.tsx`, `src/ecras/Relatorios.tsx`, `src/ui/Grafico.tsx`
- Test: `src/ui/Grafico.test.tsx`, `src/ecras/Relatorios.test.tsx`

**Interfaces:**
- Consumes: `painel()`, `relatorio(filtro)`, `Alerta`, `Resumo`.
- Produces:
  - Início: 4 cartões (Gasto este mês, Gasto no ano, Por pagar, Carros ativos), lista "Próximos vencimentos" (vermelho = Vencido, laranja = Proximo; texto: manutenção `"AA-12-BB · Mudança de óleo · faltam 420 km"` / `"passou 200 km"` / `"faltam 5 dias"`; documento `"AA-12-BB · IUC · vence a 31 mai"`; carta `"João Silva · carta caduca a 12 dez"`), clicar abre a ficha (carro ou motorista); "Últimos lançamentos" (10).
  - Relatórios: seletor de período (Este mês · Mês anterior · Este ano · Ano anterior · Personalizado), filtros carro(s) e motorista; total grande; barras por categoria; `Grafico` de barras empilhadas mensais; ranking de carros; gasto por motorista; tipos de manutenção mais caros; bloco Manutenção (interna vs oficina, peças vs mão de obra vs não discriminado, por oficina); bloco Multas (total, empresa, colaboradores, por reembolsar, por infração, por motorista com pontos). Início mostra alertas "Identificar condutor até …" e multas por pagar.
  - `Grafico.tsx`: `props { meses: { mes: string; valores: Partial<Record<Categoria, number>> }[]; cores: Record<Categoria,string> }` — SVG próprio, eixo Y com 4 marcas arredondadas (1, 2, 5 × 10ⁿ), legenda, tooltip ao passar; meses sem gasto aparecem com barra vazia.

- [ ] **Step 1: Testes que falham**

```ts
test("gráfico desenha um grupo por mês incluindo meses a zero")
test("escala arredonda o máximo 1 234,56 € para 1 500 €")
test("'Ano anterior' em 2026-10-20 pede 2025-01-01..2025-12-31")
test("'Mês anterior' em 2026-01-15 pede 2025-12-01..2025-12-31")
```

- [ ] **Step 2:** FAIL. **Step 3:** implementar. **Step 4:** PASS; manual com dados de teste.
- [ ] **Step 5: Commit** — `feat: painel e relatórios`

---

### Task 13: Planos, Backup e ecrãs de arranque

**Files:**
- Create: `src/ecras/Planos.tsx`, `src/ecras/Backup.tsx`, `src/ecras/Arranque.tsx`
- Test: `src/ecras/Arranque.test.tsx`

**Interfaces:**
- Consumes: `planos*`, `tipos*`, `limiares*`, `estadoArmazenamento`, `fazerCopia`, `listarVersoes`, `restaurarVersao`, `arranque`, `abrirConsulta`.
- Produces:
  - Planos: lista de regras ("Mudança de óleo · a cada 15 000 km ou 12 meses · todos os carros"), criar/editar (tipo, km e/ou meses — pelo menos um, aplica a todos ou escolher carros), gerir tipos de manutenção, limiares. Sem planos: sugestão "Mudança de óleo a cada 15 000 km ou 12 meses" com botão "Criar".
  - Backup: "Última cópia: hoje, 18:42 ✓" (ou aviso com o erro), pasta atual, aviso se `!noGoogleDrive` ("Esta pasta não está no Google Drive — os dados não têm cópia na nuvem."), "Fazer cópia agora", lista de versões (data, n.º carros/lançamentos) com "Restaurar…" → diálogo de confirmação ("O estado atual é guardado antes de restaurar.") → `restaurarVersao`; versão do programa.
  - Arranque (antes do `App` normal), por `arranque()`: `Bloqueado` → "O sistema está aberto no PC «X». Feche-o lá primeiro." + "Tentar de novo" + "Abrir só para consulta"; `Corrompido` → lista de versões com "Restaurar esta"; `VersaoMaisRecente` → "Estes dados foram criados por uma versão mais recente do programa." + "Abrir só para consulta"; `SemPasta` → mensagem.

- [ ] **Step 1: Testes que falham**

```ts
test("bloqueado mostra o nome do PC e permite abrir só para consulta")
test("corrompido oferece restaurar a versão mais recente")
test("restaurar pede confirmação antes de chamar restaurarVersao")
```

- [ ] **Step 2:** FAIL. **Step 3:** implementar. **Step 4:** PASS.
- [ ] **Step 5: Commit** — `feat: planos, backup e arranque`

---

### Task 14: Atualização automática e publicação

**Files:**
- Create: `src-tauri/src/atualizacao/mod.rs`, `src/ecras/Atualizando.tsx`, `.github/workflows/release.yml`, `docs/GUIA-INSTALACAO.md`, `src-tauri/icons/*` (gerados de `Logo.png`)
- Modify: `src-tauri/src/comandos.rs`, `src-tauri/src/lib.rs`, `src/App.tsx`
- Test: em `atualizacao/mod.rs`

**Interfaces:**
- Consumes: `Estado`, `gravar`, `copia_antes_de`.
- Produces:
  - `pub const URL_LATEST: &str = "https://github.com/<DONO>/<REPO>/releases/latest/download/latest.json";` e `pub const CHAVE_PUBLICA: &str` (minisign) — valores preenchidos neste passo (perguntar ao utilizador o utilizador/nome do repositório GitHub).
  - `#[derive(Deserialize)] pub struct Latest { pub versao: String, pub notas: String, pub url: String, pub sha256: String, pub assinatura: String }`
  - `pub fn nova_versao(atual: &str, json: &str) -> Option<Latest>` (semver estritamente maior; JSON inválido → None).
  - `pub fn validar(bytes: &[u8], l: &Latest, chave: &str) -> Res<()>` (SHA-256 em hex minúsculas e assinatura minisign; falha → `codigo:"atualizacao_invalida"`).
  - `pub fn trocar_exe(exe: &Path, novo: &Path) -> Res<()>` — `exe` → `Frota.old.exe` (apaga `.old` existente antes), `novo` → `exe`; se o segundo rename falhar, desfaz o primeiro.
  - `pub fn limpar_old(exe: &Path)`.
  - Comandos: `verificar_atualizacao() -> Option<{versao, notas}>` (ureq, timeout 5 s, erros → None), `aplicar_atualizacao()` (descarrega para `Frota.new.exe` → `validar` → `gravar` → `copia_antes_de(versao)` → `trocar_exe` → `app.restart()`; qualquer falha apaga `Frota.new.exe` e devolve erro, a app continua). Não corre em modo consulta nem em desenvolvimento (`cfg!(debug_assertions)`).
  - `Atualizando.tsx`: "A atualizar para a versão X.Y.Z…" com indicador; em erro, mensagem e "Continuar com a versão atual".
  - `release.yml`: em push de tag `v*`: `windows-latest`, Node 24, Rust stable, `npm ci`, `npm run tauri build -- --no-bundle`, renomear `target/release/frota.exe` → `Frota.exe`, `npx tauri signer sign` com `secrets.TAURI_SIGNING_PRIVATE_KEY`/`..._PASSWORD`, calcular SHA-256, gerar `latest.json`, criar Release com `Frota.exe` + `latest.json`. Versão da app = tag (o workflow escreve-a em `tauri.conf.json` e `Cargo.toml` antes de compilar).
  - Ícones: `npx tauri icon Logo.png` (logo sobre fundo branco arredondado para ficar visível no ambiente de trabalho escuro).
  - `GUIA-INSTALACAO.md`: Google Drive para computador → criar pasta Frota → "Disponível offline" → copiar `Frota.exe` → duplo clique (aviso SmartScreen "Mais informações → Executar mesmo assim" na primeira vez).

- [ ] **Step 1: Testes que falham**

```rust
#[test] fn deteta_versao_maior() { assert!(nova_versao("1.2.0", json("1.3.0")).is_some()); assert!(nova_versao("1.3.0", json("1.3.0")).is_none()); assert!(nova_versao("1.3.0", json("1.2.9")).is_none()); assert!(nova_versao("1.0.0", "lixo").is_none()); }
#[test] fn sha_errado_e_recusado()
#[test] fn assinatura_de_outra_chave_e_recusada()   // par de chaves de teste gerado e guardado em src-tauri/tests/fixtures
#[test] fn assinatura_valida_e_aceite()
#[test] fn troca_exe_mantem_o_antigo_como_old()     // tempdir: Frota.exe "v1", Frota.new.exe "v2" → Frota.exe "v2", Frota.old.exe "v1"
```

- [ ] **Step 2:** FAIL. **Step 3:** implementar; gerar chaves com `npx tauri signer generate -w %USERPROFILE%\.tauri\frota.key` (privada **fora** do repositório); criar repositório público no GitHub e configurar os secrets. **Step 4:** `cargo test atualizacao` → PASS.
- [ ] **Step 5: Commit** — `feat: atualização automática e publicação`

---

### Task 15: Verificação de release

**Files:** nenhum novo (corrigir o que falhar na tarefa que o possui).

- [ ] **Step 1:** `cargo test` + `npx vitest run` + `npx tsc --noEmit` → tudo PASS.
- [ ] **Step 2:** `git tag v0.1.0; git push --tags` → workflow verde; Release com `Frota.exe` (< 20 MB) e `latest.json`.
- [ ] **Step 3:** Numa pasta de teste dentro do Google Drive: duplo clique → boas-vindas; criar 3 carros, 2 motoristas, plano de óleo, lançamentos com um alerta vencido; fechar → `dados\frota.db` e `dados\versoes\frota_<hoje>.db` existem; reabrir → dados presentes.
- [ ] **Step 4:** Abrir em dois PCs (ou simular lock) → mensagem de bloqueio.
- [ ] **Step 5:** Publicar `v0.1.1` → abrir a `v0.1.0` → atualiza, reabre na `v0.1.1`, dados intactos, existe `frota_antes-da-v0.1.1.db`, `Frota.old.exe` apagado.
- [ ] **Step 6:** Ligar "Reduzir animações" e tema escuro no Windows → transições em fade, logo branco.
- [ ] **Step 7: Commit** de eventuais correções — `fix: ajustes da verificação de release`
