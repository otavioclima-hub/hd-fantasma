# A leitura inclui as pastas de nuvem, mas nunca baixa arquivos

Pastas como `~/Library/CloudStorage` (Google Drive, Dropbox) e o iCloud são lidas como qualquer outra, para o total do disco bater com o que o Otavio espera ver (decisão dele, 2026-11-14). Listar essas pastas é lento porque cada uma pede a lista ao servidor; a leitura em paralelo compensa isso. Arquivos que só existem na nuvem (flag `SF_DATALESS` no macOS, atributo `RECALL_ON_DATA_ACCESS` no Windows) entram com nome e tamanho, mas nunca passam por ffprobe nem são abertos como compactado, porque isso baixaria o arquivo inteiro.

## Considered Options

- **Pular as pastas de nuvem por padrão:** leitura bem mais rápida, mas o total do disco ficaria diferente do esperado. Rejeitado pelo Otavio.
