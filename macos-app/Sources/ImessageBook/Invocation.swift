import Foundation

/// A fully-resolved engine command: the argument vector, a scratch working directory, and
/// the artifact to open when it finishes.
struct ExportInvocation {
    let arguments: [String]
    let workingDirectory: URL
    let expectedResult: URL?   // nil for the preview server, which serves a page instead
    let isServer: Bool
}

enum InvocationBuilder {
    /// Translate the wizard's choices into an `imessage-book` invocation.
    ///
    /// Title/author (when provided) are written to a throwaway `book.toml` in an isolated
    /// scratch directory that we also use as the working directory — so the engine never
    /// picks up a stray `book.toml` from elsewhere, and there's nothing to clean up beyond
    /// a temp folder.
    static func build(
        recipient: String,
        source: MessageSource,
        options: ExportOptions,
        port: Int
    ) throws -> ExportInvocation {
        let fm = FileManager.default
        let scratch = fm.temporaryDirectory
            .appendingPathComponent("imessage-book-\(UUID().uuidString)", isDirectory: true)
        try fm.createDirectory(at: scratch, withIntermediateDirectories: true)
        try fm.createDirectory(at: options.outputDirectory, withIntermediateDirectories: true)

        var args = [options.format.subcommand, recipient]
        args += source.cliArguments

        // Subsetting.
        if let from = options.fromDate { args += ["--from", Self.day(from)] }
        if let to = options.toDate { args += ["--to", Self.day(to)] }
        if let limit = options.limit, limit > 0 { args += ["--limit", String(limit)] }

        // Attachments.
        args += ["--attachments", options.attachments.rawValue]
        if options.downloadFromICloud { args.append("--download-from-icloud") }
        if let mb = options.maxAttachmentMB, mb > 0 { args += ["--max-attachment-mb", String(mb)] }

        args += ["--output-dir", options.outputDirectory.path]

        // Front matter via a temp config.
        if let configURL = try Self.writeConfig(title: options.title, author: options.author, in: scratch) {
            args += ["--config", configURL.path]
        }

        if options.format.isServer {
            args += ["--port", String(port)]
        }

        let expected = options.format.outputFileName.map { options.outputDirectory.appendingPathComponent($0) }

        return ExportInvocation(
            arguments: args,
            workingDirectory: scratch,
            expectedResult: expected,
            isServer: options.format.isServer
        )
    }

    /// Write a minimal `book.toml` carrying just the title/author the user typed. Returns
    /// nil when both are blank (so the engine falls back to its defaults).
    private static func writeConfig(title: String, author: String, in dir: URL) throws -> URL? {
        let title = title.trimmingCharacters(in: .whitespacesAndNewlines)
        let author = author.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !title.isEmpty || !author.isEmpty else { return nil }

        var lines: [String] = []
        if !title.isEmpty { lines.append("title = \(tomlString(title))") }
        if !author.isEmpty { lines.append("author = \(tomlString(author))") }
        let url = dir.appendingPathComponent("book.toml")
        try (lines.joined(separator: "\n") + "\n").write(to: url, atomically: true, encoding: .utf8)
        return url
    }

    /// Quote a value as a TOML basic string, escaping backslashes and double quotes.
    private static func tomlString(_ value: String) -> String {
        let escaped = value
            .replacingOccurrences(of: "\\", with: "\\\\")
            .replacingOccurrences(of: "\"", with: "\\\"")
        return "\"\(escaped)\""
    }

    private static let dayFormatter: DateFormatter = {
        let f = DateFormatter()
        f.locale = Locale(identifier: "en_US_POSIX")
        f.dateFormat = "yyyy-MM-dd"
        return f
    }()

    private static func day(_ date: Date) -> String { dayFormatter.string(from: date) }
}
