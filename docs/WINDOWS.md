# HD Fantasma no Windows

Versão de teste (beta). Mesmas funções do Mac. Requer Windows 10 ou 11 (64 bits).

## Para quem vai testar

### Instalar

1. Na página de [Releases](https://github.com/otavioclima-hub/hd-fantasma/releases), abra a versão marcada **Pre-release** com "Windows" no nome e baixe o arquivo **`HD-Fantasma-…-windows-x64-setup.exe`**.
2. Dê dois cliques nele. Como o app não é assinado pela Microsoft (a assinatura é paga), o Windows mostra **"O Windows protegeu o computador"**. Clique em **Mais informações** e depois em **Executar assim mesmo**. Isso só acontece na instalação.
3. Siga o instalador (não pede senha de administrador). No fim, o HD Fantasma aparece no menu Iniciar.

Se o Windows for antigo e não tiver o componente WebView2 (o "motor" que desenha a janela), o instalador baixa ele da Microsoft. O app em si nunca usa a internet.

### O que testar (marque o que funcionou)

Anote o que deu errado com uma foto da tela (tecla Windows + Shift + S).

1. [ ] O app abre e mostra a janela em português (ou inglês, se o Windows estiver em inglês).
2. [ ] **Ler disco** com um HD ou pen drive externo plugado: ele aparece na lista com o nome e a letra (ex.: `E:\`). A barra de progresso anda até 100%.
3. [ ] Depois de ler, **desplugue o disco**: ele continua na lista, marcado como ausente, e dá para abrir as pastas dele.
4. [ ] **Busca**: digite parte do nome de um arquivo. Teste também os filtros (tipo, tamanho, data).
5. [ ] Com o disco plugado, escolha um arquivo e clique em **Mostrar no Explorer**: o Explorer abre com o arquivo selecionado.
6. [ ] **Copiar caminho** no painel de informações: o caminho colado fica no formato `E:\pasta\arquivo.mp4`.
7. [ ] **Ler de novo** um disco depois de criar ou apagar algum arquivo nele: a lista se atualiza.
8. [ ] **Duplicados**: leia dois discos com algum arquivo igual e veja se ele aparece.
9. [ ] **Exportar** um disco (arquivo `.hdfantasma`), **remover** o disco do catálogo e **importar** de volta.
10. [ ] **Disco do sistema (`C:`)**: a leitura termina sem erro e as pastas `Windows` e `Program Files` não aparecem.
11. [ ] **OneDrive** (se usar): arquivos marcados como "disponível somente online" (ícone de nuvem) não aparecem; os baixados aparecem. Nada é baixado durante a leitura.
12. [ ] **Unidade de rede** (se tiver NAS mapeado numa letra): aparece com ícone de rede e dá para ler.
13. [ ] **Conectar ao Claude** (se tiver o Claude Desktop ou o Claude Code): o botão conecta; depois de fechar e abrir o Claude, pergunte "quais discos tenho no HD Fantasma?".
14. [ ] **Desinstalar** pelo Windows (Configurações → Aplicativos): sai sem erro.

Discos formatados no Mac (APFS ou Mac OS Expandido) **não aparecem** no Windows: é normal (o próprio Windows não lê esse formato sem programa extra, como o Paragon). exFAT, NTFS e FAT32 funcionam.

### Onde fica o catálogo

`%LOCALAPPDATA%\HD Fantasma\catalogo.sqlite` (cole isso na barra de endereço do Explorer). Para mandar um disco a alguém, use **Exportar**.

### Devolver o resultado

Mande a lista acima marcada e as fotos dos problemas. Se puder, diga a versão do Windows (tecla Windows, digite `winver`).

## Para quem desenvolve

- Cada envio ao GitHub roda `.github/workflows/windows.yml` num Windows de verdade: testes, checagem de que nenhum programa usa biblioteca de rede e o instalador. O `.exe` fica em **Actions → (a execução) → Artifacts**.
- Release (à mão, depois do build verde):

  ```bash
  gh run download <id-da-execução> -n HD-Fantasma-windows-x64 -D dist-win
  gh release create v0.3.0-beta.1 --prerelease --title "0.3.0 beta 1 — Windows (teste)" dist-win/*
  ```

- No Mac, só para conferir que o código do Windows compila: `brew install mingw-w64`, `rustup target add x86_64-pc-windows-gnu` e `cargo check --target x86_64-pc-windows-gnu --workspace --tests`.
- Decisões: [ADR 0019](adr/0019-windows.md).
