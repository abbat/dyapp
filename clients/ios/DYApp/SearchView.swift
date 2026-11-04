import SwiftUI

/// Verified sample search over the active network: the nodes pick the sample, filters appear for the
/// fields they offer, and only profiles the shared core verified are listed.
struct SearchView: View {
    @ObservedObject var session: SearchSession
    let candidate: IdentityCandidate
    let peerId: String
    @ObservedObject var feed: ChatFeed
    @State private var opened: FoundProfile?
    @State private var chat: FoundProfile?

    var body: some View {
        Group {
            if let opened {
                FoundProfileView(
                    peer: opened.peerId, known: opened, feed: feed,
                    back: { self.opened = nil },
                    message: {
                        chat = opened
                        self.opened = nil
                    },
                    blocked: {
                        chat = nil
                        self.opened = nil
                    }
                )
                .id(opened.peerId)
            } else if let chat {
                ChatView(
                    feed: feed, peer: chat.peerId,
                    profile: { opened = chat },
                    back: { self.chat = nil },
                    blocked: { self.chat = nil }
                )
            } else {
                Form {
                    Text("search.sample").font(.caption)
                    SearchFilterFields(inputs: $session.filters)
                    Button("search.run") { session.run() }
                        .disabled(session.state.isLoading)
                        .accessibilityIdentifier("run-search")
                    status
                }
            }
        }
        .navigationTitle("search.title")
        .onAppear { session.open(candidate, peerId: peerId) }
    }

    @ViewBuilder private var status: some View {
        switch session.state {
        case .idle:
            Text("search.idle").font(.caption).accessibilityIdentifier("search-status")

        case .loading:
            Text("search.loading").font(.caption).accessibilityIdentifier("search-status")
            Button("search.cancel") { session.cancel() }
                .accessibilityIdentifier("search-cancel")

        case let .failed(key, details):
            Text(LocalizedStringKey(key)).accessibilityIdentifier("search-status")
            Text(verbatim: details).font(.caption).textSelection(.enabled)
                .accessibilityIdentifier("search-failures")
            if !searchFixTheFields.contains(key) {
                NavigationLink(destination: NetworkDraftView(candidate: candidate, peerId: peerId)) {
                    Text("search.openNetwork")
                }
                .accessibilityIdentifier("search-open-network")
                Button("search.retry") { session.run() }
                    .accessibilityIdentifier("search-retry")
            }

        case let .done(result):
            SearchResults(result: result, photos: feed.photos, open: { opened = $0 }, again: { session.run() })
        }
    }
}

/// Typed failures with each verified profile; an empty sample points at the filters instead of
/// offering another one.
private struct SearchResults: View {
    let result: ProfileSearchResult
    let photos: PeerPhotos
    let open: (FoundProfile) -> Void
    let again: () -> Void

    private var mainPhotos: [Data] { result.profiles.compactMap { $0.photos.first } }

    var body: some View {
        Text(verbatim: searchCountText(result))
            .accessibilityIdentifier("search-status")
            .task(id: mainPhotos) { await photos.fetch(mainPhotos, full: false) }
        if result.profiles.isEmpty { Text("search.emptyText").font(.caption) }
        ForEach(Array(result.failures.enumerated()), id: \.offset) { _, failure in
            Text(verbatim: searchFailureText(failure)).font(.caption)
                .accessibilityIdentifier("search-failures")
        }
        ForEach(Array(result.profiles.enumerated()), id: \.offset) { index, found in
            Button { open(found) } label: { SearchResultRow(found: found, photos: photos) }
                .buttonStyle(.plain)
                .accessibilityIdentifier("result-\(index + 1)")
        }
        if !result.profiles.isEmpty {
            Button("search.again", action: again)
                .accessibilityIdentifier("search-again")
        }
    }
}

/// One verified profile: the main photo, the short Peer ID, its possible age and country.
struct SearchResultRow: View {
    let found: FoundProfile
    let photos: PeerPhotos

