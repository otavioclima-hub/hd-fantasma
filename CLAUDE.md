# HD Fantasma (pasta CATALOGO_HD)

App de Mac e Windows (Tauri) que cataloga discos para pesquisar sem plugar. Projeto pessoal do Otavio Lima, open source MIT. **Não é da Arara nem da Last Potato.**

- Ler primeiro: `PLANO.md` (escopo e ordem), `CONTEXT.md` (vocabulário — usar esses termos na interface e no código), `docs/adr/` (por que cada decisão).
- Tudo do projeto fica dentro desta pasta (pedido do Otavio, ADR 0009). Pastas pesadas (`node_modules`, `target`) marcadas para o Dropbox ignorar: `xattr -w com.dropbox.ignored 1 <pasta>`.
- Rust instalado via Homebrew `rustup` (2026-11-14). Node pelo nvm: carregar `. ~/.nvm/nvm.sh` antes de `npm`.
- Git: `user.email` local já fixado (noreply do GitHub). Repo remoto `otavioclima-hub/hd-fantasma` privado até a versão de Mac estar boa.
- Nova decisão difícil de desfazer → nova ADR em `docs/adr/` (numeração sequencial).
- Código e comentários em inglês; interface em pt-BR e en; docs para o Otavio em português.
