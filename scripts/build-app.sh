#!/usr/bin/env bash
#
# Build the imessage-book macOS GUI (ImessageBook.app).
#
# Produces a self-contained .app: a SwiftUI front end (Contents/MacOS) with the Rust
# engine bundled alongside it (Contents/Resources/imessage-book). Users double-click the
# app — nothing else to install (Tectonic is still needed for PDF export).
#
# Usage:
#   scripts/build-app.sh [--open]
#
# Environment overrides:
#   BUNDLE_ID          bundle identifier            (default: com.shmoopi.imessage-book)
#   CODESIGN_IDENTITY  signing identity for codesign (default: - , i.e. ad-hoc)
#   DIST_DIR           output directory              (default: <repo>/dist)
#   UNIVERSAL          1 = universal engine (arm64 + x86_64), 0 = this Mac only (faster;
#                      handy for local iteration and CI smoke tests)   (default: 1)
#
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_PKG="$REPO_ROOT/macos-app"
DIST_DIR="${DIST_DIR:-$REPO_ROOT/dist}"
BUNDLE_ID="${BUNDLE_ID:-com.shmoopi.imessage-book}"
CODESIGN_IDENTITY="${CODESIGN_IDENTITY:--}"
APP_NAME="ImessageBook"
APP="$DIST_DIR/$APP_NAME.app"
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT

open_after=false
[[ "${1:-}" == "--open" ]] && open_after=true

log() { printf '\033[1;34m==>\033[0m %s\n' "$1"; }

# Version comes from Cargo.toml so the app and CLI stay in lockstep.
VERSION="$(grep -m1 '^version' "$REPO_ROOT/Cargo.toml" | sed -E 's/.*"([^"]+)".*/\1/')"
VERSION="${VERSION:-0.0.0}"

# --- 1. Build the Rust engine (universal when both targets are available) --------------
log "Building the imessage-book engine (release)…"
cd "$REPO_ROOT"

if [[ "${UNIVERSAL:-1}" == "0" ]]; then
    log "UNIVERSAL=0 — building for this Mac's architecture only."
    cargo build --release
    cp "$REPO_ROOT/target/release/imessage-book" "$STAGE/imessage-book"
else
    RUST_TARGETS=(aarch64-apple-darwin x86_64-apple-darwin)
    if command -v rustup >/dev/null 2>&1; then
        rustup target add "${RUST_TARGETS[@]}" >/dev/null 2>&1 || true
    fi

    built=()
    for target in "${RUST_TARGETS[@]}"; do
        if rustup target list --installed 2>/dev/null | grep -qx "$target"; then
            if cargo build --release --target "$target"; then
                built+=("$REPO_ROOT/target/$target/release/imessage-book")
            fi
        fi
    done

    if [[ ${#built[@]} -eq 2 ]]; then
        log "Creating a universal binary (arm64 + x86_64)…"
        lipo -create -output "$STAGE/imessage-book" "${built[@]}"
    elif [[ ${#built[@]} -eq 1 ]]; then
        log "Only one target available — shipping a single-architecture engine."
        cp "${built[0]}" "$STAGE/imessage-book"
    else
        log "No cross targets installed — building for this Mac's architecture only."
        cargo build --release
        cp "$REPO_ROOT/target/release/imessage-book" "$STAGE/imessage-book"
    fi
fi
chmod +x "$STAGE/imessage-book"

# --- 2. Build the Swift GUI ------------------------------------------------------------
log "Building the SwiftUI app (release)…"
swift build -c release --package-path "$APP_PKG"
SWIFT_BIN="$(swift build -c release --package-path "$APP_PKG" --show-bin-path)/$APP_NAME"
if [[ ! -x "$SWIFT_BIN" ]]; then
    echo "error: expected Swift binary at $SWIFT_BIN" >&2
    exit 1
fi

# --- 3. Assemble the .app bundle -------------------------------------------------------
log "Assembling $APP_NAME.app…"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

cp "$SWIFT_BIN" "$APP/Contents/MacOS/$APP_NAME"
cp "$STAGE/imessage-book" "$APP/Contents/Resources/imessage-book"
chmod +x "$APP/Contents/MacOS/$APP_NAME" "$APP/Contents/Resources/imessage-book"

# App icon — regenerate if it's missing, then bundle it.
ICON_SRC="$APP_PKG/AppIcon.icns"
if [[ ! -f "$ICON_SRC" ]]; then
    log "Generating app icon…"
    "$REPO_ROOT/scripts/make-icon.sh"
fi
cp "$ICON_SRC" "$APP/Contents/Resources/AppIcon.icns"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>imessage-book</string>
    <key>CFBundleDisplayName</key><string>imessage-book</string>
    <key>CFBundleExecutable</key><string>$APP_NAME</string>
    <key>CFBundleIconFile</key><string>AppIcon</string>
    <key>CFBundleIdentifier</key><string>$BUNDLE_ID</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$VERSION</string>
    <key>CFBundleVersion</key><string>$VERSION</string>
    <key>LSMinimumSystemVersion</key><string>13.0</string>
    <key>LSApplicationCategoryType</key><string>public.app-category.productivity</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSHumanReadableCopyright</key><string>Turn an iMessage conversation into a keepsake book.</string>
</dict>
</plist>
PLIST

# --- 4. Code sign ----------------------------------------------------------------------
# Sign the nested engine first, then the bundle. Ad-hoc (-) by default, which keeps Full
# Disk Access grants from resetting on each rebuild. Pass a real Developer ID via
# CODESIGN_IDENTITY for distribution: that path also enables the hardened runtime and a
# secure timestamp, which Apple requires before it will notarize the app.
log "Code signing (identity: $CODESIGN_IDENTITY)…"
if [[ "$CODESIGN_IDENTITY" == "-" ]]; then
    codesign --force --timestamp=none --sign - "$APP/Contents/Resources/imessage-book"
    codesign --force --timestamp=none --sign - "$APP"
else
    codesign --force --options runtime --timestamp \
        --sign "$CODESIGN_IDENTITY" "$APP/Contents/Resources/imessage-book"
    codesign --force --options runtime --timestamp \
        --sign "$CODESIGN_IDENTITY" "$APP"
fi
codesign --verify --strict "$APP"

log "Done: $APP"
echo "Engine architectures: $(lipo -archs "$APP/Contents/Resources/imessage-book" 2>/dev/null || echo unknown)"

if $open_after; then
    open "$APP"
fi
