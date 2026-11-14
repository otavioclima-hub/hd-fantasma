#!/usr/bin/env bash
# Installs the installer from instalador/ into /Applications (the only copy of the app that runs).
# The old app goes to the Trash. Checks that the installed app is identical to the one in the DMG.
# Usage: scripts/install-mac.sh
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="/Applications/HD Fantasma.app"

DMGS=("$ROOT"/instalador/*.dmg)
[[ -e "${DMGS[0]}" ]] || { echo "No installer in instalador/. Run scripts/build-release-mac.sh first." >&2; exit 1; }
[[ ${#DMGS[@]} -eq 1 ]] || { echo "More than one installer in instalador/." >&2; exit 1; }
DMG="${DMGS[0]}"
echo "installer: $(basename "$DMG")"

# Quit the app (the Claude connection processes keep running and pick up the new app later).
if pgrep -f "MacOS/hd-fantasma$" >/dev/null; then
  osascript -e 'tell application "HD Fantasma" to quit' || true
  for _ in 1 2 3 4 5 6 7 8 9 10; do pgrep -f "MacOS/hd-fantasma$" >/dev/null || break; sleep 1; done
  pgrep -f "MacOS/hd-fantasma$" >/dev/null && { echo "HD Fantasma is still open; quit it and try again." >&2; exit 1; }
fi

MNT="$(mktemp -d)"
hdiutil attach -nobrowse -readonly -mountpoint "$MNT" "$DMG" >/dev/null
trap 'hdiutil detach "$MNT" >/dev/null 2>&1 || true; rmdir "$MNT" 2>/dev/null || true' EXIT
SRC="$MNT/HD Fantasma.app"
[[ -d "$SRC" ]] || { echo "The installer has no HD Fantasma.app." >&2; exit 1; }

if [[ -e "$DEST" ]]; then
  mv "$DEST" "$HOME/.Trash/HD Fantasma (old $(date +%Y-%m-%d_%H%M%S)).app"
fi
ditto "$SRC" "$DEST"

# Same files, byte for byte, as the app inside the installer.
diff -rq "$SRC" "$DEST" >/dev/null || { echo "INSTALLED APP DIFFERS FROM THE INSTALLER" >&2; exit 1; }
codesign --verify --deep --strict "$DEST" || { echo "SIGNATURE CHECK FAILED" >&2; exit 1; }
echo "installed: $DEST"
codesign -dr - "$DEST" 2>&1 | tail -1
