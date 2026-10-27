import SwiftUI

/// Reads the saved draft so unsaved editor changes cannot appear as public data.
struct ProfilePreviewView: View {
    let candidate: IdentityCandidate
    let peerId: String
    @State private var preview: ProfilePublicPreview?
    @State private var readFailed = false
    @State private var isLoading = true

    var body: some View {
        Form {
            if let preview {
                if preview.fields.isEmpty && preview.country.isEmpty &&
                    preview.birth == ProfileBirthParts(year: nil, month: nil, day: nil) {
                    Text("profile.noPublicFields")
                }
                ForEach(preview.fields, id: \.scaleKey) { field in
                    HStack {
                        Text(LocalizedStringKey("profile.field.\(field.scaleKey)"))
                        Spacer()
                        Text(LocalizedStringKey("profile.value.\(field.scaleKey).\(field.choiceKey)"))
                    }
                }
                if !preview.country.isEmpty {
                    row("profile.country", value: countryName(preview.country))
                        .accessibilityElement(children: .combine)
                        .accessibilityIdentifier("preview-country")
                }
                if let place = preview.place.flatMap({
                    placeName(country: preview.country, id: $0, lang: placeLang())
                }) {
                    row("profile.place", value: placeLabel(place))
                        .accessibilityElement(children: .combine)
                        .accessibilityIdentifier("preview-place")
                }
                if let year = preview.birth.year { birthRow("profile.birthYear", value: year) }
                if let month = preview.birth.month { birthRow("profile.birthMonth", value: month) }
                if let day = preview.birth.day { birthRow("profile.birthDay", value: day) }
                HStack {
                    Text("profile.age")
                    Spacer()
                    if let youngest = preview.youngest, let oldest = preview.oldest {
                        if youngest == oldest {
                            Text(verbatim: String(youngest))
                        } else {
                            Text(verbatim: "\(youngest)-\(oldest)")
                        }
                    } else {
                        Text("profile.ageUnknown")
                    }
                }
                Text("profile.publicDisclosure")
                if preview.birth.year != nil && preview.birth.month != nil && preview.birth.day != nil {
                    Text("profile.fullDateDisclosure")
                }
            } else if readFailed {
                Text("profile.readFailed")
            } else if isLoading {
                Text("profile.loading")
            } else {
                Text("profile.noSavedDraft")
            }
        }
        .navigationTitle("profile.preview")
        .task(id: candidate.location.marker) { await load() }
    }

    private func birthRow(_ key: String, value: UInt32) -> some View {
        row(key, value: String(value))
    }

    private func row(_ key: String, value: String) -> some View {
        HStack {
            Text(LocalizedStringKey(key))
            Spacer()
            Text(verbatim: value)
        }
    }

    @MainActor
    private func load() async {
        isLoading = true
        preview = nil
        readFailed = false
        let selected = candidate
        let selectedPeer = peerId
        let result = await Task.detached(priority: .userInitiated) { () -> ProfileDraftLoad in
            do {
                return .loaded(try RustBridge.loadProfileDraft(for: selected, peerId: selectedPeer))
            } catch {
                return .failed
            }
        }.value
        guard !Task.isCancelled else { return }
        switch result {
        case .loaded(let draft):
            if let draft {
                do {
                    preview = try ProfilePublicPreview(draft: draft.values, country: draft.country)
                } catch {
                    readFailed = true
                }
            }

        case .failed:
            readFailed = true
        }
        isLoading = false
    }
}
