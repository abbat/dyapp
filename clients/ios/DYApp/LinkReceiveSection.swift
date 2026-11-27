import SwiftUI

@MainActor
struct LinkReceiveSection: View {
    @Environment(\.displayScale) private var displayScale
    @ObservedObject var feed: ChatFeed
    var wizard = false
    let installed: () -> Void
    @State private var invitation: Invitation?
    @State private var opening = UUID()
    @State private var busy = false
    @State private var failed = false

    private struct Invitation: @unchecked Sendable {
        let receiver: MobileLinkReceiver
        let code: String
        let matrix: MobileQrMatrix
        let context: ChatFeed.Context
    }

    var body: some View {
        Section {
            if let invitation {
                qr(invitation.matrix)
                Text(verbatim: invitation.code).textSelection(.enabled)
                    .accessibilityIdentifier("link-receive-code")
                Button("sticker.pack.refresh") { check(invitation) }.disabled(busy)
                Button("search.cancel") {
                    _ = invitation.receiver.cancel()
                    self.invitation = nil
                }
            } else {
                Text("link.receive.intro").font(.caption)
                Button("link.receive.start", action: start).disabled(busy)
                    .accessibilityIdentifier("link-receive-start")
            }
            if busy { ProgressView() }
            if failed { Text("devices.uncertain").font(.caption) }
        }
        .onChange(of: feed.owner) { _ in
            _ = invitation?.receiver.cancel()
            invitation = nil
            failed = false
        }
        .onDisappear {
            opening = UUID()
            _ = invitation?.receiver.cancel()
            invitation = nil
        }
    }

    private func qr(_ matrix: MobileQrMatrix) -> some View {
        Canvas { context, size in
            let side = Int(matrix.size)
            guard side > 0 else { return }
            let total = CGFloat(side + 8)
            let pixels = (min(size.width, size.height) * displayScale / total).rounded(.down)
            guard pixels >= 1 else { return }
            let step = pixels / displayScale
            let left = ((size.width - total * step) * displayScale / 2).rounded(.down)
                / displayScale
            let top = ((size.height - total * step) * displayScale / 2).rounded(.down)
                / displayScale
            var path = Path()
            for (index, dark) in matrix.modules.enumerated() where dark {
                path.addRect(CGRect(
                    x: left + CGFloat(index % side + 4) * step,
                    y: top + CGFloat(index / side + 4) * step,
                    width: step, height: step))
            }
            context.fill(path, with: .color(.black))
        }
        .frame(width: 200, height: 200)
        .background(.white)
        .accessibilityLabel(Text("link.receive.qr"))
    }

    private func start() {
        guard !busy, let context = feed.context else { return }
        let ticket = opening
        let wizard = wizard
        busy = true
        failed = false
        Task {
            let result = await IdentityIO.performBlocking {
                Result {
                    let identity = context.identity
                    let receiver = try MobileLinkReceiver.start(
                        registry: RustBridge.identityStore().registry,
                        id: identity.candidate.location.marker, expectedPeer: identity.peerId,
                        wizard: wizard)
                    return try Invitation(
                        receiver: receiver, code: receiver.code(), matrix: receiver.qrMatrix(),
                        context: context)
                }
            }
            busy = false
            guard opening == ticket, feed.isCurrent(context) else {
                if case let .success(value) = result { _ = value.receiver.cancel() }
                return
            }
            if case let .success(value) = result { invitation = value } else { failed = true }
        }
    }

    private func check(_ value: Invitation) {
        guard !busy, feed.isCurrent(value.context) else { return }
        busy = true
        failed = false
        Task {
            let result = await IdentityIO.performBlocking { Result { try value.receiver.poll() } }
            busy = false
            if case .success(.completed) = result {
                invitation = nil
                installed()
            } else if feed.isCurrent(value.context), invitation != nil {
                switch result {
                case .success(.waiting), .success(.receiving), .success(.installing):
                    break

                default:
                    failed = true
                }
            }
        }
    }
}
