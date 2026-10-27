import SwiftUI

enum ProfileDraftLoad: Sendable {
    case loaded(ProfileDraft?)
    case failed
}

private enum ProfileEligibility: Sendable {
    case ready
    case blocked(String)
    case failed
}

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
        func part(_ text: String) throws -> UInt32? {
            let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
            if trimmed.isEmpty { return nil }
            guard let value = UInt32(trimmed) else { throw ProfileBirthError.invalid }
            return value
        }
        try self.init(year: part(yearText), month: part(monthText), day: part(dayText))
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

/// Per-identity local editor state; a prior peer-named root draft is copied once on first read.
struct ProfileDraftStore {
    let root: URL
    let candidate: IdentityCandidate
    let peerId: String

    var file: URL {
        RustBridge.networkDirectory(root: root, candidate: candidate)
            .appendingPathComponent("profile-draft.json")
    }

    private var oldFile: URL { root.appendingPathComponent("profile-draft-\(peerId).json") }

    func load() throws -> [String: UInt32]? {
        let files = FileManager.default
        let current = file
        if files.fileExists(atPath: current.path) {
            return try JSONDecoder().decode([String: UInt32].self, from: Data(contentsOf: current))
        }
        guard files.fileExists(atPath: oldFile.path) else { return nil }
        let data = try Data(contentsOf: oldFile)
        let values = try JSONDecoder().decode([String: UInt32].self, from: data)
        try data.write(to: current, options: .atomic)
        return values
    }

    func save(_ values: [String: UInt32]) throws {
        try JSONEncoder().encode(values).write(to: file, options: .atomic)
    }

    func save(_ values: [String: UInt32], birth: ProfileBirthParts) throws {
        try birth.validate()
        try save(birth.inserted(into: values))
    }
}

struct ProfilePublicField: Equatable {
    let scaleKey: String
    let choiceKey: String
}

/// An exact public projection of the saved draft. Zero means unpublished for every scale; a place
/// counts only when the picker offers it in `country`.
struct ProfilePublicPreview {
    let fields: [ProfilePublicField]
    let birth: ProfileBirthParts
    let country: String
    let place: UInt32?
    let youngest: UInt32?
    let oldest: UInt32?

    init(draft: [String: UInt32], country: String = "") throws {
        var selected: [ProfilePublicField] = []
        let scales = profileScales()
        let known = Set(scales.map(\.key)).union(ProfileBirthParts.keys).union([ProfileDraft.placeKey])
        guard draft.keys.allSatisfy(known.contains) else { throw CocoaError(.fileReadCorruptFile) }
        let place = draft[ProfileDraft.placeKey]
        guard country.isEmpty || placeCountries().contains(country),
              place.map({ placeName(country: country, id: $0, lang: "en") != nil }) ?? true
        else { throw CocoaError(.fileReadCorruptFile) }
        for scale in scales {
            let code = draft[scale.key] ?? 0
            guard code != 0 else { continue }
            guard let choice = scale.choices.first(where: { $0.code == code }) else {
                throw CocoaError(.fileReadCorruptFile)
            }
            selected.append(ProfilePublicField(scaleKey: scale.key, choiceKey: choice.key))
        }
        let birth = ProfileBirthParts(draft: draft)
        let assessment = try birth.assessment()
        guard assessment.valid else { throw ProfileBirthError.invalid }
        fields = selected
        self.birth = birth
        self.country = country
        self.place = place
        youngest = assessment.youngest
        oldest = assessment.oldest
    }
}

/// A local draft editor. The shared Rust core supplies every numeric choice.
struct ProfileDraftView: View {
    let candidate: IdentityCandidate
    let peerId: String
    @State private var values: [String: UInt32] = [:]
    @State private var savedValues: [String: UInt32] = [:]
    @State private var country = ""
    @State private var savedCountry = ""
    @State private var placeText = ""
    @State private var birthYear = ""
    @State private var birthMonth = ""
    @State private var birthDay = ""
    @State private var savedBirthYear = ""
    @State private var savedBirthMonth = ""
    @State private var savedBirthDay = ""
    @State private var status = ""
    @State private var readFailed = false
    @State private var isLoading = true
    @State private var isSaving = false
    @State private var hasSavedDraft = false
    @State private var confirmedAdult = false
    @State private var confirmedPublic = false
    @State private var publishBlock: String?
    @State private var eligibilityFailed = false
    @State private var eligibilityChecked = false
    @State private var isCheckingEligibility = false
    @State private var eligibilityRevision = 0
    @State private var profileRevision = 0
    @State private var isSolving = false

