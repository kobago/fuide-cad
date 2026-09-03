#!/usr/bin/env bash
# Build distributable .app bundles and DMGs (Apple Silicon).
#   ./scripts/release.sh              # all apps
#   ./scripts/release.sh fuide-brew     # one app
# Output: dist/<App>.app and dist/<App>.dmg
#
# Steps: cargo-bundle (.app + Info.plist + .icns from assets/icons/*.svg) -> codesign -> hdiutil DMG
# with an /Applications symlink. The signature is ad-hoc unless SIGN_IDENTITY is set to a
# "Developer ID Application: ..." identity (then also notarize the DMG with `xcrun notarytool`).
set -euo pipefail
cd "$(dirname "$0")/.."   # cargo-bundle resolves icon paths from the current directory

command -v cargo-bundle >/dev/null || { echo "cargo-bundle missing: cargo install cargo-bundle" >&2; exit 1; }
TARGET=aarch64-apple-darwin
SIGN_IDENTITY="${SIGN_IDENTITY:--}"
APPS=("$@"); [ ${#APPS[@]} -eq 0 ] && APPS=(fuide-file-manager fuide-brew fuide-player fuide-activity-monitor)
mkdir -p dist

for app in "${APPS[@]}"; do
  echo "==> $app"
  cargo bundle --release --format osx -p "$app" --target "$TARGET" | grep -v "^ *Compiling" || true
  bundle=""
  for b in "target/$TARGET/release/bundle/osx/"*.app; do
    if [ "$(defaults read "$PWD/$b/Contents/Info.plist" CFBundleExecutable)" = "$app" ]; then bundle="$b"; fi
  done
  [ -n "$bundle" ] || { echo "bundle for $app not found" >&2; exit 1; }
  name=$(basename "$bundle" .app)

  rm -rf "dist/$name.app" "dist/$name.dmg"
  cp -R "$bundle" "dist/$name.app"
  codesign --force --deep --options runtime --sign "$SIGN_IDENTITY" "dist/$name.app"
  codesign --verify --deep --strict "dist/$name.app"

  staging=$(mktemp -d)
  cp -R "dist/$name.app" "$staging/"
  ln -s /Applications "$staging/Applications"
  hdiutil create -quiet -volname "$name" -srcfolder "$staging" -ov -format UDZO "dist/$name.dmg"
  rm -rf "$staging"
  if [ "$SIGN_IDENTITY" != "-" ]; then codesign --force --sign "$SIGN_IDENTITY" "dist/$name.dmg"; fi
  echo "    dist/$name.app  dist/$name.dmg"
done

echo
ls -la dist/*.dmg
echo
if [ "$SIGN_IDENTITY" = "-" ]; then
  echo "Ad-hoc signed. On another Mac, Gatekeeper will block the first launch; the recipient runs once:"
  echo "  xattr -d com.apple.quarantine \"/Applications/<App>.app\""
  echo "or right-click > Open. Set SIGN_IDENTITY to a Developer ID and notarize to avoid this."
fi
