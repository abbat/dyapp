@testable import DYApp
import Foundation
import XCTest

final class ProfileDraftStoreTests: XCTestCase {
    func testRustImportKeepsLegacyJSONAndPrefersClientDatabase() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let slots = IdentitySlotStore(root: root, legacyStore: .forSlot(UUID()))
        let first = try slots.create()
        let candidate = IdentityCandidate(location: .slot(first.slot.id), state: .ready)
        let old = root.appendingPathComponent("profile-draft-\(first.peerId).json")
        try JSONEncoder().encode(["gender": UInt32(1), "birthMonth": 2, "birthDay": 29]).write(to: old)
        let store = ProfileRustDraftStore(root: root, candidate: candidate, peerId: first.peerId,
                                          key: try slots.open(candidate.location))
        let imported = ProfileDraft(values: ["gender": 1, "birthMonth": 2, "birthDay": 29])
        XCTAssertEqual(try store.load(), imported)
        XCTAssertTrue(FileManager.default.fileExists(atPath: old.path))
        XCTAssertTrue(FileManager.default.fileExists(atPath:
            RustBridge.networkDirectory(root: root, candidate: candidate)
                .appendingPathComponent("client.db").path))

        try ProfileDraftStore(root: root, candidate: candidate, peerId: first.peerId).save(["gender": 2])
        XCTAssertEqual(try store.load(), imported)
        try store.save(ProfileDraft(values: ["gender": 3]))
        XCTAssertEqual(try store.load(), ProfileDraft(values: ["gender": 3]))
        XCTAssertTrue(FileManager.default.fileExists(atPath: old.path))
        let goal = ProfileDraft(values: ["gender": 3, "relationship_goal": 1])
        try store.save(goal)
        XCTAssertEqual(try store.load(), goal)
        try store.save(ProfileDraft(values: ["gender": 3]))

        let second = try slots.create()
        let otherCandidate = IdentityCandidate(location: .slot(second.slot.id), state: .ready)
        let other = ProfileRustDraftStore(root: root, candidate: otherCandidate, peerId: second.peerId,
                                          key: try slots.open(otherCandidate.location))
        XCTAssertNil(try other.load())
        try other.save(ProfileDraft())
        XCTAssertEqual(try other.load(), ProfileDraft())
        XCTAssertEqual(try store.load(), ProfileDraft(values: ["gender": 3]))

