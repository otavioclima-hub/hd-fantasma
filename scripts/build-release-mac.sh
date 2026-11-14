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

# Local signing (optional): with a "HD Fantasma Local" code-signing certificate in the keychain,
# every build has the same identity, so macOS keeps Full Disk Access between versions.
# Without it the app is signed ad hoc and each new build must be authorized again.
if [[ -z "${APPLE_SIGNING_IDENTITY:-}" ]]; then
  LOCAL_ID="$(security find-identity -p codesigning 2>/dev/null | awk '/"HD Fantasma Local"/ {print $2; exit}')"
  if [[ -n "$LOCAL_ID" ]]; then
    export APPLE_SIGNING_IDENTITY="$LOCAL_ID"
    echo "signing with local certificate HD Fantasma Local ($LOCAL_ID)"
  else
    echo "no HD Fantasma Local certificate: ad hoc signing"
  fi
fi

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
# A signed app carries a new signature, so then only the code itself (__TEXT,__text) is compared.
FRESH_MCP="$ROOT/src-tauri/binaries/hd-fantasma-mcp-universal-apple-darwin"
code_hash() { for a in arm64 x86_64; do otool -X -arch "$a" -s __TEXT __text "$1"; done | shasum -a 256 | cut -d' ' -f1; }
if [[ -n "${APPLE_SIGNING_IDENTITY:-}" ]]; then
  want="$(code_hash "$FRESH_MCP")"; got="$(code_hash "$APP/Contents/MacOS/hd-fantasma-mcp")"
else
  want="$(shasum -a 256 "$FRESH_MCP" | cut -d' ' -f1)"; got="$(shasum -a 256 "$APP/Contents/MacOS/hd-fantasma-mcp" | cut -d' ' -f1)"
fi
[[ "$want" == "$got" ]] || { echo "MCP in the app differs from the fresh build"; fail=1; }
if [[ -n "${APPLE_SIGNING_IDENTITY:-}" ]]; then
  codesign --verify --deep --strict "$APP" || { echo "SIGNATURE CHECK FAILED"; fail=1; }
  codesign -dr - "$APP" 2>&1 | grep -q "certificate leaf" || { echo "NOT signed with the local certificate"; fail=1; }
fi
[[ $fail -eq 0 ]] || { echo "CHECK FAILED" >&2; exit 1; }

echo
echo "DMG: $DMG"
echo "Size: $(du -h "$DMG" | cut -f1)"
