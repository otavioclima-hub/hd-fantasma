# Formatações de disco dependem do sistema operacional

O app lê qualquer disco que o sistema monte; não tem leitor próprio de formatação. No Windows, discos Mac Journaled (HFS+) e APFS não aparecem sem programa extra: o app explica isso e indica o Paragon. Escrever um leitor de HFS+/APFS seria um projeto maior que o próprio app.
