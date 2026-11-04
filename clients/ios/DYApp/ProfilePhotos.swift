import SwiftUI
import UniformTypeIdentifiers
#if os(iOS)
import PhotosUI
#endif

/// The draft's thumbnail hashes in order, and each preview that passed the core's checks.
struct ProfilePhotoRows: Sendable {
    let hashes: [Data]
    let previews: [Data: Rgba]
}

extension RustBridge {
    /// Runs a change or a listing of the draft's photos, then reads the previews it names.
    static func changeProfilePhotos(for candidate: IdentityCandidate, peerId: String,
                                    change: (String, IdentityKey) throws -> [Data]) throws -> ProfilePhotoRows {
        let key = try identityStore().open(candidate.location)
        guard key.peerId() == peerId else { throw IdentitySlotError.peerIDMismatch }
        let dir = networkDirectory(root: try directory(), candidate: candidate).path
        let hashes = try change(dir, key)
        var previews: [Data: Rgba] = [:]
        for hash in hashes {
            previews[hash] = try? checkPreview(bytes: profilePhoto(dir: dir, key: key, hash: hash))
        }
        return ProfilePhotoRows(hashes: hashes, previews: previews)
    }
}

/// Catalogue key for a failed photo change: the core's codes, then what preparation refused.
func photoErrorKey(_ error: Error) -> String {
    switch error {
    case PhotoProblem.tooLarge:
        return "profile.photoTooLarge"

    case PhotoProblem.unsupported, is CocoaError:
        return "profile.photoUnsupported"

    case CoreError.Failed(message: "photoRefused"):
        return "profile.photoRefused"

    case CoreError.Failed(message: "corrupted"):
        return "profile.photoUnsupported"

    default:
        return "profile.photoFailed"
    }
}

/// A core-checked preview as an image; the pixels are opaque, so premultiplying changes nothing.
func previewImage(_ preview: Rgba) -> Image? {
    guard let space = CGColorSpace(name: CGColorSpace.sRGB),
          let provider = CGDataProvider(data: preview.pixels as CFData),
          let image = CGImage(width: Int(preview.width), height: Int(preview.height), bitsPerComponent: 8,
                              bitsPerPixel: 32, bytesPerRow: Int(preview.width) * 4, space: space,
                              bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
                              provider: provider, decode: nil, shouldInterpolate: true, intent: .defaultIntent)
    else { return nil }
    return Image(decorative: image, scale: 1)
}

/// The editor's own photos. They are draft writes, so a change also ends a prepared proof.
struct ProfilePhotosSection: View {
    private static let limit = 6
    let candidate: IdentityCandidate
    let peerId: String
    let disabled: Bool
    let changed: () -> Void
    @State private var rows = ProfilePhotoRows(hashes: [], previews: [:])
    @State private var busy = false
    @State private var status = ""
    @State private var picking = false
    @State private var removing: Int?

    private var isFull: Bool { rows.hashes.count >= Self.limit }
    private var locked: Bool { disabled || busy }

    var body: some View {
        Section {
            Text("profile.photos").font(.headline)
            if !rows.hashes.isEmpty {
                ScrollView(.horizontal) {
                    HStack(alignment: .top) {
                        ForEach(rows.hashes.indices, id: \.self) { tile($0) }
                    }
                }
            }
            withPicker(
                // Without a borderless style a Form row fires every button in it.
                Button("profile.photoAdd") { addTapped() }
                    .buttonStyle(.borderless)
                    .disabled(locked || isFull)
                    .accessibilityIdentifier("profile-photo-add")
                    .task(id: candidate.location.marker) {
                        await apply(edits: false) { dir, key in try profilePhotos(dir: dir, key: key) }
                    }
                    .confirmationDialog("profile.photoRemoveTitle", isPresented: Binding(
                        get: { removing != nil },
                        set: { if !$0 { removing = nil } }
                    ), titleVisibility: .visible) { removeButtons } message: {
                        Text("profile.photoRemoveText")
                    }
            )
            Text(LocalizedStringKey(isFull ? "profile.photoLimit" : "profile.photoHint")).font(.caption)
            if !status.isEmpty {
                Text(LocalizedStringKey(status)).accessibilityIdentifier("profile-photo-status")
            }
        }
    }

    @ViewBuilder private var removeButtons: some View {
        Button("profile.photoRemoveAction", role: .destructive) {
            guard let index = removing else { return }
            Task {
                await apply(edits: true) { dir, key in
                    try RustBridge.withDraftLock { try removeProfilePhoto(dir: dir, key: key, index: UInt32(index)) }
                }
            }
        }
        Button("search.cancel", role: .cancel) {}
    }

