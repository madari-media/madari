import SwiftUI

@main
struct MadariApp: App {
    @StateObject private var model = AppModel()

    init() {
        DebugLog.reset()
        // The bundled DM Sans must be registered before any view reads a style.
        MadariFont.register()
    }

    var body: some Scene {
        WindowGroup {
            RootView()
                .environmentObject(model)
                .preferredColorScheme(.dark)
        }
    }
}
