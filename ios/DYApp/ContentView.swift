import SwiftUI

/// The start screen, its strings from Localizable.xcstrings; `app-ready` tags the element UI tests wait for, `peer-id` the core's answer.
struct ContentView: View {
    // A storage failure shows its reason in place of the peer ID, which the UI tests reject.
    @State private var peerId: String = {
        do {
            return try RustBridge.peerId()
        } catch {
            return "\(error)"
        }
    }()

    var body: some View {
        VStack(spacing: 16) {
            Text("app.name").font(.largeTitle)
            if !ProcessInfo.processInfo.arguments.contains("--break-ui") {
                Text("status.ready").accessibilityIdentifier("app-ready")
            }
            Text(peerId).accessibilityIdentifier("peer-id")
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
