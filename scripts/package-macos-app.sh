#!/usr/bin/env bash
#
# Package dist/ImessageBook.app into a drag-to-Applications .dmg and a .zip (each with a
# .sha256). When NOTARIZE=1, the app and the disk image are submitted to Apple's notary
# service and stapled, so Gatekeeper opens them without a warning.
#
# Run scripts/build-app.sh first (it produces and signs the .app).
#
# Environment:
#   VERSION        version string for the artifact names                       (required)
#   DIST_DIR       directory holding ImessageBook.app / receiving the output   (default: <repo>/dist)
#   NOTARIZE       1 = notarize + staple with Apple, 0 = skip                   (default: 0)
#   NOTARY_KEY     path to an App Store Connect API key (.p8)   [required when NOTARIZE=1]
#   NOTARY_KEY_ID  the API key's Key ID                         [required when NOTARIZE=1]
#   NOTARY_ISSUER  the API key's Issuer ID                      [required when NOTARIZE=1]
#
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST_DIR="${DIST_DIR:-$REPO_ROOT/dist}"
APP="$DIST_DIR/ImessageBook.app"
NOTARIZE="${NOTARIZE:-0}"
: "${VERSION:?set VERSION (e.g. VERSION=0.4.0)}"

log() { printf '\033[1;34m==>\033[0m %s\n' "$1"; }

[[ -d "$APP" ]] || {
    echo "error: $APP not found — run scripts/build-app.sh first." >&2
    exit 1
}

submit() { # $1 = path to a .zip or .dmg to notarize
    xcrun notarytool submit "$1" \
        --key "$NOTARY_KEY" --key-id "$NOTARY_KEY_ID" --issuer "$NOTARY_ISSUER" \
        --wait
}

# Notarize the app itself (submit a zip of it), then staple the ticket into the .app so it
# verifies offline. Done before the .dmg/.zip are built so both carry the stapled app.
if [[ "$NOTARIZE" == "1" ]]; then
    : "${NOTARY_KEY:?}" "${NOTARY_KEY_ID:?}" "${NOTARY_ISSUER:?}"
    log "Notarizing the app…"
    appzip="$(mktemp -d)/ImessageBook.app.zip"
    ditto -c -k --keepParent "$APP" "$appzip"
    submit "$appzip"
    xcrun stapler staple "$APP"
    log "App notarized and stapled."
fi

# Drag-to-Applications disk image.
dmg="$DIST_DIR/imessage-book-${VERSION}-macos.dmg"
log "Building $(basename "$dmg")…"
stage="$(mktemp -d)"
cp -R "$APP" "$stage/"
ln -s /Applications "$stage/Applications"
hdiutil create -volname "imessage-book ${VERSION}" -srcfolder "$stage" -ov -format UDZO "$dmg"

if [[ "$NOTARIZE" == "1" ]]; then
    log "Notarizing the disk image…"
    submit "$dmg"
    xcrun stapler staple "$dmg"
fi

# Zip alternative (ditto preserves the signature and any stapled ticket).
zip="$DIST_DIR/imessage-book-${VERSION}-macos.zip"
log "Building $(basename "$zip")…"
ditto -c -k --keepParent "$APP" "$zip"

for f in "$dmg" "$zip"; do
    sha="$(shasum -a 256 "$f" | awk '{print $1}')"
    echo "${sha}  $(basename "$f")" > "${f}.sha256"
    echo "  $(basename "$f")  sha256=${sha}"
    if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
        {
            echo "### $(basename "$f")"
            echo "- sha256: \`${sha}\`"
        } >> "$GITHUB_STEP_SUMMARY"
    fi
done

log "Done."
