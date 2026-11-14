# Tauri (Rust + interface web) para Mac e Windows

O app precisa rodar no Mac e no Windows com um código só. Escolhemos Tauri: o motor que lê os discos é escrito em Rust (rápido, sem travar com milhões de arquivos) e as telas são HTML/TypeScript. O app fica com ~10 MB, contra ~150 MB do Electron.

## Considered Options

- **Swift nativo (como o DiskCatalogMaker original):** melhor no Mac, mas não roda no Windows.
- **Electron:** mais comum, porém pesado e com leitura de disco mais lenta (Node).
