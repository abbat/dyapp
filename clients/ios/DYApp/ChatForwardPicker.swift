import SwiftUI

struct ForwardOpening: Identifiable {
    let id = UUID()
    let message: ChatMessage
    let peer: String
    let context: ChatFeed.Context
}

struct ChatForwardPicker: View {
    @Environment(\.dismiss) private var dismiss
    @ObservedObject var feed: ChatFeed
    let opening: ForwardOpening
    @State private var targets: [String]?
    @State private var status: String?
    @State private var busy = true
    @State private var committed = false

    var body: some View {
        ScrollingForm {
            Text("chat.forwardTitle").font(.headline)
            if busy { ProgressView() }
            if let status { Text(LocalizedStringKey(status)) }
            if !committed {
                ForEach(targets ?? [], id: \.self) { target in
                    Button {
                        guard !busy, feed.isCurrent(opening.context) else { return }
                        busy = true
                        Task { await send(target) }
                    } label: {
                        Text(verbatim: contactLabel(target, feed.names[target]))
                    }
                    .accessibilityIdentifier("forward-target-\(target)")
                    .disabled(busy)
                }
            }
            if targets == nil, !busy, status != nil {
                Button("search.retry") {
                    guard !busy, feed.isCurrent(opening.context) else { return }
                    busy = true
                    status = nil
                    Task { await load() }
                }
                .accessibilityIdentifier("forward-retry")
            }
            Button("search.cancel") { dismiss() }
                .accessibilityIdentifier("forward-cancel")
                .disabled(busy && targets != nil)
        }
        .interactiveDismissDisabled(busy && targets != nil)
        .task { await load() }
        .onChange(of: feed.owner) { _ in
            if !feed.isCurrent(opening.context) { dismiss() }
        }
    }

    private func load() async {
        let result = await feed.forwardTargets(
            opening.message, from: opening.peer, in: opening.context)
        guard !Task.isCancelled, feed.isCurrent(opening.context) else { return }
        busy = false
        switch result {
        case .success(let peers):
            targets = peers
            if peers.isEmpty { status = "chat.forwardEmpty" }

        case .failure:
            status = "chat.forwardLoadFailed"
        }
    }

    private func send(_ target: String) async {
        let result = await feed.forward(
            opening.message, from: opening.peer, to: target, in: opening.context)
        guard feed.isCurrent(opening.context) else { dismiss(); return }
        busy = false
        switch result {
        case .success(.accepted):
            dismiss()

        case .success(.queued):
            committed = true
            status = "chat.queued"

        case .failure:
            status = "chat.forwardFailed"
            return
        }
        await feed.read()
    }
}
