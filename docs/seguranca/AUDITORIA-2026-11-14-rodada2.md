# HD Fantasma — auditoria de segurança, rodada 2

**Data:** 2026-11-14 · **Perfil:** rápido (`quick`), **com escopo**: servidor MCP, botão "Conectar ao Claude", scripts do instalador, catálogo em uso (abertura, migração, concorrência, identidade por UUID) e tentativa de furar as correções da rodada 1. As áreas cobertas na rodada 1 e sem mudança não foram caçadas de novo (marcadas `out_of_scope` no ledger).
**Código:** commit `8882552` (cópia congelada). Ajudantes: 9 de 14 (4 caçadores, 1 crítico, 4 verificadores). Execução só local, em caixa isolada sem internet. Rodada anterior lida: `../run-1`.

**É uma passada parcial.** Uma lacuna nova do crítico ficou adiada (seção 6).

## 1. Resumo

Nenhuma falha grave, alta ou média. Três falhas baixas confirmadas, todas de novo na **importação de `.hdfantasma` de terceiros** (a rodada 1 tinha fechado as duas primeiras; estas furavam ou sobravam das correções). O MCP e o botão "Conectar ao Claude" não têm falha confirmada: só reforços. Nenhuma porta de rede existe; o MCP fala só por stdin/stdout com o programa do Claude que o iniciou.

## 2. Falhas confirmadas (todas baixas)

| Falha | Efeito observado | Situação |
|---|---|---|
| `core/db/import_disk/view-toctou-bypass` | Arquivo com uma *view* no lugar da tabela passa na checagem e entra com dados ruins; busca quebra até apagar o disco | Corrigida: só tabelas comuns são aceitas |
| `core/db/import_disk/id-max-overflow-poison` | Id gigante num catálogo vazio faz toda leitura e importação seguinte falhar | Corrigida: limite de id e conta feita em Rust |
| `core/db/recompute_descendants/chain-quadratic` | Cadeia longa de pastas deixa a importação lenta (quadrática) | Corrigida: contagem em uma passada só |

Rejeitada como falha (fica como melhoria, já feita): catálogo danificado travava o app na abertura.

## 3. Correções aplicadas depois da rodada (no código de trabalho)

- Importação: só tabelas comuns; ids até 10¹²; nomes até 1024 caracteres; texto de busca refeito a partir dos nomes; contagem de pastas linear.
- Catálogo em uso: arquivo danificado é guardado de lado (renomeado, nunca apagado) e o app abre com aviso; catálogo de versão mais nova é recusado sem mexer; catálogo ocupado espera em vez de dar erro.
- MCP: recusa qualquer volume interno (pelo sistema, não só pelo caminho); recusa substituir disco do catálogo com outro nome; nomes limpos e curtos nas mensagens; contas sem estouro; listagem paginada no banco; linha de entrada limitada a 8 MB.
- Botão Conectar: arquivos temporários e cópia de segurança sem seguir atalhos; tempo limite real; só roda um `claude` do próprio usuário e não alterável por outros.
- Janela: só os 18 comandos usados, um a um, mais eventos de progresso e janelas de abrir/salvar (`core:default` saiu); navegação só para a origem exata do app.
- Instalador: `--locked`, checagem automática de "nenhuma biblioteca de rede" nos dois tipos de Mac, conferência byte a byte do MCP embutido; alvos `app` e `dmg`.
- Dependências: SQLite embutido atualizado (rusqlite 0.41 / SQLite 3.54.1) por falha conhecida RUSTSEC-2026-0459.

## 4. Reforços que ficaram (não são falhas)

- Linhas com tipo errado num catálogo em uso adulterado ainda quebram listagens (só alguém com acesso aos Documentos consegue).
- `catalogar_disco` não tem cancelamento e segura o catálogo durante a leitura.
- Configuração do Claude com chaves duplicadas ou números enormes muda ao ser regravada.
- Avisar na tela de leitura quando o disco plugado tem a identidade de outro já catalogado.

## 5. Cobertura

13 unidades na rodada: 11 cobertas, 2 com candidatos (resolvidos: 3 confirmados, 1 rejeitado). 12 fora do escopo (cobertas na rodada 1). 1 adiada. Validadores da skill: PASS.

## 6. Adiado

- Releitura pelo MCP de um volume com UUID de outro disco do catálogo (crítico). Mitigado no código: o MCP recusa quando o nome guardado é diferente; não houve caça formal.
