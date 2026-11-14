#!/usr/bin/env bash
# Builds the universal (Apple Silicon + Intel) HD Fantasma .app and .dmg and checks them.
# Usage: scripts/build-release-mac.sh
set -euo pipefail

export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"
[[ -s "$HOME/.nvm/nvm.sh" ]] && . "$HOME/.nvm/nvm.sh"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

[[ "$(uname -s)" == "Darwin" ]] || { echo "Only works on macOS." >&2; exit 1; }
for t in aarch64-apple-darwin x86_64-apple-darwin; do
  rustup target list --installed | grep -qx "$t" || rustup target add "$t"
done

echo ">> 1/3 MCP sidecar (universal)"
scripts/build-mcp-sidecar.sh --universal

echo ">> 2/3 app (universal-apple-darwin)"
npm run tauri build -- --target universal-apple-darwin --bundles app,dmg

BUNDLE="$ROOT/target/universal-apple-darwin/release/bundle"
APP="$BUNDLE/macos/HD Fantasma.app"
DMG="$(ls -t "$BUNDLE"/dmg/*.dmg | head -n 1)"

echo ">> 3/3 checks"
fail=0
for bin in "$APP/Contents/MacOS/hd-fantasma" "$APP/Contents/MacOS/hd-fantasma-mcp"; do
  if [[ ! -f "$bin" ]]; then echo "MISSING: $bin"; fail=1; continue; fi
  archs="$(lipo -archs "$bin")"
  echo "$(basename "$bin"): $archs"
  [[ "$archs" == *arm64* && "$archs" == *x86_64* ]] || { echo "NOT universal: $bin"; fail=1; }
done
[[ -f "$APP/Contents/Resources/THIRD_PARTY_LICENSES.md" ]] || { echo "MISSING: THIRD_PARTY_LICENSES.md in Resources"; fail=1; }
[[ $fail -eq 0 ]] || { echo "CHECK FAILED" >&2; exit 1; }

echo
echo "DMG: $DMG"
echo "Size: $(du -h "$DMG" | cut -f1)"
