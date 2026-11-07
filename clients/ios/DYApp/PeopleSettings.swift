import SwiftUI

/// Sending read receipts. A failed read or save keeps the last value and says so.
struct ReadReceiptsSection: View {
    @ObservedObject var feed: ChatFeed
    @State private var enabled: Bool?
    @State private var busy = false
    @State private var failed = false

    var body: some View {
        Section {
            Toggle("receipts.toggle", isOn: Binding(get: { enabled ?? false }, set: { save($0) }))
                .disabled(enabled == nil || busy)
                .accessibilityIdentifier("send-read-receipts")
                .task(id: feed.me) { await load() }
            Text("receipts.hint").font(.caption)
            if failed {
                Text("receipts.failed").font(.caption).accessibilityIdentifier("receipts-error")
            }
        }
    }

    private func load() async {
        guard feed.me != nil else { return }
        let result = await feed.call { try $0.readReceipts() }
        switch result {
        case let .success(value):
            enabled = value
            failed = false

        case .failure:
            failed = true
        }
    }

    private func save(_ value: Bool) {
        busy = true
        Task {
            let result = await feed.call { try $0.setReadReceipts(enabled: value) }
            switch result {
            case .success:
                enabled = value
                failed = false

            case .failure:
                failed = true
            }
            busy = false
        }
    }
}

/// The people this identity blocked, then those it passed on.
struct PeopleSections: View {
    @ObservedObject var feed: ChatFeed

    var body: some View {
        PeopleSection(
            feed: feed, tag: "blocked", undoTag: "unblock", title: "settings.blocked",
            hint: "settings.blockedHint",
            none: "settings.blockedNone", failure: "settings.blockedFailed",
            undo: "settings.unblock",
            canRetry: true, list: { try $0.blockedPeers() }, change: { try $0.unblock(peerId: $1) }
        )
        PeopleSection(
            feed: feed, tag: "passed", undoTag: "unpass", title: "settings.passed",
            hint: "settings.passedHint",
            none: "settings.passedNone", failure: "settings.passedFailed", undo: "settings.unpass",
            canRetry: false, list: { try $0.passedPeers() },
            change: { _ = try $0.setPassed(peerId: $1, passed: false) }
        )
    }
}

/// One list of people with an undo on each row. A failure keeps the last rows and says so.
private struct PeopleSection: View {
    @ObservedObject var feed: ChatFeed
    let tag: String
    let undoTag: String
    let title: LocalizedStringKey
    let hint: LocalizedStringKey
    let none: LocalizedStringKey
    let failure: LocalizedStringKey
    let undo: LocalizedStringKey
    let canRetry: Bool
    let list: @Sendable (ChatSession) throws -> [String]
    let change: @Sendable (ChatSession, String) throws -> Void
    @State private var peers: [String]?
    @State private var busy = false
    @State private var failed = false
    @State private var status: ChatStatus?

    var body: some View {
        Section {
            Text(hint).font(.caption).task(id: feed.me) { await refresh(undoing: nil) }
            if peers?.isEmpty == true {
                Text(none).accessibilityIdentifier("\(tag)-none")
            }
            ForEach(Array((peers ?? []).enumerated()), id: \.element) { index, peer in
                row(peer, number: index + 1)
            }
            if failed {
                Text(failure).font(.caption).accessibilityIdentifier("\(tag)-error")
            }
            if let status {
                Group {
                    Text(LocalizedStringKey(status.key))
                    if let details = status.details {
                        Text(verbatim: details).textSelection(.enabled)
                    }
                }
                .font(.caption)
                .accessibilityIdentifier("\(tag)-revoke-status")
            }
        } header: {
            Text(title)
        }
    }

    private func row(_ peer: String, number: Int) -> some View {
        VStack(alignment: .leading) {
            Text(verbatim: contactLabel(peer, feed.names[peer]))
            HStack {
                Button(undo) { Task { await refresh(undoing: peer) } }
                    .accessibilityIdentifier("\(undoTag)-\(number)")
                if canRetry {
                    Button("settings.blockedRetry") { Task { await retry(peer) } }
                        .accessibilityIdentifier("retry-revocation-\(number)")
                }
            }
            // An iOS form row fires every default-styled button in it on any tap.
            .buttonStyle(.borderless)
            .disabled(busy)
        }
    }

    /// The change and the new list run together, as the rows then match what was stored.
    private func refresh(undoing peer: String?) async {
        guard feed.me != nil else { return }
        busy = true
        let (list, change) = (list, change)
        let result = await feed.call { session -> [String] in
            if let peer { try change(session, peer) }
            return try list(session)
        }
        busy = false
        guard case let .success(read) = result else {
            failed = true
            return
        }
        peers = read
        failed = false
        if peer != nil { await feed.read() }
        await feed.loadNames(read)
    }

    /// Blocking again asks the network to revoke the tokens once more.
    private func retry(_ peer: String) async {
        busy = true
        let result = await feed.call { try $0.block(peerId: peer) }
        busy = false
        switch result {
        case let .success(block):
            status = ChatStatus(
                key: block.revocationKey, details: block.revocation == .pending ? block.detail : nil
            )

        case let .failure(error):
            status = ChatStatus(
                key: "settings.blockedRetryFailed", details: String(describing: error))
        }
    }
}
