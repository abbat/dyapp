import SwiftUI

// shortcut: static until the shared checker reads the live feed, then persist the interval app-wide.
/// Says updates are not published here yet; it sends no request and saves nothing.
struct UpdatesSection: View {
    var body: some View {
        Section {
            Picker("update.interval", selection: .constant(24)) {
                Text("update.every6h").tag(6)
                Text("update.every12h").tag(12)
                Text("update.every24h").tag(24)
                Text("update.every7d").tag(168)
            }
            .disabled(true)
            .accessibilityIdentifier("update-interval")
            Text(String(
                format: NSLocalizedString("update.lastCheck", comment: ""),
                NSLocalizedString("update.never", comment: "")
            ))
            Text("update.notPublished").accessibilityIdentifier("update-state")
            Button("update.check") {}
                .disabled(true)
                .accessibilityIdentifier("update-check")
        } header: {
            Text("settings.updates")
        }
    }
}
