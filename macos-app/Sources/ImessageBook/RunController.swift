import Foundation
import AppKit

/// Drives a single export (or the long-running preview server), streaming the engine's
/// stdout/stderr into a live log and tracking terminal state. All mutable state lives on
/// the main actor; the pipe callbacks hop here before touching it.
@MainActor
final class RunController: ObservableObject {
    enum State: Equatable {
        case idle
        case running
        case serving(url: String)                    // preview server is up
        case succeeded(result: URL?)                  // a file was written
        case failed(message: String)
        case cancelled
    }

    @Published private(set) var state: State = .idle
    @Published private(set) var lines: [String] = []

    private var process: Process?
    private var expectedResult: URL?
    private var isServer = false
    private var stdoutRemainder = ""
    private var stderrRemainder = ""
    private var lastStderrLine = ""
    private var didOpenResult = false

    var isRunning: Bool {
        switch state {
        case .running, .serving: return true
        default: return false
        }
    }

    var isBusy: Bool { if case .running = state { return true } else { return false } }

    /// A controller pinned to a fixed state — used to render documentation screenshots.
    static func screenshot(state: State, lines: [String]) -> RunController {
        let controller = RunController()
        controller.lines = lines
        controller.state = state
        return controller
    }

    /// Launch the engine. `binary` is the resolved engine path.
    func start(binary: URL, invocation: ExportInvocation) {
        reset()
        state = .running
        expectedResult = invocation.expectedResult
        isServer = invocation.isServer

        let process = Process()
        process.executableURL = binary
        process.arguments = invocation.arguments
        process.currentDirectoryURL = invocation.workingDirectory
        process.environment = Engine.augmentedEnvironment()

        let stdout = Pipe()
        let stderr = Pipe()
        process.standardOutput = stdout
        process.standardError = stderr

        stdout.fileHandleForReading.readabilityHandler = { [weak self] handle in
            let data = handle.availableData
            if data.isEmpty { handle.readabilityHandler = nil; return }
            let text = String(decoding: data, as: UTF8.self)
            Task { @MainActor [weak self] in self?.ingest(text, isError: false) }
        }
        stderr.fileHandleForReading.readabilityHandler = { [weak self] handle in
            let data = handle.availableData
            if data.isEmpty { handle.readabilityHandler = nil; return }
            let text = String(decoding: data, as: UTF8.self)
            Task { @MainActor [weak self] in self?.ingest(text, isError: true) }
        }
        process.terminationHandler = { [weak self] proc in
            let status = proc.terminationStatus
            let reason = proc.terminationReason
            Task { @MainActor [weak self] in self?.finish(status: status, reason: reason) }
        }

        do {
            try process.run()
            self.process = process
        } catch {
            state = .failed(message: error.localizedDescription)
        }
    }

    /// Stop a running export or the preview server.
    func stop() {
        process?.terminate()
    }

    /// Put the controller straight into a failed state (e.g. we couldn't even build the
    /// command). Clears any prior run first.
    func fail(_ message: String) {
        reset()
        state = .failed(message: message)
    }

    func reset() {
        process = nil
        expectedResult = nil
        isServer = false
        stdoutRemainder = ""
        stderrRemainder = ""
        lastStderrLine = ""
        didOpenResult = false
        lines = []
        state = .idle
    }

    // MARK: - Streaming

    private func ingest(_ text: String, isError: Bool) {
        var buffer = (isError ? stderrRemainder : stdoutRemainder) + text
        var completed: [String] = []
        while let newline = buffer.firstIndex(of: "\n") {
            completed.append(String(buffer[..<newline]))
            buffer = String(buffer[buffer.index(after: newline)...])
        }
        if isError { stderrRemainder = buffer } else { stdoutRemainder = buffer }

        for line in completed {
            append(line, isError: isError)
        }
    }

    private func append(_ line: String, isError: Bool) {
        let trimmed = line.trimmingCharacters(in: .whitespacesAndNewlines)
        if !trimmed.isEmpty {
            lines.append(trimmed)
            if isError { lastStderrLine = trimmed }
        }
        // The preview server announces itself; surface the URL and mark the server "up".
        if let range = line.range(of: "Preview ready at ") {
            let url = String(line[range.upperBound...]).trimmingCharacters(in: .whitespacesAndNewlines)
            if !url.isEmpty { state = .serving(url: url) }
        }
    }

    private func finish(status: Int32, reason: Process.TerminationReason) {
        // Flush any trailing partial lines.
        if !stdoutRemainder.isEmpty { append(stdoutRemainder, isError: false); stdoutRemainder = "" }
        if !stderrRemainder.isEmpty { append(stderrRemainder, isError: true); stderrRemainder = "" }
        process = nil

        // A user-initiated stop shows up as an uncaught SIGTERM.
        let wasSignalled = reason == .uncaughtSignal
        if status == 0 {
            if isServer {
                // The server exited cleanly (rare — usually it's stopped); treat as done.
                state = .cancelled
            } else {
                let result = expectedResult.flatMap { FileManager.default.fileExists(atPath: $0.path) ? $0 : nil }
                state = .succeeded(result: result)
                if let result, !didOpenResult {
                    didOpenResult = true
                    NSWorkspace.shared.open(result)
                }
            }
        } else if wasSignalled {
            state = .cancelled
        } else {
            let detail = lastStderrLine.isEmpty
                ? "The engine stopped with an error (code \(status))."
                : lastStderrLine
            state = .failed(message: detail)
        }
    }
}
