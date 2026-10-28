import Foundation

/// Scale, birth and place codes by key, and the ISO country code or empty.
struct ProfileDraft: Equatable, Sendable {
    static let placeKey = "place"
    var values: [String: UInt32] = [:]
    var country = ""
}

/// Moves the current identity's JSON draft into the shared Rust store without erasing the source.
struct ProfileRustDraftStore {
    let root: URL
    let candidate: IdentityCandidate
    let peerId: String
    let key: IdentityKey

    private var directory: URL { RustBridge.networkDirectory(root: root, candidate: candidate) }
    private var legacy: ProfileDraftStore {
        ProfileDraftStore(root: root, candidate: candidate, peerId: peerId)
    }

    private static func record(from draft: ProfileDraft) throws -> PublicProfileDraft {
        _ = try ProfilePublicPreview(draft: draft.values, country: draft.country)
        let values = draft.values
        return PublicProfileDraft(
            gender: values["gender"] ?? 0,
            orientation: values["orientation"] ?? 0,
            relationshipGoal: values["relationship_goal"] ?? 0,
            rolePreference: values["role_preference"] ?? 0,
            careerAmbition: values["career_ambition"] ?? 0,
            financialPhilosophy: values["financial_philosophy"] ?? 0,
            employmentStatus: values["employment_status"] ?? 0,
            fitnessLevel: values["fitness_level"] ?? 0,
            zodiac: values["zodiac"] ?? 0,
            birthYear: values["birthYear"],
            birthMonth: values["birthMonth"],
            birthDay: values["birthDay"],
            country: draft.country,
            place: values[ProfileDraft.placeKey].map(String.init) ?? ""
        )
    }

    static func values(from draft: PublicProfileDraft) -> ProfileDraft {
        var values: [String: UInt32] = [:]
        for (key, code) in [
            ("gender", draft.gender),
            ("orientation", draft.orientation),
            ("relationship_goal", draft.relationshipGoal),
            ("role_preference", draft.rolePreference),
            ("career_ambition", draft.careerAmbition),
            ("financial_philosophy", draft.financialPhilosophy),
            ("employment_status", draft.employmentStatus),
            ("fitness_level", draft.fitnessLevel),
            ("zodiac", draft.zodiac),
        ] where code != 0 {
            values[key] = code
        }
        if let place = UInt32(draft.place) { values[ProfileDraft.placeKey] = place }
        let birth = ProfileBirthParts(year: draft.birthYear, month: draft.birthMonth, day: draft.birthDay)
        return ProfileDraft(values: birth.inserted(into: values), country: draft.country)
    }

    func load() throws -> ProfileDraft? {
        if let saved = try loadProfileDraft(dir: directory.path, key: key) {
            let draft = Self.values(from: saved)
            _ = try ProfilePublicPreview(draft: draft.values, country: draft.country)
            return draft
        }
        guard let old = try legacy.load() else { return nil }
        try save(ProfileDraft(values: old))
        return Self.values(from: try Self.record(from: ProfileDraft(values: old)))
    }

    func save(_ draft: ProfileDraft) throws {
        try saveProfileDraft(dir: directory.path, key: key, draft: Self.record(from: draft))
    }
}
