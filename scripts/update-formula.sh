#!/usr/bin/env bash
#
# Regenerate Formula/imessage-book.rb for a release.
#
# Usage:
#   scripts/update-formula.sh <version> <sha256-aarch64> <sha256-x86_64>
#
#   version         release version, with or without a leading "v" (0.3.0 or v0.3.0)
#   sha256-aarch64  sha256 of imessage-book-<version>-aarch64-apple-darwin.tar.gz
#   sha256-x86_64   sha256 of imessage-book-<version>-x86_64-apple-darwin.tar.gz
#
# This is the single source of truth for the formula: the committed
# Formula/imessage-book.rb is exactly what this script emits for the current
# version with placeholder checksums. .github/workflows/release.yml runs it with
# the real checksums after building the release binaries, and a maintainer can
# run it by hand to reproduce the file.
set -euo pipefail

VERSION="${1:?usage: update-formula.sh <version> <sha-aarch64> <sha-x86_64>}"
SHA_ARM="${2:?missing aarch64-apple-darwin sha256}"
SHA_INTEL="${3:?missing x86_64-apple-darwin sha256}"
VERSION="${VERSION#v}"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FORMULA="$ROOT/Formula/imessage-book.rb"

# Unquoted heredoc: bash expands the $VERSION/$SHA_* shell vars below, while the
# Ruby `#{version}` / `#{bin}` interpolations have no `$` and pass through verbatim.
cat > "$FORMULA" <<RUBY
class ImessageBook < Formula
  desc "Turn an iMessage conversation into a keepsake book"
  homepage "https://github.com/Shmoopi/imessage-book"
  version "$VERSION"

  if Hardware::CPU.arm?
    url "https://github.com/Shmoopi/imessage-book/releases/download/v#{version}/imessage-book-#{version}-aarch64-apple-darwin.tar.gz"
    sha256 "$SHA_ARM"
  else
    url "https://github.com/Shmoopi/imessage-book/releases/download/v#{version}/imessage-book-#{version}-x86_64-apple-darwin.tar.gz"
    sha256 "$SHA_INTEL"
  end

  depends_on :macos

  def install
    bin.install "imessage-book"
  end

  def caveats
    <<~EOS
      imessage-book reads your iMessage database at ~/Library/Messages/chat.db.
      Grant your terminal (or whatever runs it) Full Disk Access:
        System Settings -> Privacy & Security -> Full Disk Access

      Optional companions:
        brew install tectonic   # PDF output: imessage-book build
        brew install ffmpeg     # video poster frames when embedding attachments
    EOS
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/imessage-book --version")
  end
end
RUBY

echo "Wrote $FORMULA (version $VERSION)"
