import SwiftUI
import Vision

@MainActor
struct LinkApprovalSection: View {
    @ObservedObject var feed: ChatFeed
    @State private var code = ""
    @State private var picking = false
    @State private var busy = false
    @State private var confirming = false
    @State private var failed = false
    @State private var approved = false
    @State private var pending: Approval?

    private struct Approval: @unchecked Sendable {
        let worker: MobileLinkApproval
        let code: String
        let context: ChatFeed.Context
    }

    var body: some View {
        Section {
            TextField("devices.code.label", text: $code)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .disabled(busy || confirming)
                .accessibilityIdentifier("link-owner-code")
            Button("devices.code.photo") { picking = true }
                .disabled(busy || confirming)
                .accessibilityIdentifier("link-owner-photo")
            Button("network.continue") { prepare() }
                .disabled(code.isEmpty || busy || confirming)
            if failed { Text("devices.uncertain").font(.caption) }
            if approved { Image(systemName: "checkmark").accessibilityLabel(Text("devices.title")) }
        }
        .sheet(isPresented: $picking) {
            let context = feed.context
            PhotoPicker { data in
                guard picking, let context, feed.isCurrent(context) else { return }
                picking = false
                if let data { scan(data, context: context) }
            }
        }
        .confirmationDialog("devices.confirm.action", isPresented: $confirming) {
            Button("devices.confirm.action", role: .destructive) { send() }
            Button("search.cancel", role: .cancel) { pending = nil }
        } message: {
            Text("devices.confirm.text")
            if let pending { Text(verbatim: pending.code) }
        }
        .onChange(of: feed.owner) { _ in
            pending = nil
            picking = false
            confirming = false
            code = ""
            failed = false
            approved = false
        }
    }

    private func scan(_ data: Data, context: ChatFeed.Context) {
        busy = true
        failed = false
        approved = false
        Task {
            let scanned = await IdentityIO.performBlocking { Self.linkCode(in: data) }
            busy = false
            guard feed.isCurrent(context) else { return }
            if let scanned { code = scanned } else { failed = true }
        }
    }

    nonisolated private static func linkCode(in data: Data) -> String? {
        guard UInt64(data.count) <= photoPickedByteLimit() else { return nil }
        let request = VNDetectBarcodesRequest()
        request.symbologies = [.qr]
        let handler = VNImageRequestHandler(data: data)
        guard (try? handler.perform([request])) != nil else { return nil }
        let valid = Set((request.results ?? []).compactMap(\.payloadStringValue).filter {
            deviceLinkCodeStatus(text: $0) == .valid
        })
        return valid.count == 1 ? valid.first : nil
    }

    private func prepare() {
        guard !busy, let context = feed.context else { return }
        let input = code
        busy = true
        failed = false
        approved = false
        Task {
            let result = await IdentityIO.performBlocking {
                Result {
                    let identity = context.identity
                    let worker = try MobileLinkApproval.prepare(
                        registry: RustBridge.identityStore().registry,
                        id: identity.candidate.location.marker, expectedPeer: identity.peerId,
                        code: input)
                    return Approval(worker: worker, code: worker.code(), context: context)
                }
            }
            busy = false
            guard feed.isCurrent(context) else { return }
            switch result {
            case let .success(approval):
                pending = approval
                confirming = true
            case .failure:
                failed = true
            }
        }
    }

    private func send() {
        guard !busy, let approval = pending, feed.isCurrent(approval.context) else { return }
        pending = nil
        busy = true
        Task {
            let result = await IdentityIO.performBlocking {
                Result { try approval.worker.confirmAndSend() }
            }
            busy = false
            guard feed.isCurrent(approval.context) else { return }
            if case .success = result {
                approved = true
                code = ""
            } else {
                failed = true
            }
        }
    }
}
