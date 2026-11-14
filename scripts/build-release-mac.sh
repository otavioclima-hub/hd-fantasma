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

echo ">> 0/3 no-network check (ADR 0015)"
# The app and the MCP must not contain any networking library, on either Mac architecture.
for t in aarch64-apple-darwin x86_64-apple-darwin; do
  for p in hd-fantasma hd-fantasma-mcp; do
    for net in reqwest hyper mio socket2 ureq; do
      # "cargo tree -i" exits 0 even when the library is absent; it then prints nothing.
      if cargo tree --locked -p "$p" -e normal --target "$t" -i "$net" 2>/dev/null | grep -q "^$net v"; then
        echo "NETWORK LIBRARY FOUND: $net in $p ($t)" >&2; exit 1
      fi
    done
  done
done
echo "ok: no networking library"

echo ">> 1/3 MCP sidecar (universal)"
scripts/build-mcp-sidecar.sh --universal

echo ">> 2/3 app (universal-apple-darwin)"
npm run tauri build -- --target universal-apple-darwin --bundles app,dmg -- --locked

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
# The MCP inside the app must be exactly the one built from source in step 1.
want="$(shasum -a 256 "$ROOT/src-tauri/binaries/hd-fantasma-mcp-universal-apple-darwin" | cut -d' ' -f1)"
got="$(shasum -a 256 "$APP/Contents/MacOS/hd-fantasma-mcp" | cut -d' ' -f1)"
[[ "$want" == "$got" ]] || { echo "MCP in the app differs from the fresh build"; fail=1; }
[[ $fail -eq 0 ]] || { echo "CHECK FAILED" >&2; exit 1; }

echo
echo "DMG: $DMG"
echo "Size: $(du -h "$DMG" | cut -f1)"
