# Botão "Conectar ao Claude" edita a configuração do Claude pela pessoa

Para quem não é técnico, editar o `claude_desktop_config.json` à mão é arriscado (uma vírgula quebra todos os MCPs). O app tem um botão no topo que faz isso: no Claude Desktop, lê o JSON, acrescenta/remove só a entrada `mcpServers.hd-fantasma` (preservando todo o resto), guarda uma cópia `.bak` antes e grava de forma atômica; no Claude Code, usa o próprio comando `claude mcp add/remove --scope user` quando o executável é encontrado (não editamos o `~/.claude.json` direto, porque o Claude Code reescreve esse arquivo enquanto roda). O caminho do MCP é o do binário ao lado do app em execução, então funciona onde quer que o app esteja instalado. Nada disso usa internet (ADR 0015).

## Considered Options

- **Extensão `.mcpb` do Claude Desktop:** dois cliques para instalar, mas só cobre o Claude Desktop e exige manter um segundo arquivo de distribuição. Dispensada pelo Otavio.
