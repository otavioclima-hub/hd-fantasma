# O MCP é um programa à parte que lê o banco direto

O MCP (ligação do Claude com o catálogo) é um executável separado, `hd-fantasma-mcp`, que abre o mesmo `catalogo.sqlite` e usa o mesmo motor Rust do app. Assim o Claude consulta o catálogo com o app fechado. A ferramenta de catalogar um disco plugado também roda dentro do MCP; o motor só lê os discos, nunca grava neles.
