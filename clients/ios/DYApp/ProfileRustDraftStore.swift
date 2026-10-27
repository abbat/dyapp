import Foundation

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

    private static func record(from values: [String: UInt32]) throws -> PublicProfileDraft {
        _ = try ProfilePublicPreview(draft: values)
        return PublicProfileDraft(
            gender: values["gender"] ?? 0,
            orientation: values["orientation"] ?? 0,
            relationshipGoal: values["relationshipGoal"] ?? 0,
            rolePreference: values["rolePreference"] ?? 0,
            careerAmbition: values["careerAmbition"] ?? 0,
            financialPhilosophy: values["financialPhilosophy"] ?? 0,
            employmentStatus: values["employmentStatus"] ?? 0,
            fitnessLevel: values["fitnessLevel"] ?? 0,
            zodiac: values["zodiac"] ?? 0,
            birthYear: values["birthYear"],
            birthMonth: values["birthMonth"],
            birthDay: values["birthDay"]
        )
    }

    private static func values(from draft: PublicProfileDraft) -> [String: UInt32] {
        var values: [String: UInt32] = [:]
        for (key, code) in [
            ("gender", draft.gender),
            ("orientation", draft.orientation),
            ("relationshipGoal", draft.relationshipGoal),
            ("rolePreference", draft.rolePreference),
            ("careerAmbition", draft.careerAmbition),
            ("financialPhilosophy", draft.financialPhilosophy),
            ("employmentStatus", draft.employmentStatus),
            ("fitnessLevel", draft.fitnessLevel),
            ("zodiac", draft.zodiac),
        ] where code != 0 {
            values[key] = code
        }
        return ProfileBirthParts(year: draft.birthYear, month: draft.birthMonth, day: draft.birthDay)
            .inserted(into: values)
    }

    func load() throws -> [String: UInt32]? {
        if let saved = try loadProfileDraft(dir: directory.path, key: key) {
            let values = Self.values(from: saved)
            _ = try ProfilePublicPreview(draft: values)
            return values
        }
        guard let old = try legacy.load() else { return nil }
        try save(old)
        return Self.values(from: try Self.record(from: old))
    }

    func save(_ values: [String: UInt32]) throws {
        try saveProfileDraft(dir: directory.path, key: key, draft: Self.record(from: values))
    }
}
