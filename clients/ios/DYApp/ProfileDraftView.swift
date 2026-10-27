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
        let days = UInt64(max(0, floor(Date().timeIntervalSince1970 / 86_400)))
        guard try assessBirthParts(year: year, month: month, day: day, todayUnixDays: days).valid else {
            throw ProfileBirthError.invalid
        }
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

    private var draftStore: ProfileDraftStore {
        get throws { ProfileDraftStore(root: try RustBridge.directory(), candidate: candidate, peerId: peerId) }
    }

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
            }
            TextField("profile.birthYear", text: $birthYear)
                .accessibilityIdentifier("profile-birth-year")
            TextField("profile.birthMonth", text: $birthMonth)
                .accessibilityIdentifier("profile-birth-month")
            TextField("profile.birthDay", text: $birthDay)
                .accessibilityIdentifier("profile-birth-day")
            Button("profile.save") { save() }
                .disabled(readFailed)
                .accessibilityIdentifier("profile-save")
            if !status.isEmpty {
                Text(LocalizedStringKey(status))
            }
        }
        .navigationTitle("profile.title")
        .onAppear(perform: load)
    }

    private func load() {
        do {
            guard let decoded = try draftStore.load() else { return }
            for (key, value) in decoded where !ProfileBirthParts.keys.contains(key) {
                guard let scale = profileScales().first(where: { $0.key == key }),
                      value == 0 || scale.choices.contains(where: { $0.code == value }) else {
                    throw CocoaError(.fileReadCorruptFile)
                }
            }
            let birth = ProfileBirthParts(draft: decoded)
            try birth.validate()
            values = decoded.filter { !ProfileBirthParts.keys.contains($0.key) }
            birthYear = birth.year.map(String.init) ?? ""
            birthMonth = birth.month.map(String.init) ?? ""
            birthDay = birth.day.map(String.init) ?? ""
        } catch {
            readFailed = true
            status = "profile.readFailed"
        }
    }

    private func save() {
        do {
            let birth = try ProfileBirthParts(yearText: birthYear, monthText: birthMonth, dayText: birthDay)
            try draftStore.save(values, birth: birth)
            status = "profile.saved"
        } catch ProfileBirthError.invalid {
            status = "profile.invalidBirthDate"
        } catch {
            status = "profile.saveFailed"
        }
    }

}
