import SwiftUI
import UniformTypeIdentifiers
#if os(iOS)
    import PhotosUI
#endif

/// The draft's thumbnail hashes in order, and each preview that passed the core's checks.
struct ProfilePhotoRows: Sendable {
    let hashes: [Data]
    let previews: [Data: Rgba]
    let revision: Data
    let conflict: Bool
}

extension RustBridge {
    /// Runs a change or a listing of the draft's photos, then reads the previews it names.
    static func changeProfilePhotos(
        for candidate: IdentityCandidate, peerId: String,
        change: (String, IdentityKey) throws -> ProfilePhotoSnapshot
    ) throws -> ProfilePhotoRows {
        try withDraftLock {
            try withIdentity(candidate, peerId: peerId) { dir, key in
                try profilePhotoRows(dir: dir, key: key, change: change)
            }
        }
    }
}

/// Read one checked result, or refresh a stale action without replaying its numerical positions.
func profilePhotoRows(
    dir: String, key: IdentityKey,
    change: (String, IdentityKey) throws -> ProfilePhotoSnapshot
) throws -> ProfilePhotoRows {
    let snapshot: ProfilePhotoSnapshot
    let conflict: Bool
    do {
        snapshot = try change(dir, key)
        conflict = false
    } catch ProfilePhotoEditError.Changed {
        snapshot = try profilePhotoSnapshot(dir: dir, key: key)
        conflict = true
    }
    var previews: [Data: Rgba] = [:]
    for hash in snapshot.thumbnails {
        previews[hash] = try? checkPreview(bytes: profilePhoto(dir: dir, key: key, hash: hash))
    }
    return ProfilePhotoRows(
        hashes: snapshot.thumbnails, previews: previews,
        revision: snapshot.revision, conflict: conflict)
}

/// Catalogue key for a failed photo change: the core's codes, then what preparation refused.
func photoErrorKey(_ error: Error) -> String {
    switch error {
    case PhotoProblem.tooLarge, PhotoPreparationError.TooLarge:
        return "profile.photoTooLarge"

    case PhotoProblem.unsupported, PhotoPreparationError.Unsupported, is CocoaError:
        return "profile.photoUnsupported"

    case CoreError.Failed(message: "photoRefused"),
        ProfilePhotoEditError.Failed(reason: "photoRefused"):
        return "profile.photoRefused"

    case CoreError.Failed(message: "corrupted"),
        ProfilePhotoEditError.Failed(reason: "corrupted"):
        return "profile.photoUnsupported"

    default:
        return "profile.photoFailed"
    }
}

/// A core-checked opaque photo preview.
func previewImage(_ preview: Rgba) -> Image? {
    rgbaImage(
        width: preview.width, height: preview.height, pixels: preview.pixels,
        alpha: .noneSkipLast)
}

/// A core-decoded sticker carries straight RGBA, including transparent edges.
func stickerImage(width: UInt32, height: UInt32, pixels: Data) -> Image? {
    rgbaImage(width: width, height: height, pixels: pixels, alpha: .last)
}

private func rgbaImage(width: UInt32, height: UInt32, pixels: Data, alpha: CGImageAlphaInfo)
    -> Image?
{
    guard let space = CGColorSpace(name: CGColorSpace.sRGB),
        let provider = CGDataProvider(data: pixels as CFData),
        let image = CGImage(
            width: Int(width), height: Int(height), bitsPerComponent: 8,
            bitsPerPixel: 32, bytesPerRow: Int(width) * 4, space: space,
            bitmapInfo: CGBitmapInfo(rawValue: alpha.rawValue),
            provider: provider, decode: nil, shouldInterpolate: true, intent: .defaultIntent)
    else { return nil }
    return Image(decorative: image, scale: 1)
}

/// The confirmed action keeps the revision of the list on which the user chose its index.
struct ProfilePhotoRemoval: Sendable {
    let index: UInt32
    let revision: Data
}

/// The editor's own photos. They are draft writes, so a change also ends a prepared proof.
struct ProfilePhotosSection: View {
    private static let limit = 6
    let candidate: IdentityCandidate
    let peerId: String
    let disabled: Bool
    @Binding var isEditing: Bool
    let changed: () -> Void
    @State private var rows = ProfilePhotoRows(
        hashes: [], previews: [:], revision: Data(), conflict: false)
    @State private var busy = false
    @State private var status = ""
    @State private var picking = false
    @State private var removing: ProfilePhotoRemoval?

