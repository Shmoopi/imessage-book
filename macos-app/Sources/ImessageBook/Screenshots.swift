import SwiftUI
import AppKit

// Renders the real wizard views to PNGs for the README, using synthetic sample data (no
// real contacts). Triggered by the IMB_SHOTS_DIR environment variable — see AppDelegate.
enum Screenshots {
    /// Synthetic conversations that mirror the README's examples — nothing from a real DB.
    static let sampleConversations: [Conversation] = [
        Conversation(identifier: "+15555550142", displayName: "Naomi",
                     count: 4958, first: "2023-01-01", last: "2024-03-15", isGroup: false),
        Conversation(identifier: "chat904418871203", displayName: "Family",
                     count: 3120, first: "2019-05-02", last: "2024-07-01", isGroup: true),
        Conversation(identifier: "alex@example.com", displayName: "Alex Rivera",
                     count: 1774, first: "2021-08-14", last: "2024-02-20", isGroup: false),
        Conversation(identifier: "+15555550188", displayName: "Mom",
                     count: 2210, first: "2018-11-20", last: "2024-06-30", isGroup: false),
        Conversation(identifier: "chat610255512034", displayName: "Weekend Trip 🏔️",
                     count: 861, first: "2023-06-02", last: "2023-06-11", isGroup: true),
    ]

    @MainActor
    static func render(to directory: URL) {
        try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)

        // Heights are tuned per step so each fits its content without a scrollbar.
        shot(.source, CGSize(width: 900, height: 560), "source", directory)
        shot(.conversation, CGSize(width: 900, height: 690), "conversation", directory)
        shot(.options, CGSize(width: 900, height: 1110), "options", directory)
        shot(.run, CGSize(width: 900, height: 560), "done", directory)
    }

    // MARK: Scene construction

    @MainActor
    private static func shot(_ step: WizardStep, _ size: CGSize, _ name: String, _ directory: URL) {
        let model = mockModel(step: step)
        let view = RootView()
            .environmentObject(model)
            .environment(\.screenshotMode, true)
            .frame(width: size.width, height: size.height)
        write(view, name: name, size: size, in: directory)
    }

    @MainActor
    private static func mockModel(step: WizardStep) -> AppModel {
        let model = AppModel()
        // Pretend the environment is fully set up so no error/permission banners show.
        model.binaryURL = URL(fileURLWithPath: "/usr/bin/true")
        model.fullDiskAccess = .granted
        model.hasPDFEngine = true
        model.source = .liveMac
        model.conversations = sampleConversations
        model.loadState = .loaded
        model.step = step

        let naomi = sampleConversations[0]
        model.selection = naomi
        model.options.title = "The Naomi Chronicles"
        model.options.author = "Your Name"
        model.options.format = step == .run ? .pdf : .html

        if step == .run {
            let result = model.options.outputDirectory.appendingPathComponent("book.pdf")
            model.run = RunController.screenshot(
                state: .succeeded(result: result),
                lines: [
                    "Found conversation: +15555550142 (\"Naomi\")",
                    "Rendered 4958 messages. Building PDF…",
                    "note: Writing `book.pdf` (12.4 MiB)",
                    "Wrote book.pdf",
                ])
        }
        return model
    }

    // MARK: Output

    @MainActor
    private static func write(_ view: some View, name: String, size: CGSize, in directory: URL) {
        let renderer = ImageRenderer(content: view)
        renderer.scale = 2
        guard let cg = renderer.cgImage else {
            FileHandle.standardError.write(Data("failed to render \(name)\n".utf8))
            return
        }
        let rep = NSBitmapImageRep(cgImage: cg)
        rep.size = size   // keep points; pixels are 2×
        guard let data = rep.representation(using: .png, properties: [:]) else { return }
        let url = directory.appendingPathComponent("\(name).png")
        try? data.write(to: url)
        print("wrote \(url.path)")
    }
}
