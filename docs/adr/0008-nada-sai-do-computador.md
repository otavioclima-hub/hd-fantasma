# Nada sai do computador; instalação sem assinatura paga

> **Substituída em parte pela ADR 0015 (2026-11-14):** não existe mais checagem de versão nem atualizador; o app não acessa a internet de jeito nenhum. Não readicionar `tauri-plugin-updater` nem `tauri-plugin-opener` (trariam bibliotecas de rede para o binário de macOS). A parte "sem assinatura paga" continua valendo.

O app não manda dados de uso nem relatórios de erro (o original usa Firebase). A única conexão é a checagem de versão nova no GitHub, pelo atualizador do Tauri, que tem chave própria e grátis. O app não é assinado pela Apple nem pela Microsoft: o README ensina a passar o aviso de "app desconhecido".
