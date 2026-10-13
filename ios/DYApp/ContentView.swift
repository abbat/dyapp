import SwiftUI

/// The start screen; `app-ready` tags the element UI tests wait for, `peer-id` the core's answer.
struct ContentView: View {
    @State private var peerId = RustBridge.peerId()

    var body: some View {
        VStack(spacing: 16) {
            Text("DYApp").font(.largeTitle)
            if !ProcessInfo.processInfo.arguments.contains("--break-ui") {
                Text("Ready").accessibilityIdentifier("app-ready")
            }
            Text(peerId).accessibilityIdentifier("peer-id")
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