    private var isFull: Bool { rows.hashes.count >= Self.limit }
    private var locked: Bool { disabled || busy || isEditing }

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
                    .task(id: isEditing) {
                        guard !isEditing else { return }
                        await apply(edits: false) { dir, key in
                            try profilePhotoSnapshot(dir: dir, key: key)
                        }
                    }
                    .confirmationDialog(
                        "profile.photoRemoveTitle",
                        isPresented: Binding(
                            get: { removing != nil },
                            set: { if !$0 { removing = nil } }
                        ), titleVisibility: .visible
                    ) {
                        removeButtons
                    } message: {
                        Text("profile.photoRemoveText")
                    }
            )
            Text(LocalizedStringKey(isFull ? "profile.photoLimit" : "profile.photoHint")).font(
                .caption)
            if !status.isEmpty {
                Text(LocalizedStringKey(status)).accessibilityIdentifier("profile-photo-status")
            }
        }
    }

    @ViewBuilder private var removeButtons: some View {
        Button("profile.photoRemoveAction", role: .destructive) {
            guard let removal = removing else { return }
            removing = nil
            Task {
                await apply(edits: true) { dir, key in
                    try editProfilePhotos(
                        dir: dir, key: key, expectedRevision: removal.revision,
                        edit: .remove(index: removal.index))
                }
            }
        }
        Button("search.cancel", role: .cancel) {}
    }

    private func tile(_ index: Int) -> some View {
        VStack(spacing: 4) {
            Group {
                if let preview = rows.previews[rows.hashes[index]],
                    let image = previewImage(preview)
                {
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
                control(
                    "arrow.left", text("profile.photoLeft", index + 1), "left-\(index)",
                    enabled: index > 0
                ) {
                    move(index, to: index - 1)
                }
                control(
                    "arrow.right", text("profile.photoRight", index + 1), "right-\(index)",
                    enabled: index + 1 < rows.hashes.count
                ) { move(index, to: index + 1) }
                control(
                    "xmark", text("profile.photoRemove", index + 1), "remove-\(index)",
                    enabled: true
                ) { removing = ProfilePhotoRemoval(index: UInt32(index), revision: rows.revision) }
            }
        }
    }

    private func text(_ key: String, _ numbers: CVarArg...) -> String {
        String(format: NSLocalizedString(key, comment: ""), arguments: numbers)
    }

    private func control(
        _ symbol: String, _ label: String, _ tag: String, enabled: Bool,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) { Image(systemName: symbol) }
            .buttonStyle(.borderless)
            .disabled(locked || !enabled)
            .accessibilityLabel(label)
            .accessibilityIdentifier("profile-photo-\(tag)")
    }

    private func move(_ from: Int, to: Int) {
        let expected = rows.revision
        Task {
            await apply(edits: true) { dir, key in
                try editProfilePhotos(
                    dir: dir, key: key, expectedRevision: expected,
                    edit: .move(from: UInt32(from), to: UInt32(to)))
            }
        }
    }

    private func addTapped() {
        #if DEBUG
            if ProcessInfo.processInfo.arguments.contains("--ui-test-pick-photo") {
                let red = CGFloat(rows.hashes.count + 1) / CGFloat(Self.limit)
                Task {
                    await add {
                        try PhotoPrep.samplePNG(width: 640, height: 480, red: red, alpha: 0.5)
                    }
                }
                return
            }
        #endif
        picking = true
    }

    private func add(_ read: @escaping @Sendable () throws -> Data) async {
        status = "profile.photoWorking"
        let selected = candidate
        let peer = peerId
        await perform(edits: true) {
            // Decode/encode before draft serialization and native admission, never while holding either.
            let photo = try PhotoPrep.prepare(read())
            return try RustBridge.changeProfilePhotos(for: selected, peerId: peer) { dir, key in
                _ = try addProfilePhoto(
                    dir: dir, key: key, full: photo.full, thumbnail: photo.thumbnail)
                return try profilePhotoSnapshot(dir: dir, key: key)
            }
        }
    }

    private func apply(
        edits: Bool,
        _ change: @escaping @Sendable (String, IdentityKey) throws -> ProfilePhotoSnapshot
    ) async {
        let selected = candidate
        let peer = peerId
        await perform(edits: edits) {
            try RustBridge.changeProfilePhotos(for: selected, peerId: peer, change: change)
        }
    }

    private func perform(
        edits: Bool, _ work: @escaping @Sendable () throws -> ProfilePhotoRows
    ) async {
        guard !busy, !isEditing, !Task.isCancelled, !edits || !disabled else { return }
        busy = true
        if edits {
            isEditing = true
            changed()
        }
        defer {
            busy = false
            if edits { isEditing = false }
        }
        let result = await IdentityIO.performProfile { Result { try work() } }
        guard edits || !Task.isCancelled else { return }
        switch result {
        case let .success(fresh):
            rows = fresh
            // A form row that scrolls back into view lists again and must keep the last status.
            if edits {
                status = fresh.conflict ? "profile.photoConflict" : "profile.photoChanged"
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
            content.fileImporter(isPresented: $picking, allowedContentTypes: [.jpeg, .png]) {
                result in
                guard case let .success(url) = result else { return }
                Task { await add { try PhotoPrep.read(url) } }
            }
        #endif
    }
}

#if os(iOS)
    /// The system picker runs out of process and needs no photo library permission.
    struct PhotoPicker: UIViewControllerRepresentable {
        final class Coordinator: NSObject, PHPickerViewControllerDelegate {
            private let picked: @MainActor @Sendable (Data?) -> Void

            init(picked: @escaping @MainActor @Sendable (Data?) -> Void) {
                self.picked = picked
            }

            func picker(
                _ picker: PHPickerViewController, didFinishPicking results: [PHPickerResult]
            ) {
                let picked = picked
                guard let provider = results.first?.itemProvider else {
                    Task { @MainActor in picked(nil) }
                    return
                }
                _ = provider.loadDataRepresentation(forTypeIdentifier: UTType.image.identifier) {
                    data, _ in
                    Task { @MainActor in picked(data) }
                }
            }
        }

        let picked: @MainActor @Sendable (Data?) -> Void

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
