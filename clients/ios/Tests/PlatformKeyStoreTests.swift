@testable import DYApp
import Foundation
import Security
import XCTest

final class PlatformKeyStoreTests: XCTestCase {
    private func slot(_ last: String) throws -> UUID {
        try XCTUnwrap(UUID(uuidString: "00000000-0000-4000-8000-00000000000\(last)"))
    }

    func testSlotAccountsWrapIndependentlyAndLegacyAccountStillOpens() throws {
        let alice = PlatformKeyStore.forSlot(try slot("1"))
        let bob = PlatformKeyStore.forSlot(try slot("2"))
        let secret = Data("independent identity".utf8)
        let aliceBlob = try alice.wrap(secret: secret)
        let bobBlob = try bob.wrap(secret: secret)
        XCTAssertEqual(try alice.unwrap(wrapped: aliceBlob), secret)
        XCTAssertEqual(try bob.unwrap(wrapped: bobBlob), secret)
        XCTAssertThrowsError(try alice.unwrap(wrapped: bobBlob))
        XCTAssertThrowsError(try bob.unwrap(wrapped: aliceBlob))

        let legacyBlob = try PlatformKeyStore.shared.wrap(secret: secret)
        XCTAssertEqual(try PlatformKeyStore.shared.unwrap(wrapped: legacyBlob), secret)
        XCTAssertThrowsError(try alice.unwrap(wrapped: legacyBlob))
    }

    func testMissingSlotUnwrapDoesNotCreateAnAccount() throws {
        let missing = try slot("3")
        let blob = try PlatformKeyStore.forSlot(slot("1")).wrap(secret: Data([1]))
        XCTAssertThrowsError(try PlatformKeyStore.forSlot(missing).unwrap(wrapped: blob))
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: "com.dyapp.identity-wrap",
            kSecAttrAccount as String: "profile:\(missing.uuidString.lowercased())",
        ]
        XCTAssertEqual(SecItemCopyMatching(query as CFDictionary, nil), errSecItemNotFound)
    }
}
