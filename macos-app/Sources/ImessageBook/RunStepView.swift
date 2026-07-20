import SwiftUI
import AppKit

struct RunStepView: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.screenshotMode) private var screenshotMode

    var body: some View {
        StepLayout(title: titleText, subtitle: subtitleText) {
            statusBanner
            if !model.run.lines.isEmpty {
                logView
            }
        } footer: {
            footerButtons
        }
    }

    // MARK: Header text

    private var titleText: String {
        switch model.run.state {
        case .running: return "Working…"
        case .serving: return "Preview is running"
        case .succeeded: return "Done!"
        case .failed: return "Something went wrong"
        case .cancelled: return "Stopped"
        case .idle: return "Export"
        }
    }

    private var subtitleText: String? {
        switch model.run.state {
        case .running: return "Reading messages and building your \(model.options.format.title.lowercased())."
        case .serving: return "Your browser should open automatically. Leave this window open while you read."
        case .succeeded: return "Your \(model.options.format.title.lowercased()) is ready."
        default: return nil
        }
    }

    // MARK: Status

    @ViewBuilder private var statusBanner: some View {
        switch model.run.state {
        case .running:
            HStack(spacing: 10) {
                ProgressView()
                Text("Please wait…").foregroundStyle(.secondary)
            }
        case .serving(let url):
            Banner(.success, title: "Serving at \(url)",
                   message: "Open it in your browser, then press Stop when you're finished.") {
                Button("Open in Browser") { open(urlString: url) }
                    .buttonStyle(.borderedProminent)
            }
        case .succeeded(let result):
            Banner(.success, title: "Saved",
                   message: result.map { "Created \($0.lastPathComponent)." })
        case .failed(let message):
            Banner(.error, title: "Export failed", message: message)
        case .cancelled:
            Banner(.info, title: "Export stopped")
        case .idle:
            EmptyView()
        }
    }

    private var logView: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("Details").font(.caption.weight(.semibold)).foregroundStyle(.secondary)
            if screenshotMode {
                logLines.padding(10)
                    .frame(maxWidth: .infinity, minHeight: 120, alignment: .topLeading)
                    .background(Color.secondary.opacity(0.06), in: RoundedRectangle(cornerRadius: 8))
            } else {
                ScrollViewReader { proxy in
                    ScrollView {
                        logLines.padding(10)
                    }
                    .frame(height: 180)
                    .background(Color.secondary.opacity(0.06), in: RoundedRectangle(cornerRadius: 8))
                    .onChange(of: model.run.lines.count) { count in
                        withAnimation { proxy.scrollTo(count - 1, anchor: .bottom) }
                    }
                }
            }
        }
    }

    private var logLines: some View {
        VStack(alignment: .leading, spacing: 2) {
            ForEach(Array(model.run.lines.enumerated()), id: \.offset) { index, line in
                Text(line)
                    .font(.system(.caption, design: .monospaced))
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .id(index)
            }
        }
    }

    // MARK: Footer

    @ViewBuilder private var footerButtons: some View {
        switch model.run.state {
        case .running:
            Text("").frame(maxWidth: .infinity, alignment: .leading)
            Button("Stop", role: .destructive) { model.run.stop() }
        case .serving:
            Button("Stop Preview", role: .destructive) { model.backToOptions() }
            Spacer()
            Button("Make Another") { model.startOver() }
        case .succeeded(let result):
            Button("Back") { model.backToOptions() }
            Spacer()
            if let result {
                Button("Show in Finder") { revealInFinder(result) }
                Button("Open") { open(url: result) }
                    .controlSize(.large)
                    .buttonStyle(.borderedProminent)
            } else {
                Button("Open Folder") { open(url: model.options.outputDirectory) }
            }
            Button("Make Another") { model.startOver() }
        case .failed:
            Button("Back") { model.backToOptions() }
            Spacer()
            Button("Try Again") { model.startExport() }
                .controlSize(.large)
                .buttonStyle(.borderedProminent)
        case .cancelled, .idle:
            Button("Back") { model.backToOptions() }
            Spacer()
            Button("Make Another") { model.startOver() }
        }
    }

    // MARK: Helpers

    private func open(url: URL) { NSWorkspace.shared.open(url) }
    private func open(urlString: String) {
        if let url = URL(string: urlString) { NSWorkspace.shared.open(url) }
    }
    private func revealInFinder(_ url: URL) {
        NSWorkspace.shared.activateFileViewerSelecting([url])
    }
}
