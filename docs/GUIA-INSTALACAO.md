# Guia de instalação — Frota

O programa é um único ficheiro, `Frota.exe`, que fica numa pasta do Google Drive juntamente
com os dados. Não há instalador: basta copiar o ficheiro para a pasta certa.

## O que é preciso

- Windows 10 ou 11.
- Uma conta Google com o **Google Drive para computador** instalado
  (<https://www.google.com/drive/download/>).

## 1. Instalar o Google Drive para computador

1. Descarregue e instale o **Google Drive para computador**.
2. Inicie sessão com a conta Google da empresa.
3. No Explorador de Ficheiros passa a aparecer uma unidade **Google Drive** (normalmente `G:`)
   com a pasta **O meu disco**.

## 2. Criar a pasta Frota

1. Abra `G:\O meu disco`.
2. Crie uma pasta chamada **Frota**.
3. Clique com o botão direito na pasta **Frota** → **Acesso offline** → **Disponível offline**.
   Assim o programa e os dados ficam sempre também no computador, mesmo sem internet.

## 3. Copiar o programa

1. Descarregue o `Frota.exe` da página de versões:
   <https://github.com/gnascimento94/gestao-frota/releases/latest>
2. Copie o `Frota.exe` para dentro da pasta `G:\O meu disco\Frota`.

## 4. Abrir

1. Faça duplo clique em `Frota.exe`.
2. **Na primeira vez**, o Windows pode mostrar o aviso azul "O Windows protegeu o seu PC"
   (SmartScreen). Clique em **Mais informações** → **Executar mesmo assim**.
   O aviso aparece porque o programa é novo e não por haver algum problema; não volta a aparecer.
3. Na primeira abertura o programa cria a pasta `dados` dentro da pasta Frota. É lá que ficam
   todos os registos e as cópias de segurança (`dados\versoes`). **Não apague nem mexa nesta pasta.**

Dica: para abrir mais depressa, clique com o botão direito em `Frota.exe` →
**Mostrar mais opções** → **Enviar para** → **Ambiente de trabalho (criar atalho)**.

## Usar em mais do que um computador

Repita os passos 1 e 2 noutro computador com a **mesma conta Google**: a pasta Frota, o
programa e os dados aparecem lá automaticamente. Não é preciso copiar o `Frota.exe` outra vez.

Só um computador de cada vez pode fazer alterações. Se o programa estiver aberto noutro PC,
aparece "O sistema está aberto no PC «…». Feche-o lá primeiro." — pode, entretanto, abrir só
para consulta.

## Atualizações

O programa atualiza-se sozinho. Ao abrir, se houver uma versão nova, aparece
"A atualizar para a versão X.Y.Z…"; os dados são gravados, é feita uma cópia de segurança
(`frota_antes-da-vX.Y.Z.db`) e o programa volta a abrir já atualizado. Sem internet, abre
normalmente com a versão atual.

As atualizações só trocam o `Frota.exe`; os dados na pasta `dados` nunca são alterados por elas.
