@testable import DYApp
import XCTest

final class ChatsTests: XCTestCase {
    func testWrappedKeyFileIsMadeOnce() throws {
        let file = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        addTeardownBlock { try? FileManager.default.removeItem(at: file) }
        XCTAssertEqual(try wrappedKeyFile(file) { Data([1]) }, Data([1]))
        XCTAssertEqual(try wrappedKeyFile(file) { Data([2]) }, Data([1]), "a saved key is never replaced")
    }

    func testProfileLoadNamesAMissingConnection() {
        XCTAssertEqual(profileLoadKey(CoreError.Failed(message: "notConnected")), "search.notConnected")
        XCTAssertEqual(profileLoadKey(CoreError.Failed(message: "other")), "view.loadFailed")
    }
}
