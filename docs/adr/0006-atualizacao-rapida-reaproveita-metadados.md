---
status: superseded by ADR-0016
---
# Atualização rápida percorre tudo, mas só reprocessa o que mudou

Não dá para confiar na data das pastas para pular partes do disco (FAT32 e exFAT não atualizam isso direito). Então a "atualização rápida" ainda lista o disco inteiro, que é barato, mas só roda o trabalho caro (ffprobe nos vídeos, abrir .zip/.rar) nos arquivos cujo tamanho ou data de modificação mudou. A "releitura completa" apaga e refaz tudo.
