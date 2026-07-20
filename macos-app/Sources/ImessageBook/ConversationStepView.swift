import SwiftUI

struct ConversationStepView: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.screenshotMode) private var screenshotMode
    @State private var searchText = ""
    @State private var selectedID: String?

    private var filtered: [Conversation] {
        let query = searchText.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        guard !query.isEmpty else { return model.conversations }
        return model.conversations.filter {
            $0.title.lowercased().contains(query) || $0.identifier.lowercased().contains(query)
        }
    }

    var body: some View {
        StepLayout(
            title: "Choose a conversation",
            subtitle: "Pick the person or group chat you'd like to turn into a book."
        ) {
            switch model.loadState {
            case .idle, .loading:
                loading
            case .failed(let message):
                Banner(.error, title: "Couldn't read your conversations", message: message) {
                    Button("Try Again") { Task { await model.loadConversations() } }
                }
            case .loaded:
                if model.conversations.isEmpty {
                    Banner(.info, title: "No conversations found",
                           message: "This database doesn't contain any chats.")
                } else {
                    list
                }
            }
        } footer: {
            Button("Back") { model.step = .source }
            Spacer()
            if case .loaded = model.loadState, !model.conversations.isEmpty {
                Text("\(filtered.count) of \(model.conversations.count)")
                    .font(.caption).foregroundStyle(.secondary)
            }
            Button("Continue") { advance() }
                .keyboardShortcut(.defaultAction)
                .controlSize(.large)
                .buttonStyle(.borderedProminent)
                .disabled(selectedID == nil)
        }
    }

    private var loading: some View {
        HStack(spacing: 10) {
            ProgressView()
            Text("Reading conversations…").foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, minHeight: 200)
    }

    private var list: some View {
        VStack(alignment: .leading, spacing: 10) {
            if screenshotMode {
                FauxSearchField(placeholder: "Search by name, phone, or email")
            } else {
                TextField("Search by name, phone, or email", text: $searchText)
                    .textFieldStyle(.roundedBorder)
            }

            if screenshotMode {
                staticList
            } else {
                List(selection: $selectedID) {
                    ForEach(filtered) { conversation in
                        ConversationRow(conversation: conversation)
                            .tag(conversation.id)
                            .contentShape(Rectangle())
                            .onTapGesture { selectedID = conversation.id }
                            .simultaneousGesture(TapGesture(count: 2).onEnded { advance() })
                    }
                }
                .frame(minHeight: 320)
                .listStyle(.inset(alternatesRowBackgrounds: true))
            }
        }
    }

    /// A non-`List` facsimile (the real `ConversationRow`s) so `ImageRenderer` can capture
    /// the picker for documentation. Highlights the model's current selection.
    private var staticList: some View {
        VStack(spacing: 0) {
            ForEach(Array(filtered.enumerated()), id: \.element.id) { index, conversation in
                if index > 0 { Divider().opacity(0.4) }
                ConversationRow(conversation: conversation)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 3)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(conversation.id == model.selection?.id
                                ? Color.accentColor.opacity(0.15) : Color.clear)
            }
        }
        .padding(.vertical, 4)
        .background(Color.primary.opacity(0.035), in: RoundedRectangle(cornerRadius: 10))
    }

    private func advance() {
        guard let id = selectedID,
              let conversation = model.conversations.first(where: { $0.id == id }) else { return }
        model.choose(conversation)
    }
}

private struct ConversationRow: View {
    let conversation: Conversation

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: symbol)
                .font(.title2)
                .foregroundStyle(conversation.isGroup == true ? Color.accentColor : .secondary)
                .frame(width: 28)
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 7) {
                    Text(conversation.title).fontWeight(.medium)
                    if conversation.isGroup == true { Badge(text: "Group", tint: .accentColor) }
                }
                if let secondary = conversation.secondary {
                    Text(secondary).font(.caption).foregroundStyle(.secondary)
                }
                Text(conversation.summary).font(.caption).foregroundStyle(.tertiary)
            }
            Spacer()
        }
        .padding(.vertical, 5)
    }

    private var symbol: String {
        conversation.isGroup == true ? "person.2.crop.square.stack.fill" : "person.crop.circle.fill"
    }
}
