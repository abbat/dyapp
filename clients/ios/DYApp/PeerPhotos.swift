import ImageIO
import Network
import SwiftUI
import UniformTypeIdentifiers

/// Tells the core whether this connection costs money, at launch and on every change, so full
/// images load by themselves only where that is free. Until the first report it counts as metered.
enum NetworkType {
    private static let monitor = NWPathMonitor()

    static func startReporting() {
        #if DEBUG
            // A runner's network must not decide whether a test sees the offer to load a photo.
            let arguments = ProcessInfo.processInfo.arguments
            if arguments.contains("--ui-test-metered-network") { return }
            if arguments.contains("--ui-test-unmetered-network") {
                setNetworkMetered(metered: false)
                return
            }
        #endif
        monitor.pathUpdateHandler = { path in
            setNetworkMetered(metered: path.isExpensive || path.isConstrained)
        }
        monitor.start(queue: DispatchQueue(label: "com.dyapp.network-type"))
    }
}

/// Crosses back from a detached decode; nothing else holds the image by then.
private struct DecodedPhoto: @unchecked Sendable {
    let image: Image?
}

/// Decodes at most 2048 px on the long side. Clients publish JPEG only, so other formats never
/// reach ImageIO's many decoders; the core already held the frame to 64 megapixels.
func fullPhotoImage(_ bytes: Data) -> Image? {
    let options =
        [
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

/// An owned presentation handle; native methods synchronize their own work.
struct PeerPhotoWorker: @unchecked Sendable {
    let owner: UUID
    let searcher: ProfileSearcher
}

private struct PreparedPeerPhotos: @unchecked Sendable {
    let searcher: ProfileSearcher?
}

/// A separate native memory cache per exact slot and Peer ID, prepared off the UI actor.
@MainActor
final class PeerPhotos: ObservableObject {
    @Published private(set) var fetched = 0
    private var identity: ChatIdentity?
    private var owner = UUID()
    private var worker: PeerPhotoWorker?
    private var preparing: Task<PreparedPeerPhotos, Never>?
    private let makeSearcher: @Sendable (ChatIdentity) throws -> ProfileSearcher

    init(
        makeSearcher: @escaping @Sendable (ChatIdentity) throws -> ProfileSearcher = {
            try RustBridge.profileSearcher(for: $0.candidate, peerId: $0.peerId)
        }
    ) {
        self.makeSearcher = makeSearcher
    }

    func reset() {
        owner = UUID()
        identity = nil
        worker = nil
        preparing = nil
        fetched += 1
    }

    /// Concurrent readers share actual preparation; retiring a view cannot retarget its completion.
    @discardableResult
    func use(_ identity: ChatIdentity) async -> PeerPhotoWorker? {
        guard !Task.isCancelled else { return nil }
        if self.identity?.peerId != identity.peerId
            || self.identity?.candidate.location != identity.candidate.location
        {
            reset()
            self.identity = identity
        }
        if let worker { return worker }
        let preparation: Task<PreparedPeerPhotos, Never>
        if let preparing {
            preparation = preparing
        } else {
            // A failed preparation's late waiter cannot consume a newer retry in the same slot.
            owner = UUID()
            let makeSearcher = makeSearcher
            preparation = Task {
                await IdentityIO.performBlocking {
                    PreparedPeerPhotos(searcher: try? makeSearcher(identity))
                }
            }
            preparing = preparation
        }
        let ticket = owner
        let result = await preparation.value
        guard owner == ticket, !Task.isCancelled else { return nil }
        if preparing != nil {
            preparing = nil
            worker = result.searcher.map { PeerPhotoWorker(owner: ticket, searcher: $0) }
            fetched += 1
        }
        return worker
    }

    private func currentWorker() async -> PeerPhotoWorker? {
        guard let identity else { return nil }
        return await use(identity)
    }

    /// Fetch what is missing, without letting an obsolete completion refresh another identity.
    func fetch(_ hashes: [Data], full: Bool) async {
        let wanted = hashes.filter { !$0.isEmpty }
        guard !wanted.isEmpty, let worker = await currentWorker(), worker.owner == owner else {
            return
        }
        await IdentityIO.performBlocking {
            _ = try? worker.searcher.fetchPeerPhotos(hashes: wanted, full: full)
        }
        guard worker.owner == owner, !Task.isCancelled else { return }
        fetched += 1
    }

    /// Only core-checked thumbnails or bounded native decoding reach the current presentation.
    func decoded(_ hash: Data?, full: Bool) async -> Image? {
        guard let hash, !hash.isEmpty, let worker = await currentWorker(), worker.owner == owner
        else { return nil }
        let decoded = await IdentityIO.performBlocking {
            guard let bytes = try? worker.searcher.peerPhoto(hash: hash) else {
                return DecodedPhoto(image: nil)
            }
            if full { return DecodedPhoto(image: fullPhotoImage(bytes)) }
            return DecodedPhoto(image: (try? checkPreview(bytes: bytes)).flatMap(previewImage))
        }
        guard worker.owner == owner, !Task.isCancelled else { return nil }
        return decoded.image
    }

    func auto() async -> Bool {
        guard let identity else { return false }
        let ticket = owner
        let automatic = await IdentityIO.performBlocking {
            (try? RustBridge.peerPhotosLoadByThemselves(identity)) ?? false
        }
        return ticket == owner && !Task.isCancelled && automatic
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
        guard found.fullPhotos.indices.contains(chosen), !found.fullPhotos[chosen].isEmpty else {
            return nil
        }
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
        String(
            format: NSLocalizedString("profile.photoLabel", comment: ""), index + 1,
            found.photos.count)
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
