import AppKit
import SwiftUI

@main
struct TokemonMenuBarApp: App {
    init() {
        NSApplication.shared.setActivationPolicy(.accessory)
    }

    var body: some Scene {
        MenuBarExtra("Tokemon") {
            Text("Loading")
                .padding()
        }
        .menuBarExtraStyle(.window)
    }
}
