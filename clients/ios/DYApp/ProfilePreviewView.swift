import SwiftUI

enum ProfileBirthError: Error { case invalid }

/// Absent text remains absent in the saved draft; the shared Rust profile core checks the parts.
struct ProfileBirthParts: Equatable {
    static let keys: Set<String> = ["birthYear", "birthMonth", "birthDay"]
    let year: UInt32?
    let month: UInt32?
    let day: UInt32?

    init(year: UInt32?, month: UInt32?, day: UInt32?) {
        self.year = year
        self.month = month
        self.day = day
    }

    init(yearText: String, monthText: String, dayText: String) throws {
        guard let parts = parseProfileBirthParts(year: yearText, month: monthText, day: dayText)
        else { throw ProfileBirthError.invalid }
        self.init(
            year: parts["birth_year"].map(UInt32.init),
            month: parts["birth_month"].map(UInt32.init),
            day: parts["birth_day"].map(UInt32.init))
    }

    init(draft: [String: UInt32]) {
        self.init(year: draft["birthYear"], month: draft["birthMonth"], day: draft["birthDay"])
    }

    func inserted(into draft: [String: UInt32]) -> [String: UInt32] {
        var draft = draft
        for (key, value) in [("birthYear", year), ("birthMonth", month), ("birthDay", day)] {
            draft.removeValue(forKey: key)
            if let value { draft[key] = value }
        }
        return draft
    }

    func validate() throws {
        guard try assessment().valid else { throw ProfileBirthError.invalid }
    }

    func assessment() throws -> BirthAssessment {
        let days = UInt64(max(0, floor(Date().timeIntervalSince1970 / 86_400)))
        return try assessBirthParts(year: year, month: month, day: day, todayUnixDays: days)
    }
}


/// Reads the saved draft so unsaved editor changes cannot appear as public data.
struct ProfilePreviewView: View {
    let candidate: IdentityCandidate
    let peerId: String
    @State private var preview: ProfilePublicPreview?
    @State private var draft = ProfileDraft()
    @State private var readFailed = false
    @State private var isLoading = true

    var body: some View {
        ScrollingForm {
            if let preview {
                ProfilePublicRows(preview: preview, draft: draft)
                Text("profile.publicDisclosure")
                if preview.birth.year != nil && preview.birth.month != nil
                    && preview.birth.day != nil
                {
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
        .task(id: candidate.location.marker + ":" + peerId) { await load() }
    }

    @MainActor
    private func load() async {
        isLoading = true
        preview = nil
        readFailed = false
        let selected = candidate
        let selectedPeer = peerId
        let result = await IdentityIO.performProfile { () -> ProfileDraftLoad in
            do {
                return .loaded(try RustBridge.loadProfileDraft(for: selected, peerId: selectedPeer))
            } catch {
                return .failed
            }
        }
        guard !Task.isCancelled else { return }
        switch result {
        case .loaded(let draft):
            if let draft {
                do {
                    self.draft = draft
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

/// The rows anyone can see of a public profile, the owner's or a found one.
struct ProfilePublicRows: View {
    let preview: ProfilePublicPreview
    let draft: ProfileDraft

    var body: some View {
        if preview.fields.isEmpty && preview.country.isEmpty
            && preview.birth == ProfileBirthParts(year: nil, month: nil, day: nil)
            && ProfileExtraInputs(draft: draft) == ProfileExtraInputs()
        {
            Text("profile.noPublicFields")
        }
        ForEach(preview.fields, id: \.scaleKey) { field in
            HStack {
                // An interpolated key would look up "profile.field.%@"; concatenate instead.
                Text(LocalizedStringKey("profile.field." + field.scaleKey))
                Spacer()
                Text(LocalizedStringKey("profile.value." + field.scaleKey + "." + field.choiceKey))
            }
        }
        ProfileExtraRows(draft: draft)
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
}