        // Berlin is GeoNames 2950159; it round-trips only with its own country.
        let berlin = ProfileDraft(values: [ProfileDraft.placeKey: 2_950_159], country: "DE")
        try store.save(berlin)
        XCTAssertEqual(try store.load(), berlin)
        XCTAssertThrowsError(try store.save(ProfileDraft(values: berlin.values, country: "FR")))
        XCTAssertThrowsError(try store.save(ProfileDraft(values: berlin.values)))
        XCTAssertThrowsError(try store.save(ProfileDraft(country: "XX")))
        XCTAssertEqual(try store.load(), berlin)
    }

    func testFailedRustImportLeavesJSONAvailableForRetry() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let slots = IdentitySlotStore(root: root, legacyStore: .forSlot(UUID()))
        let slot = try slots.create()
        let candidate = IdentityCandidate(location: .slot(slot.slot.id), state: .ready)
        let json = ProfileDraftStore(root: root, candidate: candidate, peerId: slot.peerId)
        try json.save(["birthYear": 1_900, "birthMonth": 2, "birthDay": 29])
        let saved = try Data(contentsOf: json.file)
        let rust = ProfileRustDraftStore(root: root, candidate: candidate, peerId: slot.peerId,
                                         key: try slots.open(candidate.location))
        XCTAssertThrowsError(try rust.load())
        XCTAssertEqual(try Data(contentsOf: json.file), saved)
    }

    func testPreviewUsesSavedDraftInsteadOfUnsavedEditorValues() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let slots = IdentitySlotStore(root: root)
        let slot = try slots.create()
        let candidate = IdentityCandidate(location: .slot(slot.slot.id), state: .ready)
        let draft = ProfileDraftStore(root: root, candidate: candidate, peerId: slot.peerId)
        try draft.save(["gender": 1])
        let unsavedEditorValues: [String: UInt32] = ["gender": 2]
        let savedPreview = try ProfilePublicPreview(draft: XCTUnwrap(draft.load()))
        let unsavedPreview = try ProfilePublicPreview(draft: unsavedEditorValues)
        XCTAssertNotEqual(savedPreview.fields, unsavedPreview.fields)
        XCTAssertEqual(savedPreview.fields.first?.scaleKey, "gender")
        XCTAssertEqual(try draft.load(), ["gender": 1])
    }

    func testPublicPreviewOmitsEveryUnpublishedScaleAndKeepsExactBirthParts() throws {
        let empty = try ProfilePublicPreview(draft: [:])
        XCTAssertTrue(empty.fields.isEmpty)
        XCTAssertEqual(empty.birth, ProfileBirthParts(year: nil, month: nil, day: nil))
        XCTAssertNil(empty.youngest)
        XCTAssertNil(empty.oldest)

        let scale = try XCTUnwrap(profileScales().first { $0.key != "gender" })
        let choice = try XCTUnwrap(scale.choices.first)
        let preview = try ProfilePublicPreview(draft: [
            scale.key: choice.code,
            "gender": 0,
            "birthMonth": 2,
            "birthDay": 29,
        ])
        XCTAssertEqual(preview.fields, [ProfilePublicField(scaleKey: scale.key, choiceKey: choice.key)])
        XCTAssertEqual(preview.birth, ProfileBirthParts(year: nil, month: 2, day: 29))
        XCTAssertNil(preview.youngest)
        XCTAssertNil(preview.oldest)

        let aged = try ProfilePublicPreview(draft: ["birthYear": 1_950])
        XCTAssertNotNil(aged.youngest)
        XCTAssertNotNil(aged.oldest)
        XCTAssertLessThanOrEqual(try XCTUnwrap(aged.youngest), try XCTUnwrap(aged.oldest))
        XCTAssertThrowsError(try ProfilePublicPreview(draft: [scale.key: UInt32.max]))
        XCTAssertThrowsError(try ProfilePublicPreview(draft: ["unknown": 1]))
    }

    func testBirthPartsStayOptionalAndUseSharedCoreValidation() throws {
        let empty = try ProfileBirthParts(yearText: "", monthText: "", dayText: "")
        XCTAssertEqual(empty, ProfileBirthParts(year: nil, month: nil, day: nil))
        try empty.validate()
        XCTAssertEqual(empty.inserted(into: ["gender": 0]), ["gender": 0])

        let yearless = try ProfileBirthParts(yearText: "", monthText: "2", dayText: "29")
        try yearless.validate()
        XCTAssertEqual(yearless.inserted(into: [:]), ["birthMonth": 2, "birthDay": 29])
        XCTAssertThrowsError(try ProfileBirthParts(yearText: "no", monthText: "", dayText: ""))
        let impossible = try ProfileBirthParts(yearText: "1900", monthText: "2", dayText: "29")
        XCTAssertThrowsError(try impossible.validate())
    }

    func testPeerNamedDraftCopiesIntoSlotWithoutChangingSource() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("dyapp-profile-test-\(UUID().uuidString)", isDirectory: true)
        let slots = IdentitySlotStore(root: root, legacyStore: PlatformKeyStore.forSlot(UUID()))
        let slot = try slots.create()
        let old = root.appendingPathComponent("profile-draft-\(slot.peerId).json")
        try JSONEncoder().encode(["zodiac": UInt32(3)]).write(to: old)
        let candidate = IdentityCandidate(location: .slot(slot.slot.id), state: .ready)
        let draft = ProfileDraftStore(root: root, candidate: candidate, peerId: slot.peerId)

        XCTAssertEqual(try draft.load(), ["zodiac": 3])
        XCTAssertTrue(FileManager.default.fileExists(atPath: draft.file.path))
        try draft.save(["zodiac": 4])
        XCTAssertEqual(try draft.load(), ["zodiac": 4])
        XCTAssertEqual(try JSONDecoder().decode([String: UInt32].self, from: Data(contentsOf: old)),
                       ["zodiac": 3])
        try slots.remove(slot.slot.id, confirming: slot.peerId)
        XCTAssertFalse(FileManager.default.fileExists(atPath: draft.file.path))
        XCTAssertTrue(FileManager.default.fileExists(atPath: old.path))
    }

    func testInvalidBirthCannotReplaceSavedDraft() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("dyapp-profile-test-\(UUID().uuidString)", isDirectory: true)
        let slots = IdentitySlotStore(root: root, legacyStore: PlatformKeyStore.forSlot(UUID()))
        let slot = try slots.create()
        let candidate = IdentityCandidate(location: .slot(slot.slot.id), state: .ready)
        let draft = ProfileDraftStore(root: root, candidate: candidate, peerId: slot.peerId)
        try draft.save([:], birth: ProfileBirthParts(year: nil, month: nil, day: nil))
        XCTAssertEqual(try draft.load(), [:])
        let saved = try Data(contentsOf: draft.file)
        let impossible = ProfileBirthParts(year: 1_900, month: 2, day: 29)
        XCTAssertThrowsError(try draft.save(["gender": 1], birth: impossible))
        XCTAssertEqual(try Data(contentsOf: draft.file), saved)
    }
}
