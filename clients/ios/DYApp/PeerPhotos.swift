import ImageIO
import SwiftUI
import UniformTypeIdentifiers

/// Crosses back from a detached decode; nothing else holds the image by then.
private struct DecodedPhoto: @unchecked Sendable {
    let image: Image?
}

/// Decodes at most 2048 px on the long side. Clients publish JPEG only, so other formats never
/// reach ImageIO's many decoders; the core already held the frame to 64 megapixels.
func fullPhotoImage(_ bytes: Data) -> Image? {
    let options = [
        kCGImageSourceCreateThumbnailFromImageAlways: true,
        kCGImageSourceCreateThumbnailWithTransform: true,
        kCGImageSourceShouldCacheImmediately: true,
        kCGImageSourceThumbnailMaxPixelSize: 2_048,
    ] as CFDictionary
    guard let source = CGImageSourceCreateWithData(bytes as CFData, nil),
          CGImageSourceGetType(source) as String? == UTType.jpeg.identifier,
          let image = CGImageSourceCreateThumbnailAtIndex(source, 0, options)
    else { return nil }
    return Image(decorative: image, scale: 1)
}

/// Other people's photos in the searcher's memory cache; see docs/design/profile-photos.md. A list
/// fetches its photos in one call, then each view decodes from the cache and holds only its image.
@MainActor
final class PeerPhotos: ObservableObject {
    /// Counts finished fetches, so a photo shown before one reads the cache again after it.
    @Published private(set) var fetched = 0
    private(set) var searcher: ProfileSearcher?
    private var identity: ChatIdentity?

    /// Another identity starts with a searcher, and so a cache, of its own.
    func use(_ identity: ChatIdentity) {
        if self.identity?.peerId != identity.peerId {
            searcher = nil
            fetched += 1
        }
        self.identity = identity
        if searcher == nil { searcher = try? RustBridge.profileSearcher(for: identity.candidate) }
    }

    /// Fetches what the cache lacks; a failure leaves the letters, and a later view tries again.
    func fetch(_ hashes: [Data], full: Bool) async {
        let wanted = hashes.filter { !$0.isEmpty }
        guard let searcher, !wanted.isEmpty else { return }
        await Task.detached(priority: .utility) { _ = try? searcher.fetchPeerPhotos(hashes: wanted, full: full) }.value
        fetched += 1
    }

    /// The cached photo, or nil when it is missing or fails its checks. The core decodes thumbnails,
    /// so a crafted one never reaches the platform decoder.
    func decoded(_ hash: Data?, full: Bool) async -> Image? {
        guard let searcher, let hash, !hash.isEmpty else { return nil }
        return await Task.detached(priority: .userInitiated) {
            guard let bytes = try? searcher.peerPhoto(hash: hash) else { return DecodedPhoto(image: nil) }
            if full { return DecodedPhoto(image: fullPhotoImage(bytes)) }
            return DecodedPhoto(image: (try? checkPreview(bytes: bytes)).flatMap(previewImage))
        }.value.image
    }

    /// Whether full images load without a tap now.
    func auto() async -> Bool {
        guard let identity else { return false }
        return await Task.detached(priority: .utility) {
            (try? RustBridge.peerPhotosLoadByThemselves(identity)) ?? false
        }.value
    }
}

/// Which photo a view shows, and the fetch it last read the cache after.
private struct PhotoRead: Equatable {
    let hash: Data?
    let fetched: Int
}

/// A peer's main photo once the cache has it, else the letters. The row around it names the peer.
struct PeerAvatar: View {
    @ObservedObject var photos: PeerPhotos
    let peer: String
    let hash: Data?
    @State private var image: Image?

    var body: some View {
        ZStack {
            Color.secondary.opacity(0.2)
            if let image {
                image.resizable().scaledToFill()
            } else {
                Text(verbatim: String(peer.prefix(2))).font(.caption)
            }
        }
        .frame(width: 40, height: 40)
        .clipShape(Circle())
        .accessibilityHidden(true)
        .task(id: PhotoRead(hash: hash, fetched: photos.fetched)) {
            let read = await photos.decoded(hash, full: false)
            if !Task.isCancelled { image = read }
        }
    }
}

/// All of a peer's photos: the chosen one large and the thumbnails to choose from. A full image
/// loads by itself only when the core allows it now; otherwise Load full photo asks for it.
struct PeerGallery: View {
    @ObservedObject var photos: PeerPhotos
    let found: FoundProfile
    @State private var chosen = 0
    @State private var auto = false
    @State private var loading = false
    @State private var failed = false
    @State private var small: Image?
    @State private var large: Image?

    private var full: Data? {
        guard found.fullPhotos.indices.contains(chosen), !found.fullPhotos[chosen].isEmpty else { return nil }
        return found.fullPhotos[chosen]
    }

    var body: some View {
        if found.photos.indices.contains(chosen) {
            main
                .task(id: found.peerId) {
                    await photos.fetch(found.photos, full: false)
                    auto = await photos.auto()
                }
                .task(id: auto ? full : nil) {
                    if auto, let full { await photos.fetch([full], full: true) }
                }
                .task(id: PhotoRead(hash: found.photos[chosen], fetched: photos.fetched)) {
                    let read = await photos.decoded(found.photos[chosen], full: false)
                    if !Task.isCancelled { small = read }
                }
                .task(id: PhotoRead(hash: full, fetched: photos.fetched)) {
                    let read = await photos.decoded(full, full: true)
                    if !Task.isCancelled { large = read }
                }
            if found.photos.count > 1 { thumbnails }
            if let full, large == nil, !auto { load(full) }
        }
    }

    private var main: some View {
        let shown = large ?? small
        return ZStack {
            Color.secondary.opacity(0.2)
            if let shown {
                shown.resizable().scaledToFit()
            } else {
                Text(verbatim: String(found.peerId.prefix(2))).font(.title)
            }
        }
        .frame(maxWidth: 320)
        .aspectRatio(1, contentMode: .fit)
        .accessibilityElement(children: .ignore)
        // Only a drawn photo has this label; the letters read as themselves.
        .accessibilityLabel(shown == nil ? String(found.peerId.prefix(2)) : label(chosen))
        .accessibilityIdentifier("view-photo")
    }

    private var thumbnails: some View {
        ScrollView(.horizontal) {
            HStack {
                ForEach(found.photos.indices, id: \.self) { index in
                    Button {
                        chosen = index
                        failed = false
                        large = nil
                    } label: {
                        PeerAvatar(photos: photos, peer: found.peerId, hash: found.photos[index])
                    }
                    // Without a borderless style a Form row fires every button in it.
                    .buttonStyle(.borderless)
                    .accessibilityLabel(label(index))
                    .accessibilityAddTraits(index == chosen ? .isSelected : [])
                    .accessibilityIdentifier("view-photo-\(index)")
                }
            }
        }
    }

    private func label(_ index: Int) -> String {
        String(format: NSLocalizedString("profile.photoLabel", comment: ""), index + 1, found.photos.count)
    }

    @ViewBuilder
    private func load(_ full: Data) -> some View {
        Button(LocalizedStringKey(loading ? "view.photoLoading" : "view.photoLoad")) {
            loading = true
            failed = false
            Task {
                await photos.fetch([full], full: true)
                failed = await photos.decoded(full, full: true) == nil
                loading = false
            }
        }
        .disabled(loading)
        .accessibilityIdentifier("view-photo-load")
        if failed {
            Text("view.photoFailed").font(.caption).accessibilityIdentifier("view-photo-status")
        }
    }
}
