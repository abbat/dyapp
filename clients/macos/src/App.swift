import SwiftUI

@main
struct DYAppApp: App {
    @NSApplicationDelegateAdaptor(MainWindowController.self) private var controller

    init() {
        NetworkType.startReporting()
    }

    var body: some Scene {
        WindowGroup {
            ContentView()
                .frame(minWidth: 400, minHeight: 560)
                .background(WindowReader { controller.adopt($0) })
        }
        .commands { CommandGroup(replacing: .newItem) {} }
    }
}