    private var summary: String {
        let draft = ProfileRustDraftStore.values(from: found.draft)
        var parts: [String] = []
        // A profile this client cannot project still lists, under its Peer ID alone.
        if let preview = try? ProfilePublicPreview(draft: draft.values, country: draft.country),
           let youngest = preview.youngest, let oldest = preview.oldest {
            parts.append(youngest == oldest ? String(youngest) : "\(youngest)-\(oldest)")
        }
        if !draft.country.isEmpty { parts.append(countryName(draft.country)) }
        return parts.isEmpty ? NSLocalizedString("profile.noPublicFields", comment: "") : parts.joined(separator: ", ")
    }

    var body: some View {
        HStack {
            PeerAvatar(photos: photos, peer: found.peerId, hash: found.photos.first)
            VStack(alignment: .leading) {
                Text(verbatim: "@\(found.peerId.suffix(6))").font(.headline)
                Text(verbatim: summary).font(.caption)
            }
        }
    }
}

/// A rate limit names the wait in whole seconds, rounded up.
func searchFailureText(_ failure: ProfileSearchFailure) -> String {
    switch failure {
    case .noNodes:
        return NSLocalizedString("search.failureNoNodes", comment: "")

    case .unavailable:
        return NSLocalizedString("search.failureUnavailable", comment: "")

    case .timeout:
        return NSLocalizedString("search.failureTimeout", comment: "")

    case let .rateLimited(retryAfterMs):
        return String(format: NSLocalizedString("search.failureRateLimited", comment: ""),
                      String((retryAfterMs + 999) / 1_000))

    case .puzzle:
        return NSLocalizedString("search.failurePuzzle", comment: "")

    case .invalid:
        return NSLocalizedString("search.failureInvalid", comment: "")
    }
}

/// Filters the last sample offered, the optional scales behind a switch, and the local employer filter.
private struct SearchFilterFields: View {
    @Binding var inputs: SearchFilterInputs

    var body: some View {
        if inputs.ageOffered {
            TextField("search.ageMin", text: $inputs.ageMin).accessibilityIdentifier("search-age-min")
            TextField("search.ageMax", text: $inputs.ageMax).accessibilityIdentifier("search-age-max")
        }
        if inputs.heightOffered {
            TextField("search.heightMin", text: $inputs.heightMin).accessibilityIdentifier("search-height-min")
            TextField("search.heightMax", text: $inputs.heightMax).accessibilityIdentifier("search-height-max")
        }
        if inputs.countryOffered {
            PlacePicker(country: $inputs.country, place: $inputs.place, text: $inputs.placeText, disabled: false,
                        idPrefix: "search", anyLabel: "search.any", placeShown: inputs.placeOffered)
        }
        if inputs.mutualOffered {
            Toggle("search.mutual", isOn: $inputs.mutual).accessibilityIdentifier("search-mutual")
            Text("search.mutualHint").font(.caption)
        }
        if !inputs.shownScales.isEmpty {
            // A value picked earlier still applies while the switch is off.
            Toggle("search.more", isOn: $inputs.moreOpen).accessibilityIdentifier("search-more")
            if inputs.moreOpen { scales }
        }
        Picker("search.employer", selection: $inputs.employerKind) {
            Text("search.employerAny").tag(SearchEmployerKind.any)
            Text("search.employerSame").tag(SearchEmployerKind.same)
            Text("search.employerExclude").tag(SearchEmployerKind.exclude)
        }
        .accessibilityIdentifier("search-employer")
        TextField("search.company", text: $inputs.company).accessibilityIdentifier("search-company")
        Text("search.employerNotice").font(.caption).accessibilityIdentifier("search-employer-notice")
    }

    private var scales: some View {
        ForEach(inputs.shownScales, id: \.field) { scale in
            Picker(LocalizedStringKey("profile.field." + scale.key), selection: Binding(
                get: { inputs.picked[scale.key] ?? 0 },
                set: { inputs.picked[scale.key] = $0 }
            )) {
                Text("search.any").tag(UInt32(0))
                ForEach(scale.choices, id: \.code) { choice in
                    Text(LocalizedStringKey("profile.value." + scale.key + "." + choice.key))
                        .tag(choice.code)
                }
            }
            .accessibilityIdentifier("search-field-" + scale.key)
        }
    }
}
