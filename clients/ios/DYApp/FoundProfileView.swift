import SwiftUI

/// The public fields of one verified search result. `SearchView` swaps it in for its form, because a
/// navigation link in the macOS 12 detail column would leave no way back, and keeps the sample.
struct FoundProfileView: View {
    let found: FoundProfile
    let back: () -> Void

    private var draft: ProfileDraft { ProfileRustDraftStore.values(from: found.draft) }

    var body: some View {
        Form {
            Button("view.back", action: back).accessibilityIdentifier("view-back")
            Text(verbatim: "@\(found.peerId.suffix(6))").font(.headline)
            Text(verbatim: String(format: NSLocalizedString("search.peer", comment: ""), found.peerId))
                .font(.caption)
                .textSelection(.enabled)
                .accessibilityIdentifier("view-peer")
            // A profile this client cannot project stays under its Peer ID alone, as in the results.
            if let preview = try? ProfilePublicPreview(draft: draft.values, country: draft.country) {
                ProfilePublicRows(preview: preview, draft: draft)
            } else {
                Text("profile.noPublicFields")
            }
        }
    }
}
