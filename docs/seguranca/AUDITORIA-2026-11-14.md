# HD Fantasma — auditoria de segurança (rodada 1)

**Data:** 2026-11-14 · **Perfil:** rápido (`quick`) · **Escopo:** app inteiro (núcleo Rust, casca Tauri, telas Svelte, dependências, empacotamento), menos `crates/mcp`.

## 1. Como foi feita

- Método: skill `security-audit` da Cloudflare, 6 fases. Ajudantes usados: 12 de 16 planejados (4 de mapeamento, 4 caçadores, 1 crítico, 3 verificadores independentes).
- Código auditado: commit `15b62cd` **mais** as mudanças ainda não commitadas (ADR 0015/0016: CSP ligada, plugin `opener` removido, trava de navegação). Foi tirada uma cópia congelada no início, porque outra conversa estava editando a pasta.
- Execução: só leitura de código e testes pequenos dentro de uma caixa isolada (sem internet, escrita só na pasta de rascunho, limite de CPU/tempo). Limitação: o macOS não permite limitar memória nessa caixa; por isso os testes usaram dados minúsculos. Nada foi instalado, nada saiu para a internet, o app real não foi aberto.
- Rodadas anteriores: nenhuma.
- **Esta é uma passada parcial.** Perfil rápido = uma rodada de caça e um crítico. Ficaram 5 áreas adiadas (seção 7). Não é atestado de "não há mais nada".

## 2. Resumo da situação

O app está em bom estado para o que se propõe. Nenhuma falha grave, alta ou média. Os três pilares do projeto se sustentam no código:

- **Sem internet:** nenhuma biblioteca de rede entra no binário de Mac (conferido com `cargo tree` offline: `reqwest`, `hyper`, `mio`, `socket2` ausentes; `tokio` sem o recurso `net`). CSP bloqueia conexões externas; a janela não navega para fora.
- **Nunca abre arquivos:** a leitura de disco só lista pastas; não abre conteúdo, não segue atalhos (symlinks), não sai do volume, não baixa arquivos de nuvem.
- **Texto do disco não vira código:** nomes de arquivos e discos aparecem só como texto na tela (sem `{@html}`, sem `innerHTML`); a busca usa parâmetros no SQL (testada com entradas maliciosas).

O ponto fraco é um só: **importar um arquivo `.hdfantasma` recebido de outra pessoa**. O app confia demais nesse arquivo. Duas falhas de gravidade baixa confirmadas ali.

## 3. Falhas confirmadas

| Gravidade | Título | Fronteira | Resultado observado |
|---|---|---|---|
| Baixa | Arquivo importado com laço de pastas trava a importação para sempre | arquivo `.hdfantasma` de terceiros → catálogo | Importação nunca termina; gravações no catálogo falham; só sai fechando o app à força |
| Baixa | Arquivo importado com dado de tipo errado quebra busca e listagem | arquivo `.hdfantasma` de terceiros → catálogo | Busca e listagem dão erro até o disco importado ser removido |

### 3.1 Laço de pastas trava a importação (`core/db/recompute_descendants/parent-cycle-hang`)

- **Onde:** `crates/core/src/db.rs`, função `recompute_descendants`, laço na linha 465; chamada por `import_disk` (linha 421).
- **Quem ataca:** alguém que manda um `.hdfantasma` adulterado e convence o usuário a importar.
- **Como reproduzir (teste local):** exportar um disco pequeno; no arquivo, rodar `UPDATE items SET parent_id=id WHERE name='a'` (uma pasta que é "mãe de si mesma"); importar.
- **Condições:** o usuário precisa escolher o arquivo na janela de importar.
- **O que acontece:** o código sobe de pasta em pasta para contar itens e nunca chega ao topo. A importação não volta e não tem botão de cancelar. Enquanto isso, ler o catálogo funciona, mas gravar falha com "database is locked" depois de 10 s. Nada é gravado: ao fechar o app à força, o catálogo fica intacto.
- **Por que baixa:** só indisponibilidade, sem perda nem vazamento, e depende de o usuário importar o arquivo.
- **Menor correção:** limitar o número de passos da subida ao número de itens e devolver erro "exportação inválida" se passar. Teste de regressão com pasta mãe-de-si-mesma e com ciclo de 2.

```rust
for (_, p) in parent.iter() {
    let mut cur = *p;
    let mut steps = 0usize;
    while let Some(pid) = cur {
        steps += 1;
        if steps > parent.len() {
            return Err(Error::msg("Este arquivo não é uma exportação válida do HD Fantasma."));
        }
        *count.entry(pid).or_default() += 1;
        cur = parent.get(&pid).copied().flatten();
    }
}
```

### 3.2 Dado de tipo errado quebra busca e listagem (`core/db/import_disk/column-type-poison`)

- **Onde:** `crates/core/src/db.rs`, `import_disk` (cópia crua na linha 404), `row_to_item` (linhas 480–489); `crates/core/src/search.rs` linhas 96 e 107.
- **Quem ataca:** o mesmo remetente de arquivo adulterado.
- **Como reproduzir (teste local):** no arquivo exportado, `UPDATE items SET child_count='abc' WHERE name='zzbad.txt'`; importar; buscar.
- **O que acontece:** o SQLite aceita texto em coluna de número. Depois, ao ler essa linha, o app dá erro e descarta o resultado inteiro. Qualquer busca cuja página inclua a linha ruim falha, mesmo trazendo itens de outros discos. A lista de discos continua funcionando; remover o disco importado resolve.
- **Por que baixa:** recuperável pela própria tela, sem perda nem vazamento.
- **Menor correção:** antes de copiar, conferir o tipo de cada coluna e recusar o arquivo:

```sql
SELECT COUNT(*) FROM imp.items WHERE typeof(id)!='integer'
  OR typeof(parent_id) NOT IN ('integer','null')
  OR typeof(name)!='text' OR typeof(path)!='text' OR typeof(kind)!='text'
  OR typeof(size_bytes)!='integer' OR size_bytes<0
  OR typeof(child_count)!='integer' OR child_count<0
  OR typeof(ext) NOT IN ('text','null')
  OR typeof(created_at) NOT IN ('text','null')
  OR typeof(modified_at) NOT IN ('text','null');
```

## 4. Precisa de validação

Nenhum registro nesta categoria. Duas conferências ficam com o dono do projeto por dependerem de fatos fora do código:

- Depois de gerar o app: `codesign -dvv --entitlements - "HD Fantasma.app"` e `spctl -a -vv "HD Fantasma.app"`, para ver como o pacote sai assinado.
- Antes de abrir o repositório: varrer o histórico do git por segredos e conferir o e-mail dos commits.

## 5. Reforços recomendados (não são falhas)

**Importação e catálogo**
1. Importar disco com UUID que já existe: mostrar "isso vai substituir o disco X (nome, data, nº de itens)" e pedir confirmação. Hoje troca direto e não tem desfazer. (Candidato analisado e rejeitado como falha de segurança: é comportamento documentado; fica como melhoria.)
2. Na importação, recusar caminho absoluto ou com `..`; em `resolve_item_path` (`search.rs:176`), exigir que o resultado fique dentro do disco. Hoje um arquivo importado consegue fazer o "Mostrar no Finder" apontar para fora do disco (só revela, não abre).
3. Limitar tamanho/quantidade de linhas de uma importação.
4. `export_disk`: recusar destino igual ao catálogo em uso (hoje apagaria o catálogo).

**Janela e ponte com o Rust**
5. `allow_navigation`: aceitar só a origem exata do app; tirar `asset`, `ipc` e `*.localhost` em qualquer porta.
6. Trocar `core:default` pelo mínimo usado; declarar os 15 comandos no `build.rs` (`AppManifest`) e liberar um a um na capability.
7. Chamar a janela de abrir/salvar pelo Rust, ou conferir a extensão `.hdfantasma` no Rust.
8. Validar as larguras lidas do `localStorage` (número finito, com limites). Tirar `asset:` da CSP.

**Leitura de disco**
9. Somar tamanhos com `saturating_add` (hoje pode dar volta com arquivos esparsos gigantes: só número errado na tela).
10. Abrir pastas com `openat` + `O_NOFOLLOW` e conferir o dispositivo pelo descritor aberto.
11. Checar `p+4` antes de `rd32` em `fastdir.rs`. Lembrar que `panic=abort` anula o `catch_unwind` no release.
12. Disco clonado com mesmo UUID substitui o original ao ler: avisar se capacidade/sistema de arquivos mudou muito.
13. Tempo limite na chamada do `diskutil`.

**Dependências e publicação**
14. Gerar o release com `--locked`. Guarda automática: `cargo tree -i reqwest` e `-i hyper` têm de sair vazios para os dois alvos de Mac.
15. Rodar `cargo audit` e `npm audit` (precisam de internet; não rodados aqui).
16. `bundle.targets`: trocar `"all"` por `["app","dmg"]`.
17. Publicar SHA-256 de cada instalador (melhor: atestado de artefato do GitHub) e explicar no README que o app não é assinado e como conferir. Nas instruções, preferir "botão direito → Abrir" a `xattr -cr`.
18. Antes de abrir o repositório: tirar e-mail e notas pessoais do `CLAUDE.md` (ou ignorá-lo); pôr `.claude/`, `.vscode/`, `*.sqlite`, `*.hdfantasma`, `src-tauri/binaries/` no `.gitignore`; README ainda é o modelo do Tauri.
19. Marcar a ADR 0008 como substituída pela 0015 (ela ainda fala em atualizador).
20. Nunca distribuir build de debug (libera `localhost:1420` e devtools).

**Padrões bons encontrados**
- SQL sempre com parâmetros; termos de busca entre aspas no FTS.
- `diskutil` e `open` chamados sem shell, com caminho absoluto.
- Gatilhos de um arquivo importado nunca disparam (só há `SELECT` sobre ele).
- Capability sem `fs`, `shell`, `http`, `opener`; CSP estrita; sem devtools no release.
- Travas de versão (`Cargo.lock`, `package-lock.json`) commitadas, só do registro oficial, com checksums.

## 6. Cobertura

19 unidades: 13 cobertas, 1 com candidatos (resolvidos: 2 confirmados, 1 rejeitado), 5 adiadas, 0 bloqueadas. Os dois validadores da skill passaram.

## 7. O que ficou de fora (para a próxima rodada)

1. **`crates/mcp`** (servidor MCP para o Claude): estava sendo escrito durante a auditoria. Auditar quando estabilizar — é superfície nova (ferramentas chamadas por IA) e pode ir para dentro do instalador.
2. **Catálogo adulterado na pasta Documentos** por outro programa: a migração roda sem validar; o mesmo laço da falha 3.1 travaria o app toda vez que abre; gatilhos plantados no arquivo rodariam.
3. **Comandos ao mesmo tempo** (ler + importar + cancelar): estados parciais, sinal de cancelar "velho".
4. **Disco com UUID clonado** trocando o catálogo de outro disco na leitura.
5. **Listagens sem limite** (`list_children`) com pasta de milhões de itens.