    private var hasUnsavedEdits: Bool {
        values != savedValues || country != savedCountry || birthYear != savedBirthYear ||
            birthMonth != savedBirthMonth || birthDay != savedBirthDay
    }

    private var canPrepareProof: Bool {
        hasSavedDraft && !hasUnsavedEdits && confirmedAdult && confirmedPublic && eligibilityChecked &&
            publishBlock == nil && !eligibilityFailed && !isCheckingEligibility &&
            !isLoading && !isSaving && !isSolving && !readFailed
    }

    var body: some View {
        Form {
            ForEach(profileScales(), id: \.field) { scale in
                // An interpolated key would look up "profile.field.%@"; concatenate instead.
                Picker(LocalizedStringKey("profile.field." + scale.key), selection: Binding(
                    get: { values[scale.key] ?? 0 },
                    set: { values[scale.key] = $0 }
                )) {
                    Text("profile.unset").tag(UInt32(0))
                    ForEach(scale.choices, id: \.code) { choice in
                        Text(LocalizedStringKey("profile.value." + scale.key + "." + choice.key))
                            .tag(choice.code)
                    }
                }
                .disabled(isLoading || readFailed || isSaving || isSolving)
                .accessibilityIdentifier("profile-field-" + scale.key)
            }
            PlacePicker(country: $country, place: Binding(
                get: { values[ProfileDraft.placeKey] },
                set: { values[ProfileDraft.placeKey] = $0 }
            ), text: $placeText, disabled: isLoading || readFailed || isSaving || isSolving)
            TextField("profile.birthYear", text: $birthYear)
                .disabled(isLoading || readFailed || isSaving || isSolving)
                .accessibilityIdentifier("profile-birth-year")
            TextField("profile.birthMonth", text: $birthMonth)
                .disabled(isLoading || readFailed || isSaving || isSolving)
                .accessibilityIdentifier("profile-birth-month")
            TextField("profile.birthDay", text: $birthDay)
                .disabled(isLoading || readFailed || isSaving || isSolving)
                .accessibilityIdentifier("profile-birth-day")
            Button("profile.save") { Task { await save() } }
                .disabled(isLoading || readFailed || isSaving || isSolving)
                .accessibilityIdentifier("profile-save")
            NavigationLink("profile.preview") {
                ProfilePreviewView(candidate: candidate, peerId: peerId)
            }
            .disabled(isLoading || isSaving || isSolving)
            .accessibilityIdentifier("profile-preview")
            if !status.isEmpty {
                Text(LocalizedStringKey(status))
                    .accessibilityIdentifier("profile-status")
            }
            if isLoading { Text("profile.loading") }
            if isSaving { Text("profile.saving") }
            Section {
                Toggle("profile.confirmAdult", isOn: $confirmedAdult)
                    .disabled(isLoading || isSaving || isSolving || readFailed || hasUnsavedEdits)
                    .accessibilityIdentifier("profile-confirm-adult")
                Toggle("profile.confirmPublic", isOn: $confirmedPublic)
                    .disabled(isLoading || isSaving || isSolving || readFailed || hasUnsavedEdits)
                    .accessibilityIdentifier("profile-confirm-public")
                Text(eligibilityStatus)
                    .accessibilityIdentifier("profile-publish-eligibility")
                ProfileProofControls(candidate: candidate, peerId: peerId, canPrepare: canPrepareProof,
                                     attested: confirmedAdult, resetToken: profileRevision,
                                     isSolving: $isSolving, clearConfirmations: clearConfirmations)
            }
        }
        .navigationTitle("profile.title")
        .task(id: candidate.location.marker) { await load() }
        .onChange(of: values) { _ in invalidateConsent() }
        .onChange(of: country) { _ in invalidateConsent() }
        .onChange(of: birthYear) { _ in invalidateConsent() }
        .onChange(of: birthMonth) { _ in invalidateConsent() }
        .onChange(of: birthDay) { _ in invalidateConsent() }
        .onChange(of: confirmedAdult) { _ in Task { await checkEligibility() } }
        .onChange(of: confirmedPublic) { _ in Task { await checkEligibility() } }
    }

