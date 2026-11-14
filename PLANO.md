# HD Fantasma — plano

Decisões tomadas com o Otavio em 2026-11-14 (4 rodadas de perguntas). O porquê de cada decisão difícil está em `docs/adr/`. Vocabulário em `CONTEXT.md`.

## Escopo da versão 1

- Ler discos presentes: HD/SSD externo, disco interno, pen drive, cartão, NAS. Qualquer formatação que o sistema monte (ADR 0004).
- Guardar de cada item: nome, extensão/tipo, tamanho, datas (criação, modificação), caminho relativo ao disco.
- Dados de vídeo via ffprobe embutido: duração, largura×altura, codec, fps.
- Conteúdo compactado: .zip e .rar.
- Ignorar arquivos de sistema por padrão (.DS_Store, Thumbs.db, .Spotlight-V100, .Trashes, .fseventsd, $RECYCLE.BIN, System Volume Information, ._*), com opção de incluir.
- Tela em 3 colunas (discos | pastas | itens) + barra de busca.
- Busca em todos os discos, com filtros: tipo (vídeo/imagem/áudio/documento/compactado/outro), tamanho mín/máx, data, discos escolhidos.
- Navegar disco ausente.
- Duplicados (nome + tamanho, ADR 0007).
- Atualização rápida + releitura completa (ADR 0006).
- Exportar/importar catálogo de um disco (`.hdfantasma`).
- Idiomas: português e inglês.
- Atualizador do Tauri pelo GitHub Releases; nada mais sai do computador (ADR 0008).
- MCP (`hd-fantasma-mcp`): buscar com filtros, listar discos, detalhes do item, navegar pastas, duplicados, catalogar disco presente (ADR 0005).

## Ordem

1. Mac (Apple Silicon + Intel).
2. Windows — build pelo GitHub Actions, teste feito por um parceiro.
3. MCP.

Repo `otavioclima-hub/hd-fantasma`, privado até o Mac ficar bom, depois público. Licença MIT, autor Otavio Lima. Ícone em vetor feito pelo Claude.

## Arquitetura

```
CATALOGO_HD/
├── crates/core/     biblioteca Rust: banco (SQLite+FTS5), leitura de disco, busca, duplicados, exportação
├── crates/mcp/      executável do MCP (etapa 3), usa crates/core
├── src-tauri/       app Tauri: comandos que chamam crates/core
├── src/             telas (TypeScript + Vite, sem framework pesado)
└── docs/adr/
```
