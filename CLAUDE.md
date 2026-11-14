# HD Fantasma (pasta CATALOGO_HD)

App de Mac e Windows (Tauri) que cataloga discos para pesquisar sem plugar. Projeto open source MIT de Otavio Lima.

- **Memória do projeto: `memoria/INDICE.md` + `memoria/ESTADO.md` (onde paramos).** Ler ao começar, gravar lá (não na `memory/` automática). Pasta fora do git.
- Ler primeiro: `PLANO.md` (escopo e ordem), `CONTEXT.md` (vocabulário — usar esses termos na interface e no código), `docs/adr/` (por que cada decisão).
- Tudo do projeto fica dentro desta pasta (ADR 0009). Pastas pesadas (`node_modules`, `target`) marcadas para o Dropbox ignorar: `xattr -w com.dropbox.ignored 1 <pasta>`.
- Rust via `rustup`; Node 20+. Build de release: `scripts/build-release-mac.sh` (deixa **só o DMG** em `instalador/`, nenhum `.app` no projeto); instalar no Mac: `scripts/install-mac.sh` (do DMG para /Applications, confere que ficaram idênticos).
- Nova decisão difícil de desfazer → nova ADR em `docs/adr/` (numeração sequencial).
- Código e comentários em inglês; interface em pt-BR e en; docs para o Otavio em português.
- O app nunca abre arquivos, nunca usa internet e nunca grava nos discos (ADRs 0013, 0014, 0015). Não quebrar isso.
- MCP em `crates/mcp` (stdio, sem rede): ver `docs/MCP.md`.
