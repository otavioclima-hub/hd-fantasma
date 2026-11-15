# Ligar o HD Fantasma ao Claude (MCP)

## O que é isso

O HD Fantasma guarda uma lista de tudo o que existe nos seus discos. O **MCP** é uma "tomada" que deixa o Claude consultar essa lista. Depois de ligar, você pergunta em português — "onde está o arquivo do cliente X?" — e o Claude procura no catálogo, **mesmo com o disco guardado na gaveta**.

O que dá para fazer:

| O Claude consegue... | Ferramenta |
|---|---|
| Ver quais discos existem e se estão plugados | `listar_discos` |
| Achar arquivos por nome, tipo, tamanho, data e disco | `buscar_arquivos` |
| Ver todos os dados de um arquivo ou pasta | `detalhes_item` |
| Abrir uma pasta e ver o que tem dentro | `listar_pasta` |
| Achar arquivos duplicados e quanto espaço dá para liberar | `achar_duplicados` |
| Ver quais discos estão plugados agora | `discos_plugados` |
| Ler um disco plugado e colocar no catálogo | `catalogar_disco` |

## Segurança, em uma frase cada

- **Sem internet.** O programa só conversa com o Claude dentro do seu próprio Mac (ADR 0015). Ele não tem como se conectar à rede.
- **Não abre seus arquivos.** Só lê nome, tamanho, data e a estrutura de pastas (ADR 0013).
- **Não escreve no disco.** `catalogar_disco` só lê o disco e grava no catálogo. Só aceita um disco inteiro que esteja plugado, nunca uma pasta qualquer.
- **O catálogo fica no computador**: no Mac em `Documentos/HD Fantasma/catalogo.sqlite`, no Windows em `%LOCALAPPDATA%\HD Fantasma\catalogo.sqlite`. O app pode estar aberto ao mesmo tempo.
- Observação: a conversa com o Claude em si passa pela internet (isso é do Claude, não do HD Fantasma). Os resultados das buscas (nomes e caminhos de arquivos) viram parte da conversa.

## Antes de começar

1. Instale o HD Fantasma na pasta **Aplicativos**.
2. Abra o app **uma vez** e leia pelo menos um disco, para o catálogo existir. (Sem catálogo, o Claude responde: "Nenhum catálogo ainda: abra o HD Fantasma e leia um disco.")

O programa do MCP já vem dentro do app, neste endereço:

```
/Applications/HD Fantasma.app/Contents/MacOS/hd-fantasma-mcp
```

## Ligar no Claude Desktop

1. Abra o Claude Desktop.
2. Menu **Claude → Settings… → Developer → Edit Config**. Vai abrir a pasta com o arquivo `claude_desktop_config.json`.
3. Abra esse arquivo num editor de texto e coloque:

```json
{
  "mcpServers": {
    "hd-fantasma": {
      "command": "/Applications/HD Fantasma.app/Contents/MacOS/hd-fantasma-mcp"
    }
  }
}
```

   Se o arquivo já tiver outras coisas dentro de `"mcpServers"`, acrescente só o bloco `"hd-fantasma": { ... }` junto, separado por vírgula. Cuidado para não apagar o que já estava lá.
4. Salve e **feche o Claude Desktop por completo** (Cmd+Q) e abra de novo.
5. Numa conversa nova, clique no ícone de ferramentas (perto da caixa de texto): deve aparecer **hd-fantasma** com as 7 ferramentas.

## Ligar no Claude Code

Cole no Terminal (uma vez só):

```
claude mcp add hd-fantasma -- "/Applications/HD Fantasma.app/Contents/MacOS/hd-fantasma-mcp"
```

Para conferir: `claude mcp list` deve mostrar `hd-fantasma`.

## No Windows

O botão **Conectar ao Claude** faz tudo sozinho também. O programa do MCP fica em
`%LOCALAPPDATA%\HD Fantasma\hd-fantasma-mcp.exe`, e a configuração do Claude Desktop em
`%APPDATA%\Claude\claude_desktop_config.json` (na versão da Microsoft Store, dentro de
`%LOCALAPPDATA%\Packages\Claude_…\LocalCache\Roaming\Claude\`). No Claude Code, pelo PowerShell:

```
claude mcp add --scope user hd-fantasma "$env:LOCALAPPDATA\HD Fantasma\hd-fantasma-mcp.exe"
```

Para ler um disco pelo Claude, informe a letra: "catalogue o disco E:".

## Perguntas para fazer

- "Quais discos eu tenho catalogados? Quais estão plugados agora?"
- "Procure no meu catálogo o arquivo `CLIENTE_final.mov`. Em qual disco ele está?"
- "Ache todos os vídeos com mais de 2GB modificados depois de 1º de janeiro de 2025."
- "O que tem dentro da pasta `Projetos/2025` do disco BACKUP_01?"
- "Tenho arquivos duplicados entre os meus HDs? Quanto espaço eu liberaria apagando as cópias?"
- "Quais pastas do disco ARQUIVO_2023 têm mais coisa dentro?" (o Claude navega pasta por pasta)
- "Plugue o HD novo e catalogue ele." (o Claude usa `discos_plugados` e depois `catalogar_disco`; discos grandes podem levar alguns minutos)

Dicas:
- Dá para pedir tamanho com unidade ("maiores que 500MB") e datas como "depois de 2025-03-01".
- O Claude mostra até 50 resultados por vez e pode buscar mais se você pedir.
- Disco **ausente** (desplugado) aparece normalmente; o Claude avisa em qual disco procurar fisicamente.

## Se algo não funcionar

- **Não aparece hd-fantasma no Claude Desktop:** confira se o app está mesmo em `/Applications` e se o JSON está certo (vírgulas e aspas). Feche com Cmd+Q e abra de novo.
- **"Nenhum catálogo ainda":** abra o HD Fantasma e leia um disco.
- **Os resultados parecem velhos:** use "Ler de novo" no app, ou peça ao Claude para catalogar o disco de novo.
- **Testar sem o Claude (opcional, para técnicos):** `hd-fantasma-mcp --catalog /caminho/catalogo.sqlite` fala o protocolo MCP pela entrada e saída padrão (uma mensagem JSON por linha).

## Para quem desenvolve

- Código: `crates/mcp` (executável `hd-fantasma-mcp`). O protocolo MCP (JSON-RPC por stdio) está escrito à mão em cima de `serde_json`, sem biblioteca de rede, para garantir a ADR 0015. `cargo tree -p hd-fantasma-mcp -e normal` não pode listar hyper, reqwest, tokio nem similares.
- Testes: `cargo test -p hd-fantasma-mcp` (sobem o executável com um catálogo temporário).
- Para o programa entrar dentro do `.app`: rode `scripts/build-mcp-sidecar.sh` antes de `npm run tauri build` (e use `--universal` para Apple Silicon + Intel). Veja a configuração do Tauri (`externalBin`) em `src-tauri/tauri.conf.json`:

```json
"bundle": {
  "externalBin": ["binaries/hd-fantasma-mcp"]
}
```

  O Tauri copia o arquivo para `Contents/MacOS/hd-fantasma-mcp` (sem o sufixo do triple). Ponha `src-tauri/binaries/` no `.gitignore`.
