@testable import DYApp
import Foundation
import XCTest

final class ProfileDraftStoreTests: XCTestCase {
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
