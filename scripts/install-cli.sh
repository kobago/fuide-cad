#!/usr/bin/env bash
# Install terminal launchers `ffm [DIR]` (FUIDE File Manager) and `fuide-brew`, like `open`.
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

cat > "$dest/ffm" <<'SH'
#!/bin/sh
# ffm [DIR] — open FUIDE File Manager at DIR (default: current directory)
# ffm --mcp  — stdio MCP bridge to the running app (for `claude mcp add ffm -- ffm --mcp`)
app="FUIDE File Manager"
if [ "${1:-}" = "--mcp" ]; then
  for d in /Applications "$HOME/Applications"; do
    bin="$d/$app.app/Contents/MacOS/fuide-file-manager"
    [ -x "$bin" ] && exec "$bin" --mcp
  done
  echo "ffm: $app.app not found in /Applications or ~/Applications" >&2; exit 1
fi
dir="${1:-.}"
[ -d "$dir" ] || { echo "ffm: not a directory: $dir" >&2; exit 1; }
abs=$(cd "$dir" && pwd -P)
exec open -na "$app" --args "$abs"
SH

cat > "$dest/fuide-brew" <<'SH'
#!/bin/sh
# fuide-brew       — open FUIDE Brew
# fuide-brew --mcp — stdio MCP bridge to the running app (for `claude mcp add fuide-brew -- fuide-brew --mcp`)
app="FUIDE Brew"
if [ "${1:-}" = "--mcp" ]; then
  for d in /Applications "$HOME/Applications"; do
    bin="$d/$app.app/Contents/MacOS/fuide-brew"
    [ -x "$bin" ] && exec "$bin" --mcp
  done
  echo "fuide-brew: $app.app not found in /Applications or ~/Applications" >&2; exit 1
fi
exec open -a "$app"
SH

chmod +x "$dest/ffm" "$dest/fuide-brew"
echo "installed: $dest/ffm  $dest/fuide-brew"
case ":$PATH:" in
  *":$dest:"*) ;;
  *) echo "note: $dest is not on PATH — add to ~/.zshrc:  export PATH=\"$dest:\$PATH\"" ;;
esac
echo "requires the apps in /Applications (drag them from the DMGs)."
