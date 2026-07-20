// swift-tools-version:5.9
import PackageDescription

// The GUI is a thin, native SwiftUI front end over the `imessage-book` CLI. It shells
// out to the Rust binary (bundled inside the .app by scripts/build-app.sh, or discovered
// on PATH during development) — all the real work still happens in the engine.
let package = Package(
    name: "ImessageBook",
    platforms: [.macOS(.v13)],
    targets: [
        .executableTarget(
            name: "ImessageBook",
            path: "Sources/ImessageBook"
        )
    ]
)
