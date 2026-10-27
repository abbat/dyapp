@testable import DYApp
import Foundation
import XCTest

final class ProfileDraftStoreTests: XCTestCase {
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
}