    private var eligibilityStatus: LocalizedStringKey {
        if !hasSavedDraft { return "profile.saveBeforePublish" }
        if hasUnsavedEdits { return "profile.saveChangesBeforePublish" }
        if isSolving { return "profile.proofWorking" }
        if isCheckingEligibility { return "profile.checkingEligibility" }
        if eligibilityFailed { return "profile.eligibilityFailed" }
        if let publishBlock { return LocalizedStringKey("profile.publishBlocked.\(publishBlock)") }
        if !confirmedAdult { return "profile.publishBlocked.notAttested" }
        if !confirmedPublic { return "profile.confirmPublicRequired" }
        return "profile.eligible"
    }

    @MainActor
    private func load() async {
        isLoading = true
        readFailed = false
        status = ""
        hasSavedDraft = false
        values = [:]
        savedValues = [:]
        country = ""
        savedCountry = ""
        placeText = ""
        birthYear = ""
        birthMonth = ""
        birthDay = ""
        savedBirthYear = ""
        savedBirthMonth = ""
        savedBirthDay = ""
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
        case .loaded(let decoded):
            if let decoded {
                hasSavedDraft = true
                values = decoded.values.filter { !ProfileBirthParts.keys.contains($0.key) }
                country = decoded.country
                savedCountry = country
                placeText = values[ProfileDraft.placeKey]
                    .flatMap { placeName(country: country, id: $0, lang: placeLang()) }.map(placeLabel) ?? ""
                let birth = ProfileBirthParts(draft: decoded.values)
                birthYear = birth.year.map(String.init) ?? ""
                birthMonth = birth.month.map(String.init) ?? ""
                birthDay = birth.day.map(String.init) ?? ""
                savedValues = values
                savedBirthYear = birthYear
                savedBirthMonth = birthMonth
                savedBirthDay = birthDay
            }

        case .failed:
            readFailed = true
            status = "profile.readFailed"
        }
        isLoading = false
    }

    @MainActor
    private func save() async {
        guard !isLoading && !isSaving && !readFailed else { return }
        guard placeText.isEmpty || values[ProfileDraft.placeKey] != nil else {
            status = "profile.placeUnknown"
            return
        }
        do {
            let birth = try ProfileBirthParts(yearText: birthYear, monthText: birthMonth, dayText: birthDay)
            try birth.validate()
            let draft = ProfileDraft(values: birth.inserted(into: values), country: country)
            let selected = candidate
            let selectedPeer = peerId
            status = ""
            isSaving = true
            let saved = await Task.detached(priority: .userInitiated) {
                (try? RustBridge.storeProfileDraft(for: selected, peerId: selectedPeer, draft: draft)) != nil
            }.value
            isSaving = false
            guard !Task.isCancelled else { return }
            status = saved ? "profile.saved" : "profile.saveFailed"
            if saved {
                hasSavedDraft = true
                savedValues = values
                savedCountry = country
                savedBirthYear = birthYear
                savedBirthMonth = birthMonth
                savedBirthDay = birthDay
                invalidateConsent()
            }
        } catch ProfileBirthError.invalid {
            status = "profile.invalidBirthDate"
        } catch {
            status = "profile.saveFailed"
        }
    }

    @MainActor
    private func invalidateConsent() {
        profileRevision += 1
        clearConfirmations()
    }

    @MainActor
    private func clearConfirmations() {
        eligibilityRevision += 1
        confirmedAdult = false
        confirmedPublic = false
        publishBlock = nil
        eligibilityFailed = false
        eligibilityChecked = false
        isCheckingEligibility = false
    }

    @MainActor
    private func checkEligibility() async {
        eligibilityRevision += 1
        let revision = eligibilityRevision
        guard hasSavedDraft && !hasUnsavedEdits && !isLoading && !isSolving && !readFailed else {
            isCheckingEligibility = false
            return
        }
        eligibilityChecked = false
        isCheckingEligibility = true
        let selected = candidate
        let selectedPeer = peerId
        let attested = confirmedAdult
        let result = await Task.detached(priority: .userInitiated) { () -> ProfileEligibility in
            do {
                if let block = try RustBridge.publicationBlock(for: selected, peerId: selectedPeer,
                                                               attested: attested) {
                    return .blocked(block)
                }
                return .ready
            } catch {
                return .failed
            }
        }.value
        guard !Task.isCancelled && revision == eligibilityRevision else { return }
        isCheckingEligibility = false
        eligibilityChecked = true
        switch result {
        case .ready:
            publishBlock = nil
            eligibilityFailed = false

        case .blocked(let block):
            publishBlock = block
            eligibilityFailed = false

        case .failed:
            eligibilityFailed = true
            eligibilityChecked = false
        }
    }
}
