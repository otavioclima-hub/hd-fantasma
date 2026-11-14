# Leitura em paralelo, getattrlistbulk no Mac e índice de busca montado no fim

Medição em 2026-11-14: a leitura de um disco gastava quase todo o tempo esperando `readdir` numa pasta por vez. Agora várias pastas são lidas ao mesmo tempo. No Mac, cada pasta é lida com `getattrlistbulk` (a mesma técnica do WhatSize), que traz nome, tipo, tamanho e datas de muitos itens numa só chamada, em vez de uma chamada `stat` por arquivo. O índice de busca (FTS5) é preenchido de uma vez no final da leitura, não item a item.

## Considered Options

- **Helper com privilégio de administrador (como o WhatSize):** exige senha na instalação e assinatura paga da Apple. Rejeitado (ADR 0008); pastas sem permissão são puladas sem travar.
