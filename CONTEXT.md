# HD Fantasma

App que cataloga discos para encontrar arquivos sem precisar plugar o disco.

## Language

**Catálogo**:
O registro, guardado no computador, de todos os discos já lidos e de tudo o que havia neles.
_Avoid_: banco, base, índice

**Disco**:
Um volume que o sistema monta (HD, SSD, pen drive, cartão, compartilhamento de rede), reconhecido pela sua identidade única e não pelo nome.
_Avoid_: HD (quando o assunto for qualquer tipo), drive, volume, mídia

**Disco presente**:
Disco catalogado que está plugado agora.
_Avoid_: online, montado

**Disco ausente**:
Disco catalogado que não está plugado; continua navegável e pesquisável pelo catálogo.
_Avoid_: offline, desconectado

**Item**:
Uma entrada do catálogo: arquivo ou pasta, com nome, tipo, tamanho, datas e caminho dentro do disco.
_Avoid_: entrada, registro, nó

**Conteúdo compactado**:
Os itens que estão dentro de um arquivo .zip ou .rar, registrados como filhos dele.
_Avoid_: arquivo interno

**Dados de vídeo**:
Duração, resolução, codec e quadros por segundo de um item de vídeo.
_Avoid_: metadados (genérico demais)

**Leitura**:
O ato de percorrer um disco presente e gravar seus itens no catálogo.
_Avoid_: scan, varredura, indexação

**Atualização rápida**:
Leitura que reaproveita os dados de vídeo e o conteúdo compactado dos itens que não mudaram.
_Avoid_: sincronizar

**Releitura completa**:
Leitura que descarta tudo o que havia daquele disco e refaz do zero.
_Avoid_: reset, recatalogar

**Duplicado**:
Itens em qualquer disco com o mesmo nome e o mesmo tamanho.
_Avoid_: cópia, igual

**Exportação**:
Arquivo `.hdfantasma` com o catálogo de um único disco, para mandar a outra pessoa.
_Avoid_: backup
