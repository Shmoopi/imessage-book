import Foundation
import SwiftUI

/// The four steps of the guided flow.
enum WizardStep: Int, CaseIterable, Identifiable {
    case source, conversation, options, run
    var id: Int { rawValue }

    var title: String {
        switch self {
        case .source: return "Source"
        case .conversation: return "Conversation"
        case .options: return "Options"
        case .run: return "Export"
        }
    }
}

enum LoadState: Equatable {
    case idle
    case loading
    case loaded
    case failed(String)
}

/// Central state for the wizard. Owns the child `RunController` and the current step.
@MainActor
final class AppModel: ObservableObject {
    static let previewPort = 8000

    @Published var step: WizardStep = .source

    // Environment discovered at launch (and re-checked when the user returns to step 1).
    @Published var binaryURL: URL?
    @Published var fullDiskAccess: FullDiskAccessStatus = .noDatabase
    @Published var hasPDFEngine = true

    // Source + conversation list.
    @Published var source: MessageSource = .liveMac
    @Published var conversations: [Conversation] = []
    @Published var loadState: LoadState = .idle
    @Published var selection: Conversation?

    // Options + run.
    @Published var options: ExportOptions
    @Published var run = RunController()

    var engineAvailable: Bool { binaryURL != nil }

    init() {
        let documents = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask).first
            ?? FileManager.default.homeDirectoryForCurrentUser
        options = ExportOptions(outputDirectory: documents.appendingPathComponent("imessage-book", isDirectory: true))
        refreshEnvironment()
    }

    /// Re-check the binary, Full Disk Access, and PDF-engine availability.
    func refreshEnvironment() {
        binaryURL = Engine.locateBinary()
        fullDiskAccess = Engine.fullDiskAccessStatus()
        hasPDFEngine = Engine.hasPDFEngine()
    }

    /// Whether the current source is ready to browse conversations.
    var sourceReady: Bool {
        guard engineAvailable else { return false }
        if source.requiresFullDiskAccess { return fullDiskAccess == .granted }
        return true
    }

    // MARK: - Navigation

    func goToConversations() {
        step = .conversation
        Task { await loadConversations() }
    }

    func loadConversations() async {
        loadState = .loading
        conversations = []
        do {
            let list = try await Engine.listConversations(source: source)
            conversations = list
            loadState = .loaded
        } catch {
            loadState = .failed(error.localizedDescription)
        }
    }

    func choose(_ conversation: Conversation) {
        selection = conversation
        if options.title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            options.title = conversation.title
        }
        options.outputDirectory = defaultOutputDirectory(for: conversation)
        step = .options
    }

    /// `~/Documents/imessage-book/<safe name>` — a tidy per-conversation folder.
    private func defaultOutputDirectory(for conversation: Conversation) -> URL {
        let base = options.outputDirectory.deletingLastPathComponent()
            .appendingPathComponent("imessage-book", isDirectory: true)
        return base.appendingPathComponent(Self.sanitize(conversation.title), isDirectory: true)
    }

    static func sanitize(_ name: String) -> String {
        let invalid = CharacterSet(charactersIn: "/\\:?%*|\"<>")
        let cleaned = name.components(separatedBy: invalid).joined(separator: "-")
            .trimmingCharacters(in: .whitespacesAndNewlines)
        return cleaned.isEmpty ? "conversation" : cleaned
    }

    // MARK: - Export

    func startExport() {
        guard let binary = binaryURL, let conversation = selection else { return }
        step = .run
        do {
            let invocation = try InvocationBuilder.build(
                recipient: conversation.identifier,
                source: source,
                options: options,
                port: Self.previewPort
            )
            run.start(binary: binary, invocation: invocation)
        } catch {
            // Surface directory/config-writing failures in the run view.
            run.fail("Couldn't prepare the export: \(error.localizedDescription)")
        }
    }

    /// Return to the start for another export, stopping anything in flight.
    func startOver() {
        run.stop()
        run.reset()
        selection = nil
        step = .source
        refreshEnvironment()
    }

    func backToOptions() {
        run.stop()
        run.reset()
        step = .options
    }
}
