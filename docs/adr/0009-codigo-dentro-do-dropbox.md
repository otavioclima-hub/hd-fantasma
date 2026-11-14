# Código dentro do Dropbox, com as pastas pesadas fora da sincronização

O código fica na pasta CATALOGO_HD, que está no Dropbox (pedido do Otavio). As pastas que a construção gera (`node_modules`, `src-tauri/target`, vários GB) recebem a marca de "ignorar" do Dropbox, para não sincronizar. Elas são recriadas sozinhas a cada build.
