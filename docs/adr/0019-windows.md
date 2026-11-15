# Windows: mesmo repositório, build na nuvem do GitHub

O Otavio não tem Windows; quem testa é um parceiro. Decisões da versão de Windows (2026-11-15):

- **Mesmo repositório e mesmo código.** O Tauri (ADR 0001) já é feito para os dois sistemas; o que muda fica atrás de `#[cfg(windows)]` em poucos lugares (`volumes.rs`, `fastdir.rs`, `scan.rs`, `claude.rs`). Um repositório só: uma página de Releases, uma lista de problemas, as correções valem para os dois.
- **Build e testes num Windows de verdade, pelo GitHub Actions** (`.github/workflows/windows.yml`, `windows-latest`). Cada envio roda os testes do motor, do MCP e do app no Windows e gera o instalador `.exe` (NSIS, sem pedir senha de administrador). A Release é feita à mão com esse arquivo. Só actions do próprio GitHub; o token do job é só leitura.
- **Disco = letra de unidade.** O id é o número de série do volume (fica gravado no próprio disco, igual em qualquer PC). Unidade de rede mapeada: servidor + compartilhamento. Externo ou interno: pelo tipo de ligação (USB, FireWire, cartão SD = externo); o disco do sistema (`C:`) é sempre interno.
- **Nuvem (ADR 0014):** OneDrive, Dropbox e outros marcam o que está só na nuvem com atributos do Windows (`RECALL_ON_DATA_ACCESS`, `RECALL_ON_OPEN`, `OFFLINE`). Esses itens ficam fora, como no Mac.
- **No disco do sistema**, pula `Windows`, `Program Files`, `ProgramData` e outras pastas do Windows (como no Mac pula as do macOS).
- **Catálogo em `%LOCALAPPDATA%\HD Fantasma\`, não em Documentos.** No Windows a pasta Documentos costuma estar dentro do OneDrive, e um banco SQLite sendo sincronizado (ou virando "só online") pode se estragar.
- **Sem assinatura da Microsoft** (é paga, como a da Apple). O Windows mostra o aviso do SmartScreen na primeira vez; o guia ensina a passar.
- Só x64 por enquanto (roda também nos Windows com processador ARM, por emulação).

## Considered Options

- **Repositório separado para o Windows:** duplicaria o código e as correções. Rejeitado.
- **Gerar o `.exe` no Mac (cross-compile):** o Tauri chama isso de experimental, e os testes não rodam. Fica só como checagem de que o código compila (`cargo check --target x86_64-pc-windows-gnu`).
