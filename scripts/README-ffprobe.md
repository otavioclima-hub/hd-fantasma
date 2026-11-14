# ffprobe no HD Fantasma

O app usa o `ffprobe` (do projeto FFmpeg) para ler duração, resolução, codec e fps dos vídeos. Ele roda como **programa separado**; o app nunca o baixa sozinho.

## Como o app acha o ffprobe (`find_ffprobe`, em `crates/core/src/extract.rs`)

1. Ao lado do executável do app (é onde o Tauri coloca um "sidecar"; no instalador ele se chama só `ffprobe` / `ffprobe.exe`).
2. No `PATH`.
3. Lugares comuns: `/opt/homebrew/bin`, `/usr/local/bin`, `/usr/bin`, `~/bin`.

Sem ffprobe, a Leitura funciona normalmente, só que sem os dados de vídeo (os .zip/.rar continuam sendo lidos).

## O que o fluxo de lançamento (GitHub Actions) precisa fazer

O `tauri.conf.json` **não** declara `externalBin` ainda, porque o `cargo build`/`tauri build` falha se o arquivo não existir. Na hora de gerar o instalador:

1. Baixar um ffprobe **estático** para cada alvo (ex.: `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-pc-windows-msvc`) e salvar em
   `src-tauri/binaries/ffprobe-<target-triple>` (no Windows, `ffprobe-x86_64-pc-windows-msvc.exe`).
   `rustc -vV` mostra o triple do alvo. No macOS dê `chmod +x`.
2. Acrescentar em `src-tauri/tauri.conf.json`, dentro de `bundle`:
   `"externalBin": ["binaries/ffprobe"]`
3. Rodar `tauri build`. O Tauri copia o binário para ao lado do app, sem o sufixo do triple.

Dica: o passo 2 pode ser feito pelo próprio workflow (só depois de baixar os binários), para o build local seguir funcionando sem eles.

## Licença (importante)

Os builds estáticos do ffprobe/FFmpeg são **LGPL ou GPL**, dependendo de como foram compilados (escolha um build LGPL se possível). Distribuí-lo como executável separado ao lado do app MIT é permitido, mas exige:

- incluir o **texto da licença** do ffprobe (LGPL/GPL) nos avisos de terceiros do instalador e do repositório;
- dar o **link para o código-fonte** da versão exata distribuída (ou oferecê-lo);
- não misturar o código do ffprobe ao do app (hoje só o executamos, o que mantém o app MIT).

Também incluir o texto da licença do UnRAR (ver `docs/adr/0010-*.md`).