    private func tile(_ index: Int) -> some View {
        VStack(spacing: 4) {
            Group {
                if let preview = rows.previews[rows.hashes[index]], let image = previewImage(preview) {
                    image.resizable().scaledToFill()
                } else {
                    Color.secondary.opacity(0.2)
                }
            }
            .frame(width: 96, height: 96)
            .clipped()
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(text("profile.photoLabel", index + 1, rows.hashes.count))
            .accessibilityIdentifier("profile-photo-\(index)")
            if index == 0 { Text("profile.photoMain").font(.caption) }
            HStack {
                control("arrow.left", "profile.photoLeft", "left", index, enabled: index > 0) { move(index, to: index - 1) }
                control("arrow.right", "profile.photoRight", "right", index, enabled: index + 1 < rows.hashes.count) {
                    move(index, to: index + 1)
                }
                control("xmark", "profile.photoRemove", "remove", index, enabled: true) { removing = index }
            }
        }
    }

    private func text(_ key: String, _ numbers: CVarArg...) -> String {
        String(format: NSLocalizedString(key, comment: ""), arguments: numbers)
    }

    private func control(_ symbol: String, _ key: String, _ tag: String, _ index: Int, enabled: Bool,
                         action: @escaping () -> Void) -> some View {
        Button(action: action) { Image(systemName: symbol) }
            .buttonStyle(.borderless)
            .disabled(locked || !enabled)
            .accessibilityLabel(text(key, index + 1))
            .accessibilityIdentifier("profile-photo-\(tag)-\(index)")
    }

    private func move(_ from: Int, to: Int) {
        Task {
            await apply(edits: true) { dir, key in
                try RustBridge.withDraftLock { try moveProfilePhoto(dir: dir, key: key, from: UInt32(from), to: UInt32(to)) }
            }
        }
    }

    private func addTapped() {
        #if DEBUG
        if ProcessInfo.processInfo.arguments.contains("--ui-test-pick-photo") {
            let red = CGFloat(rows.hashes.count + 1) / CGFloat(Self.limit)
            Task { await add { try PhotoPrep.samplePNG(width: 640, height: 480, red: red, alpha: 0.5) } }
            return
        }
        #endif
        picking = true
    }

    private func add(_ read: @escaping @Sendable () throws -> Data) async {
        status = "profile.photoWorking"
        await apply(edits: true) { dir, key in
            // Encoding takes a while, so it stays outside the lock that a save waits on.
            let photo = try PhotoPrep.prepare(read())
            return try RustBridge.withDraftLock {
                try addProfilePhoto(dir: dir, key: key, full: photo.full, thumbnail: photo.thumbnail)
            }
        }
    }

    private func apply(edits: Bool, _ change: @escaping @Sendable (String, IdentityKey) throws -> [Data]) async {
        busy = true
        let selected = candidate
        let peer = peerId
        let result = await Task.detached(priority: .userInitiated) {
            Result { try RustBridge.changeProfilePhotos(for: selected, peerId: peer, change: change) }
        }.value
        busy = false
        switch result {
        case let .success(fresh):
            rows = fresh
            // A form row that scrolls back into view lists again and must keep the last status.
            if edits {
                status = "profile.photoChanged"
                changed()
            }

        case let .failure(error):
            status = photoErrorKey(error)
        }
    }

    @ViewBuilder
    private func withPicker(_ content: some View) -> some View {
        #if os(iOS)
        content.sheet(isPresented: $picking) {
            PhotoPicker { data in
                picking = false
                guard let data else { return }
                Task { await add { data } }
            }
        }
        #else
        content.fileImporter(isPresented: $picking, allowedContentTypes: [.jpeg, .png]) { result in
            guard case let .success(url) = result else { return }
            Task { await add { try PhotoPrep.read(url) } }
        }
        #endif
    }
}

#if os(iOS)
/// The system picker runs out of process and needs no photo library permission.
private struct PhotoPicker: UIViewControllerRepresentable {
    let picked: @MainActor @Sendable (Data?) -> Void

    final class Coordinator: NSObject, PHPickerViewControllerDelegate {
        private let picked: @MainActor @Sendable (Data?) -> Void

        init(picked: @escaping @MainActor @Sendable (Data?) -> Void) {
            self.picked = picked
        }

        func picker(_ picker: PHPickerViewController, didFinishPicking results: [PHPickerResult]) {
            let picked = picked
            guard let provider = results.first?.itemProvider else {
                Task { @MainActor in picked(nil) }
                return
            }
            _ = provider.loadDataRepresentation(forTypeIdentifier: UTType.image.identifier) { data, _ in
                Task { @MainActor in picked(data) }
            }
        }
    }

    func makeCoordinator() -> Coordinator {
        Coordinator(picked: picked)
    }

    func makeUIViewController(context: Context) -> PHPickerViewController {
        var configuration = PHPickerConfiguration()
        configuration.filter = .images
        // A HEIC original arrives as a JPEG, the only camera format preparation takes.
        configuration.preferredAssetRepresentationMode = .compatible
        let picker = PHPickerViewController(configuration: configuration)
        picker.delegate = context.coordinator
        return picker
    }

    func updateUIViewController(_ controller: PHPickerViewController, context: Context) {}
}
#endif
