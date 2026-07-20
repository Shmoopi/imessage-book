import SwiftUI
import AppKit

struct SourceStepView: View {
    @EnvironmentObject private var model: AppModel
    @State private var showingAdvanced = false

    var body: some View {
        StepLayout(
            title: "Let's make a book",
            subtitle: "imessage-book turns a conversation into a keepsake — a web page, a PDF, an EPUB, and more. Everything happens on this Mac; nothing is ever uploaded."
        ) {
            if !model.engineAvailable {
                engineMissing
            }

            VStack(alignment: .leading, spacing: 10) {
                SectionLabel("Messages from")
                SelectableCard(
                    symbol: "menubar.dock.rectangle",
                    title: "This Mac's Messages",
                    subtitle: "Your Messages app on this computer",
                    selected: isLiveMac
                ) { model.source = .liveMac }

                DisclosureGroup("Use an iPhone backup or another database", isExpanded: $showingAdvanced) {
                    VStack(alignment: .leading, spacing: 10) {
                        SelectableCard(
                            symbol: "iphone",
                            title: backupTitle,
                            subtitle: "The folder of an unencrypted iPhone or iPad backup",
                            selected: isBackup
                        ) { chooseBackupFolder() }

                        SelectableCard(
                            symbol: "cylinder.split.1x2",
                            title: databaseTitle,
                            subtitle: "A chat.db file copied from another Mac or backup",
                            selected: isDatabase
                        ) { chooseDatabaseFile() }
                    }
                    .padding(.top, 8)
                }
                .tint(.secondary)
                .padding(.top, 2)
            }

            if case .liveMac = model.source {
                fullDiskAccessStatus
            }
        } footer: {
            Spacer()
            Button("Continue") { model.goToConversations() }
                .keyboardShortcut(.defaultAction)
                .controlSize(.large)
                .buttonStyle(.borderedProminent)
                .disabled(!model.sourceReady)
        }
    }

    // MARK: Pieces

    private var engineMissing: some View {
        Banner(.error,
               title: "Engine not found",
               message: "The imessage-book engine is missing from this app. Reinstall it, or set IMESSAGE_BOOK_BIN to the binary's path.") {
            Button("Re-check") { model.refreshEnvironment() }
        }
    }

    @ViewBuilder private var fullDiskAccessStatus: some View {
        switch model.fullDiskAccess {
        case .granted:
            Banner(.success, title: "Ready to read your messages",
                   message: "This app has permission to open your Messages history.")
        case .denied:
            Banner(.warning,
                   title: "One quick permission",
                   message: "macOS keeps your Messages private, so you'll grant this app Full Disk Access once. Click below, flip the switch next to imessage-book, then come back and re-check.") {
                VStack(spacing: 8) {
                    Button("Open Settings…") { Engine.openFullDiskAccessSettings() }
                        .buttonStyle(.borderedProminent)
                    Button("Re-check") { model.refreshEnvironment() }
                }
            }
        case .noDatabase:
            Banner(.info,
                   title: "No Messages found on this Mac",
                   message: "There's no Messages history here. To export from an iPhone, choose a backup above.")
        }
    }

    // MARK: State helpers

    private var isLiveMac: Bool { if case .liveMac = model.source { return true } else { return false } }
    private var isBackup: Bool { if case .iosBackup = model.source { return true } else { return false } }
    private var isDatabase: Bool { if case .chatDatabase = model.source { return true } else { return false } }

    private var backupTitle: String {
        if case .iosBackup(let url) = model.source { return "iPhone backup — \(url.lastPathComponent)" }
        return "Choose an iPhone backup folder…"
    }
    private var databaseTitle: String {
        if case .chatDatabase(let url) = model.source { return "Database — \(url.lastPathComponent)" }
        return "Choose a chat.db file…"
    }

    // MARK: Pickers

    private func chooseBackupFolder() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        panel.prompt = "Use Backup"
        panel.message = "Select the root folder of an unencrypted iOS backup."
        if panel.runModal() == .OK, let url = panel.url {
            model.source = .iosBackup(url)
        }
    }

    private func chooseDatabaseFile() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = false
        panel.canChooseFiles = true
        panel.allowsMultipleSelection = false
        panel.prompt = "Use Database"
        panel.message = "Select a chat.db file."
        if panel.runModal() == .OK, let url = panel.url {
            model.source = .chatDatabase(url)
        }
    }
}
