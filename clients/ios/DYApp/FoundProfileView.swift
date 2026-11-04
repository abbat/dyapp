import SwiftUI

/// One peer's page: the public fields, the likes and the quiet actions. The lists swap it in for
/// their form, because a navigation link in the macOS 12 detail column would leave no way back.
/// Your own profile has no likes or actions.
struct FoundProfileView: View {
    @ObservedObject var feed: ChatFeed
    @StateObject private var page: ProfilePage
    let back: () -> Void
    let message: () -> Void
    let blocked: () -> Void

    private var own: Bool { page.peer == feed.me }

    var body: some View {
        ScrollingForm {
            Button("view.back", action: back).accessibilityIdentifier("view-back")
            Text(verbatim: "@\(page.peer.suffix(6))").font(.headline)
            Text(verbatim: String(format: NSLocalizedString("search.peer", comment: ""), page.peer))
                .font(.caption)
                .textSelection(.enabled)
                .accessibilityIdentifier("view-peer")
            if let found = page.found { PeerGallery(photos: feed.photos, found: found) }
            if !own { ProfileLikesBlock(page: page, message: message) }
            fields
            if !own { ProfileQuietActions(page: page, blocked: blocked) }
        }
        .task { await page.load() }
        // The feed counts its reads, so a like that a receive brought shows here.
        .task(id: feed.version) { if !own { await page.readLikes() } }
    }

    @ViewBuilder private var fields: some View {
        if let found = page.found {
            // A profile this client cannot project stays under its Peer ID alone, as in the results.
            let draft = ProfileRustDraftStore.values(from: found.draft)
            if let preview = try? ProfilePublicPreview(draft: draft.values, country: draft.country) {
                ProfilePublicRows(preview: preview, draft: draft)
            } else {
                Text("profile.noPublicFields")
            }
        } else if page.loading {
            Text("view.loading").accessibilityIdentifier("view-load")
        } else if let failure = page.loadFailure {
            Text(LocalizedStringKey(failure.key)).accessibilityIdentifier("view-load")
            Text(verbatim: failure.details ?? "").font(.caption).textSelection(.enabled)
                .accessibilityIdentifier("view-load-details")
            Button("search.retry") { Task { await page.load() } }.accessibilityIdentifier("view-retry")
        } else if page.searched {
            Text("view.notFound").accessibilityIdentifier("view-load")
            Button("search.retry") { Task { await page.load() } }.accessibilityIdentifier("view-retry")
        }
    }

    init(
        peer: String, known: FoundProfile?, feed: ChatFeed,
        back: @escaping () -> Void, message: @escaping () -> Void, blocked: @escaping () -> Void
    ) {
        self.feed = feed
        _page = StateObject(wrappedValue: ProfilePage(peer: peer, known: known, feed: feed))
        self.back = back
        self.message = message
        self.blocked = blocked
    }
}
