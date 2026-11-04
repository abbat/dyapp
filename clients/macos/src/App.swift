import SwiftUI

@main
struct DYAppApp: App {
    @NSApplicationDelegateAdaptor(MainWindowController.self) private var controller

    var body: some Scene {
        WindowGroup {
            ContentView()
                .frame(minWidth: 400, minHeight: 560)
                .background(WindowReader { controller.adopt($0) })
        }
        .commands { CommandGroup(replacing: .newItem) {} }
    }

    init() {
        // Resume would bring back what the design does not save: minimized and full screen.
        // A saved state without windows would also make the WindowGroup open none at launch.
        UserDefaults.standard.register(defaults: ["ApplePersistenceIgnoreState": true])
        NetworkType.startReporting()
    }
}
