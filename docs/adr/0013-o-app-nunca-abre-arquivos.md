---
status: accepted
---
# O app nunca abre arquivos

Decisão do Otavio em 2026-11-14, depois de testar a leitura do disco interno: o catálogo guarda só o que o sistema informa sem abrir o arquivo — nome, tipo (pela extensão), tamanho, datas e a estrutura de pastas. Saem os dados de vídeo (ffprobe) e o conteúdo de .zip/.rar, porque abrir cada arquivo deixava a leitura lenta. Isso substitui a ADR 0010 e a parte de "detalhes" da ADR 0006, e tira do escopo os itens "dados de vídeo" e "conteúdo de .zip/.rar" do PLANO.md.
