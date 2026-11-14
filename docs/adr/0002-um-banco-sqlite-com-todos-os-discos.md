# Um único banco SQLite com todos os discos, na pasta Documentos

Todos os discos catalogados ficam num único arquivo SQLite em `Documentos/HD Fantasma/catalogo.sqlite` (Mac e Windows). Assim a busca em todos os discos é uma consulta só, com índice de texto (FTS5), e o MCP lê o mesmo arquivo sem o app aberto. Para compartilhar um disco, o app exporta o catálogo dele num arquivo separado (`.hdfantasma`, que também é SQLite) e importa de volta.

## Considered Options

- **Um arquivo por disco (como o original):** fácil de mandar para alguém, mas a busca global teria que abrir dezenas de arquivos.

## Consequences

- O banco usa modo WAL para o app e o MCP poderem abrir ao mesmo tempo.
