import SwiftUI

private enum ProfileDraftLoad: Sendable {
    case loaded([String: UInt32]?)
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

/// An exact public projection of the saved draft. Zero means unpublished for every scale.
struct ProfilePublicPreview {
    let fields: [ProfilePublicField]
    let birth: ProfileBirthParts
    let youngest: UInt32?
    let oldest: UInt32?

    init(draft: [String: UInt32]) throws {
        var selected: [ProfilePublicField] = []
        let scales = profileScales()
        let known = Set(scales.map(\.key)).union(ProfileBirthParts.keys)
        guard draft.keys.allSatisfy(known.contains) else { throw CocoaError(.fileReadCorruptFile) }
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
        youngest = assessment.youngest
        oldest = assessment.oldest
    }
}

/// A local draft editor. The shared Rust core supplies every numeric choice.
struct ProfileDraftView: View {
    let candidate: IdentityCandidate
    let peerId: String
    @State private var values: [String: UInt32] = [:]
    @State private var birthYear = ""
    @State private var birthMonth = ""
    @State private var birthDay = ""
    @State private var status = ""
    @State private var readFailed = false
    @State private var isLoading = true
    @State private var isSaving = false

    var body: some View {
        Form {
            ForEach(profileScales(), id: \.field) { scale in
                Picker(LocalizedStringKey("profile.field.\(scale.key)"), selection: Binding(
                    get: { values[scale.key] ?? 0 },
                    set: { values[scale.key] = $0 }
                )) {
                    Text("profile.unset").tag(UInt32(0))
                    ForEach(scale.choices, id: \.code) { choice in
                        Text(LocalizedStringKey("profile.value.\(scale.key).\(choice.key)"))
                            .tag(choice.code)
                    }
                }
                .disabled(isLoading || readFailed || isSaving)
            }
            TextField("profile.birthYear", text: $birthYear)
                .disabled(isLoading || readFailed || isSaving)
                .accessibilityIdentifier("profile-birth-year")
            TextField("profile.birthMonth", text: $birthMonth)
                .disabled(isLoading || readFailed || isSaving)
                .accessibilityIdentifier("profile-birth-month")
            TextField("profile.birthDay", text: $birthDay)
                .disabled(isLoading || readFailed || isSaving)
                .accessibilityIdentifier("profile-birth-day")
            Button("profile.save") { Task { await save() } }
                .disabled(isLoading || readFailed || isSaving)
                .accessibilityIdentifier("profile-save")
            NavigationLink("profile.preview") {
                ProfilePreviewView(candidate: candidate, peerId: peerId)
            }
            .disabled(isLoading || isSaving)
            .accessibilityIdentifier("profile-preview")
            if !status.isEmpty {
                Text(LocalizedStringKey(status))
            }
            if isLoading { Text("profile.loading") }
            if isSaving { Text("profile.saving") }
        }
        .navigationTitle("profile.title")
        .task(id: candidate.location.marker) { await load() }
    }

    @MainActor
    private func load() async {
        isLoading = true
        readFailed = false
        status = ""
        values = [:]
        birthYear = ""
        birthMonth = ""
        birthDay = ""
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
                values = decoded.filter { !ProfileBirthParts.keys.contains($0.key) }
                let birth = ProfileBirthParts(draft: decoded)
                birthYear = birth.year.map(String.init) ?? ""
                birthMonth = birth.month.map(String.init) ?? ""
                birthDay = birth.day.map(String.init) ?? ""
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
        do {
            let birth = try ProfileBirthParts(yearText: birthYear, monthText: birthMonth, dayText: birthDay)
            try birth.validate()
            let draft = birth.inserted(into: values)
            let selected = candidate
            let selectedPeer = peerId
            status = ""
            isSaving = true
            let saved = await Task.detached(priority: .userInitiated) {
                (try? RustBridge.storeProfileDraft(for: selected, peerId: selectedPeer, values: draft)) != nil
            }.value
            isSaving = false
            guard !Task.isCancelled else { return }
            status = saved ? "profile.saved" : "profile.saveFailed"
        } catch ProfileBirthError.invalid {
            status = "profile.invalidBirthDate"
        } catch {
            status = "profile.saveFailed"
        }
    }

}

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
                if preview.fields.isEmpty && preview.birth == ProfileBirthParts(year: nil, month: nil, day: nil) {
                    Text("profile.noPublicFields")
                }
                ForEach(preview.fields, id: \.scaleKey) { field in
                    HStack {
                        Text(LocalizedStringKey("profile.field.\(field.scaleKey)"))
                        Spacer()
                        Text(LocalizedStringKey("profile.value.\(field.scaleKey).\(field.choiceKey)"))
                    }
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
        HStack {
            Text(LocalizedStringKey(key))
            Spacer()
            Text(verbatim: String(value))
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
                    preview = try ProfilePublicPreview(draft: draft)
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
