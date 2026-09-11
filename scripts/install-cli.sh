#!/usr/bin/env bash
# Install the terminal launcher `fuide-cad [FILE]` (FUIDE CAD), like `open`.
#   ./scripts/install-cli.sh            # into /opt/homebrew/bin if writable, else ~/.local/bin
#   ./scripts/install-cli.sh ~/bin      # explicit directory
# The launchers use `open -na`, so the app starts detached via LaunchServices (Dock icon,
# Spotlight-equivalent), and relative paths are resolved against the terminal's cwd first.
set -euo pipefail

dest="${1:-}"
if [ -z "$dest" ]; then
  if [ -w /opt/homebrew/bin ]; then dest=/opt/homebrew/bin; else dest="$HOME/.local/bin"; fi
fi
mkdir -p "$dest"

cat > "$dest/fuide-cad" <<'SH'
#!/bin/sh
# fuide-cad [FILE] — open FUIDE CAD (optionally with a .cad.json document)
# fuide-cad --mcp  — stdio MCP bridge to the running app (for `claude mcp add fuide-cad -- fuide-cad --mcp`)
app="FUIDE CAD"
if [ "${1:-}" = "--mcp" ]; then
  for d in /Applications "$HOME/Applications"; do
    bin="$d/$app.app/Contents/MacOS/fuide-cad"
    [ -x "$bin" ] && exec "$bin" --mcp
  done
  echo "fuide-cad: $app.app not found in /Applications or ~/Applications" >&2; exit 1
fi
if [ $# -eq 0 ]; then exec open -na "$app"; fi
f="$1"
[ -e "$f" ] || { echo "fuide-cad: not found: $f" >&2; exit 1; }
abs=$(cd "$(dirname "$f")" && pwd -P)/$(basename "$f")
exec open -na "$app" --args "$abs"
SH

chmod +x "$dest/fuide-cad"
echo "installed: $dest/fuide-cad"
case ":$PATH:" in
  *":$dest:"*) ;;
  *) echo "note: $dest is not on PATH — add to ~/.zshrc:  export PATH=\"$dest:\$PATH\"" ;;
esac
echo "requires the apps in /Applications (drag it from the DMG)."
