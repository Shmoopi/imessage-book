import Foundation
import AppKit

enum EngineError: LocalizedError {
    case binaryNotFound
    case listFailed(String)

    var errorDescription: String? {
        switch self {
        case .binaryNotFound:
            return "Couldn't find the imessage-book engine. Reinstall the app, or set the "
                + "IMESSAGE_BOOK_BIN environment variable to its path."
        case .listFailed(let message):
            return message
        }
    }
}

/// Locating and driving the `imessage-book` binary. Stateless helpers; a running export is
/// managed by `RunController`.
enum Engine {
    /// GUI apps launched from Finder inherit launchd's minimal PATH (`/usr/bin:/bin:…`),
    /// which omits Homebrew — so `tectonic`, `ffmpeg`, and a Homebrew-installed engine are
    /// invisible unless we add their directories back.
    static func augmentedEnvironment() -> [String: String] {
        var env = ProcessInfo.processInfo.environment
        let home = NSHomeDirectory()
        let extras = [
            "/opt/homebrew/bin", "/opt/homebrew/sbin",
            "/usr/local/bin", "/usr/local/sbin",
            "\(home)/.cargo/bin",
            "/usr/bin", "/bin", "/usr/sbin", "/sbin",
        ]
        let current = (env["PATH"] ?? "").split(separator: ":").map(String.init)
        var seen = Set<String>()
        let merged = (extras + current).filter { seen.insert($0).inserted }
        env["PATH"] = merged.joined(separator: ":")
        return env
    }

    /// Where the engine lives. Preference order: an explicit override, the copy bundled in
    /// the .app, common install locations, a dev build under `target/release`, then PATH.
    static func locateBinary() -> URL? {
        let fm = FileManager.default

        if let override = ProcessInfo.processInfo.environment["IMESSAGE_BOOK_BIN"], !override.isEmpty {
            let url = URL(fileURLWithPath: (override as NSString).expandingTildeInPath)
            if fm.isExecutableFile(atPath: url.path) { return url }
        }

        if let bundled = Bundle.main.url(forResource: "imessage-book", withExtension: nil),
           fm.isExecutableFile(atPath: bundled.path) {
            return bundled
        }

        let home = NSHomeDirectory()
        let cwd = fm.currentDirectoryPath
        let candidates = [
            "/opt/homebrew/bin/imessage-book",
            "/usr/local/bin/imessage-book",
            "\(home)/.cargo/bin/imessage-book",
            "\(cwd)/target/release/imessage-book",
            "\(cwd)/../target/release/imessage-book",
        ]
        for path in candidates where fm.isExecutableFile(atPath: path) {
            return URL(fileURLWithPath: path)
        }

        return which("imessage-book")
    }

    /// Resolve a command on PATH (using our augmented PATH so Homebrew tools are found).
    static func which(_ name: String) -> URL? {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = ["which", name]
        process.environment = augmentedEnvironment()
        let out = Pipe()
        process.standardOutput = out
        process.standardError = Pipe()
        do { try process.run() } catch { return nil }
        let data = out.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        guard process.terminationStatus == 0 else { return nil }
        let path = String(decoding: data, as: UTF8.self).trimmingCharacters(in: .whitespacesAndNewlines)
        return path.isEmpty ? nil : URL(fileURLWithPath: path)
    }

    static func hasTool(_ name: String) -> Bool { which(name) != nil }

    /// True when a PDF engine (Tectonic, or a system TeX) is available.
    static func hasPDFEngine() -> Bool {
        hasTool("tectonic") || hasTool("latexmk") || hasTool("xelatex")
    }

    // MARK: Full Disk Access

    /// Whether we can actually read the protected live Messages database. We test by
    /// opening it and reading a byte — the only reliable signal for a TCC grant.
    static func fullDiskAccessStatus() -> FullDiskAccessStatus {
        let path = NSHomeDirectory() + "/Library/Messages/chat.db"
        guard FileManager.default.fileExists(atPath: path) else { return .noDatabase }
        guard let handle = try? FileHandle(forReadingFrom: URL(fileURLWithPath: path)) else {
            return .denied
        }
        defer { try? handle.close() }
        if (try? handle.read(upToCount: 1)) != nil { return .granted }
        return .denied
    }

    static func openFullDiskAccessSettings() {
        let url = URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles")
        if let url { NSWorkspace.shared.open(url) }
    }

    // MARK: Listing conversations

    /// Run `list-chats --json` and decode it. Drains stdout to completion on a background
    /// queue (so a large list can't deadlock against a full pipe buffer).
    static func listConversations(source: MessageSource) async throws -> [Conversation] {
        guard let binary = locateBinary() else { throw EngineError.binaryNotFound }
        return try await withCheckedThrowingContinuation { continuation in
            DispatchQueue.global(qos: .userInitiated).async {
                let process = Process()
                process.executableURL = binary
                process.arguments = ["list-chats", "--json"] + source.cliArguments
                process.environment = augmentedEnvironment()
                let out = Pipe()
                let err = Pipe()
                process.standardOutput = out
                process.standardError = err
                do {
                    try process.run()
                } catch {
                    continuation.resume(throwing: error)
                    return
                }
                let outData = out.fileHandleForReading.readDataToEndOfFile()
                let errData = err.fileHandleForReading.readDataToEndOfFile()
                process.waitUntilExit()

                if process.terminationStatus != 0 {
                    let message = String(decoding: errData, as: UTF8.self)
                        .trimmingCharacters(in: .whitespacesAndNewlines)
                    continuation.resume(throwing: EngineError.listFailed(
                        message.isEmpty
                            ? "The engine exited with code \(process.terminationStatus)."
                            : message))
                    return
                }
                do {
                    let conversations = try JSONDecoder().decode([Conversation].self, from: outData)
                    continuation.resume(returning: conversations)
                } catch {
                    continuation.resume(throwing: EngineError.listFailed(
                        "Couldn't read the conversation list from the engine."))
                }
            }
        }
    }
}
