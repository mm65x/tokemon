import AppKit
import SwiftUI

@main
struct TokemonMenuBarApp: App {
    @StateObject private var model = AppModel()

    init() {
        NSApplication.shared.setActivationPolicy(.accessory)
    }

    var body: some Scene {
        MenuBarExtra {
            MenuContentView(model: model)
        } label: {
            Text(model.menuBarTitle)
                .monospacedDigit()
                .onAppear {
                    model.start()
                }
        }
        .menuBarExtraStyle(.window)
    }
}
