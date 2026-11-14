# Pastas de nuvem: só o que já está no Mac, sem nunca ir à internet

Teste de 2026-11-14: ler `~/Library/CloudStorage` (Google Drive) travava a leitura, porque cada pasta ainda não trazida para o Mac é buscada no servidor, segundos por pasta. Decisão do Otavio: catalogar só as pastas e arquivos que já estão no Mac. A leitura liga a política do macOS `IOPOL_TYPE_VFS_MATERIALIZE_DATALESS_FILES = OFF` nas suas threads, então o sistema nunca busca nada na nuvem para o app; arquivos e pastas marcados como "só na nuvem" (`SF_DATALESS`) ficam fora do catálogo, e pastas de nuvem que sobrarem vazias também. O total de GB do disco não muda, porque o que está só na nuvem não ocupa espaço. Vale para Google Drive, Dropbox (File Provider) e iCloud, sem opção na tela.

Substitui a ADR 0011.
