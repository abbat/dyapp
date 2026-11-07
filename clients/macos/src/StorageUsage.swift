import SwiftUI

/// Local storage numbers for Settings; `nil` means unknown, never zero.
enum StorageUsage {
    static func messageBytes(in directory: URL) -> Int64? {
        let store = directory.appendingPathComponent("client.db")
        guard let used = fileSize(store) else { return nil }
        // SQLite keeps recent writes beside the database.
        return ["-wal", "-shm"].reduce(used) {
            $0 + (fileSize(URL(fileURLWithPath: store.path + $1)) ?? 0)
        }
    }

    static func freeBytes(on directory: URL) -> Int64? {
        let keys: Set<URLResourceKey> = [.volumeAvailableCapacityForImportantUsageKey]
        return (try? directory.resourceValues(forKeys: keys))?
            .volumeAvailableCapacityForImportantUsage
    }

    /// Binary units as on the desktop client: whole bytes, one decimal above them.
    static func sizeText(_ bytes: Int64, locale: Locale = .current) -> String {
        let units = [
            "usage.unit.B", "usage.unit.KiB", "usage.unit.MiB", "usage.unit.GiB", "usage.unit.TiB",
        ]
        var value = Double(bytes)
        var unit = 0
        while value >= 1_024, unit < units.count - 1 {
            value /= 1_024
            unit += 1
        }
        let style = FloatingPointFormatStyle<Double>(locale: locale).precision(
            .fractionLength(unit == 0 ? 0 : 1))
        return value.formatted(style) + " " + NSLocalizedString(units[unit], comment: "")
    }

    private static func fileSize(_ file: URL) -> Int64? {
        (try? file.resourceValues(forKeys: [.fileSizeKey]))?.fileSize.map { Int64($0) }
    }
}

/// The status bar under the window content, refreshed every 2 seconds.
struct StatusBar: View {
    let candidate: IdentityCandidate?
    @State private var traffic: ClientTraffic?
    @State private var messages: Int64?

    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            Text(verbatim: Self.trafficText(traffic))
                .fixedSize(horizontal: false, vertical: true)
                .help(Text("statusbar.trafficNote"))
                .accessibilityIdentifier("status-traffic")
            Spacer(minLength: 8)
            Text(verbatim: Self.messagesText(messages))
                .fixedSize(horizontal: false, vertical: true)
                .accessibilityIdentifier("status-messages")
        }
        .font(.caption)
        .padding(.horizontal, 8)
        .padding(.vertical, 4)
        .accessibilityElement(children: .contain)
        .accessibilityLabel(Text("statusbar.label"))
        .task(id: candidate?.location) {
            let directory = candidate.flatMap { candidate in
                (try? RustBridge.directory()).map {
                    RustBridge.networkDirectory(root: $0, candidate: candidate)
                }
            }
            while !Task.isCancelled {
                traffic = clientTraffic()
                messages = directory.flatMap { StorageUsage.messageBytes(in: $0) }
                try? await Task.sleep(nanoseconds: 2_000_000_000)
            }
        }
    }

    nonisolated static func trafficText(_ traffic: ClientTraffic?) -> String {
        guard let traffic else { return NSLocalizedString("statusbar.noTraffic", comment: "") }
        return String(
            format: NSLocalizedString("statusbar.traffic", comment: ""),
            StorageUsage.sizeText(Int64(clamping: traffic.received)),
            StorageUsage.sizeText(Int64(clamping: traffic.sent))
        )
    }

    nonisolated static func messagesText(_ bytes: Int64?) -> String {
        guard let bytes else { return NSLocalizedString("statusbar.noMessages", comment: "") }
        return String(
            format: NSLocalizedString("statusbar.messages", comment: ""),
            StorageUsage.sizeText(bytes))
    }
}

/// The Storage section of Settings. Apple runs no embedded node, so nothing here has a quota or a usage bar.
struct StorageSection: View {
    let candidate: IdentityCandidate
    @State private var root: URL?
    @State private var messages: Int64?
    @State private var store: URL?
    @State private var free: Int64?

    var body: some View {
        Section {
            row("usage.identityDir", root?.path, id: "storage-identity")
                .task(id: candidate.location) {
                    while !Task.isCancelled {
                        refresh()
                        try? await Task.sleep(nanoseconds: 10_000_000_000)
                    }
                }
            row(
                "usage.messages", messages.map { StorageUsage.sizeText($0) }, id: "storage-messages"
            )
            if messages != nil, let store {
                Text(verbatim: store.path)
                    .font(.caption)
                    .textSelection(.enabled)
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier("storage-messages-path")
            }
            if let free {
                row("usage.free", StorageUsage.sizeText(free), id: "storage-free")
            }
        } header: {
            Text("usage.title")
        }
    }

    private func row(_ title: LocalizedStringKey, _ value: String?, id: String) -> some View {
        VStack(alignment: .leading) {
            Text(title)
            Group {
                if let value {
                    Text(verbatim: value)
                } else {
                    Text("usage.unavailable")
                }
            }
            .font(.caption)
            .textSelection(.enabled)
            .fixedSize(horizontal: false, vertical: true)
            .accessibilityIdentifier(id)
        }
    }

    private func refresh() {
        guard let folder = try? RustBridge.directory() else { return }
        let directory = RustBridge.networkDirectory(root: folder, candidate: candidate)
        root = folder
        store = directory.appendingPathComponent("client.db")
        messages = StorageUsage.messageBytes(in: directory)
        // The identity's own folder may not exist yet; the volume is the same.
        free = StorageUsage.freeBytes(on: folder)
    }
}
