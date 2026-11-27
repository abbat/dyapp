import AVFoundation
import SwiftUI
import Vision
import VisionKit

@available(iOS 16, *)
private struct LinkCameraScanner: UIViewControllerRepresentable {
    let picked: (String) -> Void
    let unavailable: () -> Void

    func makeUIViewController(context: Context) -> DataScannerViewController {
        let scanner = DataScannerViewController(
            recognizedDataTypes: [.barcode(symbologies: [.qr])],
            recognizesMultipleItems: false)
        scanner.delegate = context.coordinator
        do {
            try scanner.startScanning()
        } catch {
            Task { @MainActor in unavailable() }
        }
        return scanner
    }

    func updateUIViewController(_ controller: DataScannerViewController, context: Context) {}

    static func dismantleUIViewController(
        _ controller: DataScannerViewController, coordinator: Coordinator
    ) {
        controller.stopScanning()
    }

    func makeCoordinator() -> Coordinator {
        Coordinator(picked: picked, unavailable: unavailable)
    }

    @MainActor
    final class Coordinator: NSObject, DataScannerViewControllerDelegate {
        let picked: (String) -> Void
        let unavailable: () -> Void

        init(picked: @escaping (String) -> Void, unavailable: @escaping () -> Void) {
            self.picked = picked
            self.unavailable = unavailable
        }

        func dataScanner(
            _ scanner: DataScannerViewController,
            becameUnavailableWithError error: DataScannerViewController.ScanningUnavailable
        ) {
            scanner.stopScanning()
            unavailable()
        }

        func dataScanner(
            _ scanner: DataScannerViewController, didAdd addedItems: [RecognizedItem],
            allItems: [RecognizedItem]
        ) {
            for item in addedItems {
                guard case let .barcode(barcode) = item, let code = barcode.payloadStringValue,
                    deviceLinkCodeStatus(text: code) == .valid
                else { continue }
                scanner.stopScanning()
                picked(code)
                break
            }
        }
    }
}

@MainActor
struct LinkApprovalSection: View {
    @ObservedObject var feed: ChatFeed
    @State private var code = ""
    @State private var picking = false
    @State private var scanning = false
    @State private var busy = false
    @State private var confirming = false
    @State private var failed = false
    @State private var codeError: String?
    @State private var approved = false
    @State private var pending: Approval?
    @State private var opening = UUID()

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
            Button("devices.code.photo") {
                opening = UUID()
                picking = true
            }
                .disabled(busy || confirming)
                .accessibilityIdentifier("link-owner-photo")
            if #available(iOS 16, *), DataScannerViewController.isSupported {
                Button("devices.code.camera", action: requestCamera)
                    .disabled(busy || confirming)
            }
            Button("network.continue") { prepare() }
                .disabled(code.isEmpty || busy || confirming)
            if let codeError {
                Text(LocalizedStringKey(codeError)).font(.caption)
            }
            if failed { Text("devices.uncertain").font(.caption) }
            if approved { Image(systemName: "checkmark").accessibilityLabel(Text("devices.title")) }
        }
        .sheet(isPresented: $picking) {
            let context = feed.context
            let ticket = opening
            PhotoPicker { data in
                guard picking, opening == ticket, let context, feed.isCurrent(context) else { return }
                picking = false
                if let data { scan(data, context: context) }
            }
        }
        .sheet(isPresented: $scanning) {
            if #available(iOS 16, *) {
                let context = feed.context
                LinkCameraScanner { text in
                    guard scanning, let context, feed.isCurrent(context) else { return }
                    code = text
                    scanning = false
                } unavailable: {
                    guard scanning, let context, feed.isCurrent(context) else { return }
                    scanning = false
                    failed = true
                }
            }
        }
        .confirmationDialog("devices.confirm.action", isPresented: $confirming) {
            Button("devices.confirm.action", role: .destructive) { send() }
            Button("search.cancel", role: .cancel) { pending = nil }
        } message: {
            Text("devices.confirm.text")
            if let pending { Text(verbatim: pending.code) }
        }
        .onDisappear {
            opening = UUID()
            pending = nil
            confirming = false
            picking = false
            scanning = false
        }
        .onChange(of: code) { _ in codeError = nil }
        .onChange(of: feed.owner) { _ in
            pending = nil
            picking = false
            scanning = false
            confirming = false
            code = ""
            failed = false
            approved = false
        }
    }

    private func requestCamera() {
        guard !busy, let context = feed.context else { return }
        let ticket = opening
        busy = true
        failed = false
        Task {
            let granted = await AVCaptureDevice.requestAccess(for: .video)
            busy = false
            guard opening == ticket, feed.isCurrent(context) else { return }
            scanning = granted
            failed = !granted
        }
    }

    private func scan(_ data: Data, context: ChatFeed.Context) {
        let ticket = opening
        busy = true
        failed = false
        approved = false
        Task {
            let scanned = await IdentityIO.performBlocking { Self.linkCode(in: data) }
            busy = false
            guard opening == ticket, feed.isCurrent(context) else { return }
            if let scanned { code = scanned } else { failed = true }
        }
    }

    nonisolated private static func linkCode(in data: Data) -> String? {
        guard let image = try? PhotoPrep.decoded(data, purpose: .chat) else { return nil }
        let request = VNDetectBarcodesRequest()
        request.symbologies = [.qr]
        let handler = VNImageRequestHandler(cgImage: image, options: [:])
        guard (try? handler.perform([request])) != nil else { return nil }
        let valid = Set((request.results ?? []).compactMap(\.payloadStringValue).filter {
            deviceLinkCodeStatus(text: $0) == .valid
        })
        return valid.count == 1 ? valid.first : nil
    }

    private func prepare() {
        guard !busy, let context = feed.context else { return }
        let input = code
        let ticket = opening
        failed = false
        approved = false
        switch deviceLinkCodeStatus(text: input) {
        case .malformed:
            codeError = "devices.code.bad"
            return

        case .expired:
            codeError = "devices.code.expired"
            return

        case .valid:
            break
        }
        codeError = nil
        busy = true
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
            guard opening == ticket, feed.isCurrent(context) else { return }
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
