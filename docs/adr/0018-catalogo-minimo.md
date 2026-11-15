# Catálogo mínimo (formato v4)

O Otavio quer o catálogo o menor possível. Com o HD interno do Mac (2,9 milhões de itens) ele tinha 2,2 GB. Medido numa cópia real, o formato v4 deixa o mesmo catálogo com 454 MB (−79%), com os mesmos caminhos, as mesmas datas e os mesmos resultados de busca, item por item:

- **Arquivos não guardam o caminho.** Só as pastas guardam; o caminho de um arquivo é o da pasta + o nome, montado na consulta (`ITEM_COLS` + `ITEM_JOIN`). Era o que mais se repetia (127 letras em média por item).
- **Datas em segundos (número), não em texto.** Voltam como texto ISO na consulta, então nada muda fora do banco.
- **Índice de busca sem posições** (`fts5 … detail=none, columnsize=0`). A busca passou de `MATCH` para `GLOB '*texto*'` (ou `LIKE` quando o texto tem `*`, `?` ou `[`), que esse índice responde sem erro: o SQLite confere cada candidato com o texto guardado. Ficou igual ou mais rápida.
- **Índice de duplicados só pelo tamanho** (ADR 0007 continua: mesmo nome + mesmo tamanho; o nome é comparado nas poucas linhas de cada tamanho).
- **O arquivo devolve espaço** depois de remover ou ler de novo um disco (`auto_vacuum=INCREMENTAL`).

Catálogos antigos são convertidos uma vez, ao abrir (cerca de 25 s para 2,9 milhões de itens), com a janela já aberta e um aviso. Exportações antigas continuam importando; exportações v4 não abrem em versões antigas do app (elas avisam para atualizar).

Também: depois de ler, remover ou importar um disco, o app devolve ao macOS a memória que sobrou do trabalho (`malloc_zone_pressure_relief`).
