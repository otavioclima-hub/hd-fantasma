# .rar pelo `unrar-ng`, .zip pelo crate `zip`; ffprobe como programa à parte

Para listar o conteúdo compactado (só listar, nunca extrair) usamos o crate `zip` (MIT) para .zip e o `unrar-ng` (MIT ou Apache-2.0, versão mantida do `unrar`) para .rar. Compila no Mac e no Windows só com o compilador de C++ que o Rust já exige, sem instalar nada no sistema; testado com arquivos .rar reais (RAR 2.x/3.x/5). Os crates em Rust puro para RAR (`rars`, `rar`) são novos ou incompletos e leem menos variações do formato, e ler RAR de forma confiável é o ponto mais frágil.

**Licença:** o `unrar-ng` embute o código-fonte do UnRAR da RARLAB, que não é MIT: pode ser usado em qualquer programa para ler RAR, de graça, mas não para criar RAR nem recriar o algoritmo de compressão, e o texto da licença (`license.txt` do UnRAR) precisa acompanhar o programa. Isso é compatível com distribuir o HD Fantasma como MIT, porque o app só lê. Obrigação prática: incluir o texto dessa licença nos avisos de terceiros do instalador e do repositório.

O ffprobe não vira dependência do código: é um executável separado, achado em tempo de execução (ao lado do app, no PATH ou em pastas comuns). Se não existir, o catálogo funciona sem os dados de vídeo. Detalhes de empacotamento e licença em `scripts/README-ffprobe.md`.
