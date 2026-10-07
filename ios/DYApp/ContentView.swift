import SwiftUI

struct ContentView: View {
    var body: some View {
        VStack(spacing: 16) {
            Text("DYApp").font(.largeTitle)
            if !ProcessInfo.processInfo.arguments.contains("--break-ui") {
                Text("Ready").accessibilityIdentifier("app-ready")
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
