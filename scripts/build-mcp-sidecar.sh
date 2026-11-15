#!/usr/bin/env bash
# Compila o MCP (hd-fantasma-mcp) em release e põe em src-tauri/binaries/ com o nome que o
# Tauri exige para "externalBin": hd-fantasma-mcp-<target-triple>.
# Funciona no macOS e no Windows (Git Bash). Rode ANTES de `npm run tauri build` (e de `tauri dev`: o Tauri confere se o arquivo existe).
#
# Uso:
#   scripts/build-mcp-sidecar.sh              # só para este Mac (aarch64 ou x86_64)
#   scripts/build-mcp-sidecar.sh --universal  # Apple Silicon + Intel (lipo); use com
#                                             #   npm run tauri build -- --target universal-apple-darwin
set -euo pipefail

export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="$ROOT/src-tauri/binaries"
BIN="hd-fantasma-mcp"
cd "$ROOT"
mkdir -p "$OUT"

HOST="$(rustc -vV | sed -n 's/^host: //p')"

build_for() { # $1 = triple
  echo ">> compilando $BIN para $1"
  cargo build --locked --release -p hd-fantasma-mcp --target "$1"
}

if [[ "${1:-}" == "--universal" ]]; then
  [[ "$(uname -s)" == "Darwin" ]] || { echo "--universal só funciona no macOS." >&2; exit 1; }
  for t in aarch64-apple-darwin x86_64-apple-darwin; do
    rustup target list --installed | grep -qx "$t" || rustup target add "$t"
    build_for "$t"
    cp "target/$t/release/$BIN" "$OUT/$BIN-$t"
  done
  lipo -create -output "$OUT/$BIN-universal-apple-darwin" \
    "target/aarch64-apple-darwin/release/$BIN" "target/x86_64-apple-darwin/release/$BIN"
  lipo -info "$OUT/$BIN-universal-apple-darwin"
else
  EXT=""
  [[ "$HOST" == *windows* ]] && EXT=".exe"
  build_for "$HOST"
  cp "target/$HOST/release/$BIN$EXT" "$OUT/$BIN-$HOST$EXT"
fi

chmod +x "$OUT"/$BIN-*
echo ">> pronto em $OUT:"
ls -l "$OUT"
