import SwiftUI
import AppKit

struct OptionsStepView: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.screenshotMode) private var screenshotMode

    @State private var fromEnabled = false
    @State private var toEnabled = false
    @State private var fromDate = Date()
    @State private var toDate = Date()
    @State private var limitEnabled = false
    @State private var limitValue = 500
    @State private var maxMBEnabled = false
    @State private var maxMBValue = 25
    @State private var showAdvanced = false

    var body: some View {
        StepLayout(
            title: model.selection.map { "Export “\($0.title)”" } ?? "Options",
            subtitle: "Choose a format and fine-tune what goes in the book."
        ) {
            formatSection
            attachmentsSection
            dateRangeSection
            frontMatterSection
            outputSection
            advancedSection
        } footer: {
            Button("Back") { model.step = .conversation }
            Spacer()
            Button(exportButtonTitle) { export() }
                .keyboardShortcut(.defaultAction)
                .controlSize(.large)
                .buttonStyle(.borderedProminent)
                .disabled(!model.engineAvailable)
        }
        .onAppear(perform: seedDates)
    }

    private var exportButtonTitle: String {
        model.options.format == .html ? "Start Preview" : "Create \(model.options.format.title)"
    }

    // MARK: Format

    private var formatSection: some View {
        VStack(alignment: .leading, spacing: 8) {
            SectionLabel("Format")
            ForEach(ExportFormat.allCases) { format in
                SelectableCard(
                    symbol: format.symbol,
                    title: format.title,
                    badge: (format.requiresTectonic && !model.hasPDFEngine) ? "needs Tectonic" : nil,
                    subtitle: format.detail,
                    selected: model.options.format == format
                ) { model.options.format = format }
            }
            if model.options.format == .pdf && !model.hasPDFEngine {
                Banner(.warning,
                       title: "PDF needs Tectonic",
                       message: "Install it with `brew install tectonic`, then come back — or pick Web preview or EPUB, which work right now.")
            }
        }
    }

    // MARK: Attachments

    private var attachmentsSection: some View {
        VStack(alignment: .leading, spacing: 8) {
            SectionLabel("Photos & videos")
            if screenshotMode {
                FauxSegmented(options: AttachmentsMode.allCases.map(\.title),
                              selected: AttachmentsMode.allCases.firstIndex(of: model.options.attachments) ?? 0)
            } else {
                Picker("", selection: $model.options.attachments) {
                    ForEach(AttachmentsMode.allCases) { mode in
                        Text(mode.title).tag(mode)
                    }
                }
                .pickerStyle(.segmented)
                .labelsHidden()
            }

            if case .liveMac = model.source, model.options.attachments == .media {
                if screenshotMode {
                    FauxCheckbox(label: "Download offloaded attachments from iCloud",
                                 on: model.options.downloadFromICloud)
                } else {
                    Toggle("Download offloaded attachments from iCloud", isOn: $model.options.downloadFromICloud)
                }
                Text("Re-downloads photos Messages moved to iCloud. Uses the network and can be slow.")
                    .font(.caption).foregroundStyle(.secondary)
            }
        }
    }

    // MARK: Date range

    private var dateRangeSection: some View {
        VStack(alignment: .leading, spacing: 8) {
            SectionLabel("Date range")
            if screenshotMode {
                FauxCheckbox(label: "From", on: false)
                FauxCheckbox(label: "To", on: false)
            } else {
                Toggle(isOn: $fromEnabled) {
                    HStack {
                        Text("From")
                        if fromEnabled {
                            DatePicker("", selection: $fromDate, displayedComponents: .date)
                                .labelsHidden()
                        }
                    }
                }
                Toggle(isOn: $toEnabled) {
                    HStack {
                        Text("To")
                        if toEnabled {
                            DatePicker("", selection: $toDate, displayedComponents: .date)
                                .labelsHidden()
                        }
                    }
                }
            }
            Text("Leave both off to include the entire conversation.")
                .font(.caption).foregroundStyle(.secondary)
        }
    }

    // MARK: Front matter

    private var frontMatterSection: some View {
        VStack(alignment: .leading, spacing: 8) {
            SectionLabel("Title page")
            if screenshotMode {
                FauxTextField(text: model.options.title, prompt: "Title")
                FauxTextField(text: model.options.author, prompt: "Author (optional)")
            } else {
                TextField("Title", text: $model.options.title)
                    .textFieldStyle(.roundedBorder)
                TextField("Author (optional)", text: $model.options.author)
                    .textFieldStyle(.roundedBorder)
            }
        }
    }

    // MARK: Output

    private var outputSection: some View {
        VStack(alignment: .leading, spacing: 8) {
            SectionLabel("Save to")
            HStack {
                Text(model.options.outputDirectory.path)
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Spacer()
                Button("Choose…") { chooseOutputDirectory() }
            }
            .padding(10)
            .background(Color.secondary.opacity(0.06), in: RoundedRectangle(cornerRadius: 8))
        }
    }

    // MARK: Advanced

    private var advancedSection: some View {
        DisclosureGroup("Advanced", isExpanded: $showAdvanced) {
            VStack(alignment: .leading, spacing: 10) {
                Toggle(isOn: $limitEnabled) {
                    HStack {
                        Text("Only the first")
                        if limitEnabled {
                            TextField("", value: $limitValue, format: .number)
                                .frame(width: 70).textFieldStyle(.roundedBorder)
                            Text("messages")
                        } else {
                            Text("N messages (faster preview)")
                        }
                    }
                }
                Toggle(isOn: $maxMBEnabled) {
                    HStack {
                        Text("Skip attachments larger than")
                        if maxMBEnabled {
                            TextField("", value: $maxMBValue, format: .number)
                                .frame(width: 60).textFieldStyle(.roundedBorder)
                            Text("MB")
                        } else {
                            Text("N MB")
                        }
                    }
                }
            }
            .padding(.top, 6)
        }
    }

    // MARK: Actions

    private func seedDates() {
        // Default the pickers to the conversation's own span, if we know it.
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.dateFormat = "yyyy-MM-dd"
        if let first = model.selection?.first, let date = formatter.date(from: first) { fromDate = date }
        if let last = model.selection?.last, let date = formatter.date(from: last) { toDate = date }
    }

    private func export() {
        model.options.fromDate = fromEnabled ? fromDate : nil
        model.options.toDate = toEnabled ? toDate : nil
        model.options.limit = limitEnabled ? max(1, limitValue) : nil
        model.options.maxAttachmentMB = maxMBEnabled ? max(1, maxMBValue) : nil
        model.startExport()
    }

    private func chooseOutputDirectory() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.canCreateDirectories = true
        panel.allowsMultipleSelection = false
        panel.directoryURL = model.options.outputDirectory.deletingLastPathComponent()
        panel.prompt = "Save Here"
        if panel.runModal() == .OK, let url = panel.url {
            model.options.outputDirectory = url
        }
    }
}
