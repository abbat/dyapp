import CoreGraphics
@testable import DYApp
import XCTest

final class ProfilePhotoEditTests: XCTestCase {
    func testStaleConfirmationRefreshesWithoutRemovingTheNewOccupant() throws {
        try withPhotos { dir, key in
            let shown = try profilePhotoSnapshot(dir: dir, key: key)
            let confirmation = ProfilePhotoRemoval(index: 0, revision: shown.revision)
            let reordered = try editProfilePhotos(
                dir: dir, key: key, expectedRevision: shown.revision, edit: .move(from: 0, to: 1))
            var calls = 0
            let refreshed = try profilePhotoRows(dir: dir, key: key) { dir, key in
                calls += 1
                return try editProfilePhotos(
                    dir: dir, key: key, expectedRevision: confirmation.revision,
                    edit: .remove(index: confirmation.index))
            }
            XCTAssertEqual(calls, 1, "Never retry the positional removal after refreshing")
            XCTAssertTrue(refreshed.conflict)
            XCTAssertEqual(refreshed.hashes, reordered.thumbnails)
            XCTAssertEqual(refreshed.revision, reordered.revision)
            XCTAssertEqual(refreshed.previews.count, 2)
            XCTAssertEqual(
                try profilePhotoSnapshot(dir: dir, key: key).revision, reordered.revision)
        }
    }

    func testStaleMoveAfterRemovalRefreshesAndCurrentRevisionStillWorks() throws {
        try withPhotos { dir, key in
            let shown = try profilePhotoSnapshot(dir: dir, key: key)
            let removed = try editProfilePhotos(
                dir: dir, key: key, expectedRevision: shown.revision, edit: .remove(index: 0))
            let refreshed = try profilePhotoRows(dir: dir, key: key) { dir, key in
                try editProfilePhotos(
                    dir: dir, key: key, expectedRevision: shown.revision,
                    edit: .move(from: 0, to: 1))
            }
            XCTAssertTrue(refreshed.conflict)
            XCTAssertEqual(refreshed.hashes, removed.thumbnails)
            let committed = try profilePhotoRows(dir: dir, key: key) { dir, key in
                try editProfilePhotos(
                    dir: dir, key: key, expectedRevision: refreshed.revision,
                    edit: .remove(index: 0))
            }
            XCTAssertFalse(committed.conflict)
            XCTAssertTrue(committed.hashes.isEmpty)
            XCTAssertTrue(committed.previews.isEmpty)
        }
    }

    func testTypedStorageFailuresKeepTheirExistingStatus() {
        XCTAssertEqual(
            photoErrorKey(ProfilePhotoEditError.Failed(reason: "photoRefused")),
            "profile.photoRefused")
        XCTAssertEqual(
            photoErrorKey(ProfilePhotoEditError.Failed(reason: "corrupted")),
            "profile.photoUnsupported")
        XCTAssertEqual(
            photoErrorKey(ProfilePhotoEditError.Failed(reason: "disk full")), "profile.photoFailed")
    }

    private func withPhotos(_ body: (String, IdentityKey) throws -> Void) throws {
        let root = try identitySlotTestRoot()
        let store = IdentitySlotStore(root: root, legacyStore: .forSlot(UUID()))
        let created = try store.create()
        try store.withIdentity(.slot(created.slot.id), peerId: created.peerId) { dir, key in
            for red: CGFloat in [0.2, 0.8] {
                let photo = try PhotoPrep.prepare(
                    PhotoPrep.samplePNG(width: 64, height: 48, red: red, alpha: 1))
                _ = try addProfilePhoto(
                    dir: dir, key: key, full: photo.full, thumbnail: photo.thumbnail)
            }
            try body(dir, key)
        }
    }
}
