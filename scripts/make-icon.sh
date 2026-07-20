#!/usr/bin/env bash
#
# Render the app icon and assemble macos-app/AppIcon.icns (plus a PNG for the README).
#
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ICONSET="$(mktemp -d)/AppIcon.iconset"
ICNS="$REPO_ROOT/macos-app/AppIcon.icns"
DOCS_DIR="$REPO_ROOT/docs/screenshots"

swift "$REPO_ROOT/scripts/make-icon.swift" "$ICONSET"
iconutil -c icns "$ICONSET" -o "$ICNS"
echo "==> Wrote $ICNS"

# A standalone PNG so the icon can be shown in the README (GitHub can't render .icns).
mkdir -p "$DOCS_DIR"
sips -s format png -Z 256 "$ICONSET/icon_512x512.png" --out "$DOCS_DIR/icon.png" >/dev/null
echo "==> Wrote $DOCS_DIR/icon.png"

rm -rf "$(dirname "$ICONSET")"
