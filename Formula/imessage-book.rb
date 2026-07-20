class ImessageBook < Formula
  desc "Turn an iMessage conversation into a keepsake book"
  homepage "https://github.com/Shmoopi/imessage-book"
  version "1.0.2"

  if Hardware::CPU.arm?
    url "https://github.com/Shmoopi/imessage-book/releases/download/v#{version}/imessage-book-#{version}-aarch64-apple-darwin.tar.gz"
    sha256 "0000000000000000000000000000000000000000000000000000000000000000"
  else
    url "https://github.com/Shmoopi/imessage-book/releases/download/v#{version}/imessage-book-#{version}-x86_64-apple-darwin.tar.gz"
    sha256 "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
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
