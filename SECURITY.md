# Segurança / Security

## Português

### Como avisar uma falha

Achou uma falha de segurança no HD Fantasma? **Não abra uma issue pública.** Avise em particular:

1. Abra a aba **Security** do repositório.
2. Clique em **Report a vulnerability** ([link direto](https://github.com/otavioclima-hub/hd-fantasma/security/advisories/new)).
3. Conte o que acontece, em qual versão e, se der, como repetir.

Só o mantenedor vê o aviso. A resposta vem pelo próprio GitHub. O conserto sai numa versão nova e o aviso é publicado depois, com crédito para quem avisou (se a pessoa quiser).

### Versões que recebem correção

Só a versão mais recente da página de [Releases](https://github.com/otavioclima-hub/hd-fantasma/releases).

### Baixe só daqui

O instalador oficial existe **apenas** na página de [Releases](https://github.com/otavioclima-hub/hd-fantasma/releases) deste repositório. O app não é assinado pela Apple, então desconfie de cópias recebidas por outro caminho. Para conferir o arquivo, compare o SHA-256 com o que aparece ao lado dele na página de Releases:

```bash
shasum -a 256 ~/Downloads/HD-Fantasma-*-mac-universal.dmg
```

### O que o app promete

- Nunca usa internet.
- Nunca abre o conteúdo dos arquivos, só lê nome, tamanho, formato e datas.
- Nunca grava nada nos discos que lê.

Qualquer coisa que quebre uma dessas promessas conta como falha de segurança.

## English

### Reporting a vulnerability

Please **do not open a public issue.** Report privately through the repository's **Security** tab → **Report a vulnerability** ([direct link](https://github.com/otavioclima-hub/hd-fantasma/security/advisories/new)). Include what happens, which version, and steps to reproduce if possible. Only the maintainer sees the report. Fixes ship in a new release and the advisory is published afterwards, crediting the reporter if they wish.

### Supported versions

Only the latest release on the [Releases](https://github.com/otavioclima-hub/hd-fantasma/releases) page.

### Download only from here

The official installer is published **only** on this repository's [Releases](https://github.com/otavioclima-hub/hd-fantasma/releases) page. The app is not signed by Apple, so be wary of copies obtained elsewhere. Compare the file's SHA-256 (`shasum -a 256 <file>`) with the digest shown next to it on the Releases page.

### What the app promises

It never uses the network, never opens file contents (only name, size, type and dates), and never writes to the disks it reads. Anything that breaks one of these promises is a security issue.
