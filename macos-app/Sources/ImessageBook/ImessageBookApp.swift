import SwiftUI
import AppKit

@main
struct ImessageBookApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate
    @StateObject private var model = AppModel()

    var body: some Scene {
        WindowGroup("imessage-book") {
            RootView()
                .environmentObject(model)
                .frame(minWidth: 760, minHeight: 600)
        }
        .windowResizability(.contentMinSize)
        .commands {
            CommandGroup(replacing: .newItem) {}   // one window; no "New"
        }
    }
}

/// SwiftPM-built executables don't automatically become foreground apps — nudge AppKit so
/// the window shows and the app takes focus when launched from the .app bundle.
final class AppDelegate: NSObject, NSApplicationDelegate {
    func applicationDidFinishLaunching(_ notification: Notification) {
        // Documentation mode: render the README screenshots and quit, no UI needed.
        if let dir = ProcessInfo.processInfo.environment["IMB_SHOTS_DIR"], !dir.isEmpty {
            Screenshots.render(to: URL(fileURLWithPath: dir))
            NSApp.terminate(nil)
            return
        }
        NSApp.setActivationPolicy(.regular)
        NSApp.activate(ignoringOtherApps: true)
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }
}
