import SwiftUI

/// Private save state is independent of Like and remains usable when the public profile is absent.
struct ProfileFavoriteBlock: View {
    @ObservedObject var page: ProfilePage

    var body: some View {
        let expected = page.opening
        Button(LocalizedStringKey(page.favorite.label)) {
            Task { await page.resolveFavorite(change: true, opening: expected) }
        }
        .disabled(page.favorite.busy || (page.favorite.saved == nil && page.favorite.status == nil))
        .accessibilityIdentifier("view-favorite-toggle")
        Text("favorites.hint").font(.caption)
        if let status = page.favorite.status {
            Text(LocalizedStringKey(status)).font(.caption)
                .accessibilityIdentifier("view-favorite-status")
        }
    }
}
