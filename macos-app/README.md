# imessage-book — macOS app

A native SwiftUI front end that lets non-technical users turn an iMessage conversation
into a book without touching the command line. It's a thin GUI over the `imessage-book`
Rust engine — all the real work still happens there.

## What it does

A four-step guided flow:

1. **Source** — reads this Mac's Messages database (with a Full Disk Access check and a
   one-click jump to the right System Settings pane), or an iOS backup / specific
   `chat.db` for advanced users.
2. **Conversation** — a searchable list of every chat (name, handle, message count, date
   span), powered by `imessage-book list-chats --json`.
3. **Options** — pick a format (Web preview, PDF, EPUB, Markdown, JSON), toggle photos &
   videos, set a date range, give the book a title/author, and choose where to save.
4. **Export** — streams live progress. File formats open automatically and can be revealed
   in Finder; the Web preview runs a local server you stop when you're done.

## Architecture

```
ImessageBook.app/
  Contents/
    MacOS/ImessageBook          # the SwiftUI GUI
    Resources/imessage-book     # the Rust engine, bundled (universal on release)
    Info.plist
```

The GUI never re-implements any logic — it discovers the engine (bundled copy first, then
Homebrew / `~/.cargo/bin` / `PATH`), shells out to it, and streams stdout/stderr. Because
GUI apps launched from Finder don't inherit Homebrew's `PATH`, the app augments `PATH` so
`tectonic` (PDF), `sips`, and `ffmpeg` are found.

Key files under `Sources/ImessageBook/`:

| File | Responsibility |
|------|----------------|
| `Engine.swift` | Locate the binary, augment `PATH`, Full Disk Access check, `list-chats --json`. |
| `Invocation.swift` | Turn wizard choices into an argument vector + temp `book.toml`. |
| `RunController.swift` | Spawn the engine and stream output into live state. |
| `AppModel.swift` | Wizard state machine. |
| `*StepView.swift` | The four steps. |

## Build

From the repository root:

```sh
scripts/build-app.sh          # builds dist/ImessageBook.app
scripts/build-app.sh --open   # ...and launches it
```

The script builds a **universal** engine (arm64 + x86_64) when both Rust targets are
installed (`rustup target add aarch64-apple-darwin x86_64-apple-darwin`), otherwise it
falls back to your Mac's architecture. It then compiles the Swift app, assembles the
bundle, and ad-hoc code-signs it.

## Develop

Iterate on the Swift code without rebuilding the whole bundle:

```sh
cargo build --release                      # so the engine exists at target/release/
swift run --package-path macos-app         # runs the GUI, discovering that engine
```

Point the app at a specific engine build with an environment override:

```sh
IMESSAGE_BOOK_BIN=/path/to/imessage-book swift run --package-path macos-app
```

> When running via `swift run`, Full Disk Access is attributed to your **terminal**, not
> the app — so grant Terminal FDA for development. The shipped `.app` asks for its own
> grant.

## Assets (icon & screenshots)

Both are generated from code, so they're reproducible and reviewable:

```sh
scripts/make-icon.sh                 # draws macos-app/AppIcon.icns (+ docs/screenshots/icon.png)
IMB_SHOTS_DIR=docs/screenshots \
  dist/ImessageBook.app/Contents/MacOS/ImessageBook   # renders the README screenshots
```

- The icon is drawn with CoreGraphics in [`scripts/make-icon.swift`](../scripts/make-icon.swift);
  `build-app.sh` bundles the resulting `.icns` and sets `CFBundleIconFile`.
- The README screenshots are rendered from the **real** SwiftUI views via `ImageRenderer`
  ([`Screenshots.swift`](Sources/ImessageBook/Screenshots.swift)) using synthetic sample
  data — triggered by the `IMB_SHOTS_DIR` environment variable, which makes the app render
  the four steps and quit.

## Requirements

- macOS 13 (Ventura) or later.
- Full Disk Access for reading this Mac's live Messages database.
- [Tectonic](https://tectonic-typesetting.github.io/) (`brew install tectonic`) only for
  PDF export. Web preview, EPUB, Markdown, and JSON need no extra tools.
