import Foundation

// MARK: - Conversation

/// One row of `imessage-book list-chats --json`. The `identifier` is what we pass back to
/// the export subcommands as the recipient.
struct Conversation: Identifiable, Decodable, Hashable {
    let identifier: String
    let displayName: String?
    let count: Int
    let first: String?
    let last: String?
    let isGroup: Bool?

    var id: String { identifier }

    enum CodingKeys: String, CodingKey {
        case identifier
        case displayName = "display_name"
        case count, first, last
        case isGroup = "is_group"
    }

    init(identifier: String, displayName: String?, count: Int,
         first: String?, last: String?, isGroup: Bool?) {
        self.identifier = identifier
        self.displayName = displayName
        self.count = count
        self.first = first
        self.last = last
        self.isGroup = isGroup
    }

    /// Friendly name if the chat has one (a group name or a Contacts match), else the raw
    /// phone/email handle.
    var title: String {
        if let name = displayName, !name.isEmpty { return name }
        return identifier
    }

    /// A secondary handle line, shown only when it differs from the title.
    var secondary: String? {
        guard let name = displayName, !name.isEmpty, name != identifier else { return nil }
        return identifier
    }

    /// "4,958 messages · 2023-01-01 – 2024-03-15"
    var summary: String {
        var parts = ["\(count.formatted()) message\(count == 1 ? "" : "s")"]
        if let a = first, let b = last {
            parts.append(a == b ? a : "\(a) – \(b)")
        }
        return parts.joined(separator: "  ·  ")
    }
}

// MARK: - Output format

/// The five things the engine can produce. Each maps to a CLI subcommand.
enum ExportFormat: String, CaseIterable, Identifiable {
    case html        // `preview` — a live local web page
    case pdf         // `build`   — print-ready PDF (needs Tectonic)
    case epub        // `epub`
    case markdown    // `markdown`
    case json        // `json`

    var id: String { rawValue }

    var subcommand: String {
        switch self {
        case .html: return "preview"
        case .pdf: return "build"
        case .epub: return "epub"
        case .markdown: return "markdown"
        case .json: return "json"
        }
    }

    var title: String {
        switch self {
        case .html: return "Web preview"
        case .pdf: return "PDF book"
        case .epub: return "EPUB"
        case .markdown: return "Markdown"
        case .json: return "JSON"
        }
    }

    var detail: String {
        switch self {
        case .html: return "Open in your browser instantly — bubbles, photos, videos, and charts. Best for a first look."
        case .pdf: return "A print-ready book you can send to a print-on-demand service. Requires Tectonic."
        case .epub: return "An e-reader edition for Books, Kindle, or Kobo."
        case .markdown: return "A plain-text document for archiving or editing."
        case .json: return "The full conversation as structured data."
        }
    }

    var symbol: String {
        switch self {
        case .html: return "safari"
        case .pdf: return "book.closed"
        case .epub: return "books.vertical"
        case .markdown: return "doc.plaintext"
        case .json: return "curlybraces"
        }
    }

    /// The file the engine writes into the output directory (nil for the live preview,
    /// which serves a page instead of leaving a single artifact to open).
    var outputFileName: String? {
        switch self {
        case .html: return nil
        case .pdf: return "book.pdf"
        case .epub: return "book.epub"
        case .markdown: return "book.md"
        case .json: return "book.json"
        }
    }

    /// The preview runs a local server that stays up until stopped; everything else writes
    /// a file and exits.
    var isServer: Bool { self == .html }

    /// Only the PDF path shells out to Tectonic / a system TeX.
    var requiresTectonic: Bool { self == .pdf }
}

// MARK: - Message source

/// Where the messages come from. Defaults to the live Mac database; advanced users can
/// point at an iOS backup or a specific `chat.db`.
enum MessageSource: Equatable {
    case liveMac
    case iosBackup(URL)
    case chatDatabase(URL)

    /// The DB-location flags to append to every engine invocation.
    var cliArguments: [String] {
        switch self {
        case .liveMac: return []
        case .iosBackup(let url): return ["--ios-backup-dir", url.path]
        case .chatDatabase(let url): return ["--chat-database", url.path]
        }
    }

    /// Full Disk Access is what unlocks the protected live database; a user-chosen backup
    /// folder or db file is reached through a normal sandboxless read.
    var requiresFullDiskAccess: Bool {
        if case .liveMac = self { return true }
        return false
    }

    var label: String {
        switch self {
        case .liveMac: return "This Mac's Messages"
        case .iosBackup(let url): return "iOS backup — \(url.lastPathComponent)"
        case .chatDatabase(let url): return "Database — \(url.lastPathComponent)"
        }
    }
}

// MARK: - Attachments

enum AttachmentsMode: String, CaseIterable, Identifiable {
    case media  // embed photos + video poster frames
    case none   // labeled placeholders only

    var id: String { rawValue }

    var title: String {
        switch self {
        case .media: return "Include photos & videos"
        case .none: return "Text only"
        }
    }
}

// MARK: - Full Disk Access

enum FullDiskAccessStatus {
    case granted
    case denied
    case noDatabase  // no ~/Library/Messages/chat.db at all
}

// MARK: - Export options

/// Everything the user configures on the options step, ready to be turned into CLI args.
struct ExportOptions {
    var format: ExportFormat = .html
    var attachments: AttachmentsMode = .media
    var downloadFromICloud = false
    var maxAttachmentMB: Int? = nil
    var fromDate: Date? = nil
    var toDate: Date? = nil
    var limit: Int? = nil
    var title: String = ""
    var author: String = ""
    var outputDirectory: URL
}
